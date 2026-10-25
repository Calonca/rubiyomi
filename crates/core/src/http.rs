// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::sync::{Arc, OnceLock};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub body: String,
    pub retry_after: Option<u64>,
}

impl Response {
    pub fn ok(body: impl Into<String>) -> Self {
        Response { status: 200, body: body.into(), retry_after: None }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unreachable(pub String);

pub trait Transport: Send + Sync {
    fn post(&self, request: &Request) -> Result<Response, Unreachable>;
}

struct Ureq(ureq::Agent);

impl Transport for Ureq {
    fn post(&self, r: &Request) -> Result<Response, Unreachable> {
        let mut b = self.0.post(&r.url);
        for (k, v) in &r.headers {
            b = b.header(k, v);
        }
        let mut response = b.send(r.body.as_bytes()).map_err(|e| Unreachable(e.to_string()))?;
        let status = response.status().as_u16();
        let retry_after = response.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| v.trim().parse().ok());
        let body = response.body_mut().read_to_string().unwrap_or_default();
        Ok(Response { status, body, retry_after })
    }
}

pub fn transport() -> Arc<dyn Transport> {
    static REAL: OnceLock<Arc<dyn Transport>> = OnceLock::new();
    REAL.get_or_init(|| {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_global(Some(Duration::from_secs(30)))
            .user_agent(concat!("Rubiyomi/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Arc::new(Ureq(agent))
    })
    .clone()
}

#[cfg(test)]
pub use mock::Mock;

#[cfg(test)]
mod mock {
    use std::sync::{Arc, Mutex};

    use super::{Request, Response, Transport, Unreachable};

    type Handler = Box<dyn Fn(&Request) -> Result<Response, Unreachable> + Send + Sync>;

    pub struct Mock {
        handler: Handler,
        requests: Mutex<Vec<Request>>,
    }

    impl Mock {
        pub fn new(handler: impl Fn(&Request) -> Result<Response, Unreachable> + Send + Sync + 'static) -> Arc<Mock> {
            Arc::new(Mock { handler: Box::new(handler), requests: Mutex::new(Vec::new()) })
        }

        pub fn requests(&self) -> Vec<Request> {
            self.requests.lock().unwrap().clone()
        }
    }

    impl Transport for Mock {
        fn post(&self, request: &Request) -> Result<Response, Unreachable> {
            self.requests.lock().unwrap().push(request.clone());
            (self.handler)(request)
        }
    }
}
