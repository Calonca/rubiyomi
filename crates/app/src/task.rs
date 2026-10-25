// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::sync::mpsc::{Receiver, TryRecvError, channel};

pub struct Task<T> {
    rx: Receiver<T>,
    done: bool,
}

impl<T: Send + 'static> Task<T> {
    pub fn spawn(ctx: &egui::Context, f: impl FnOnce() -> T + Send + 'static) -> Task<T> {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(f());
            ctx.request_repaint();
        });
        Task { rx, done: false }
    }

    pub fn poll(&mut self) -> Option<T> {
        if self.done {
            return None;
        }
        match self.rx.try_recv() {
            Ok(v) => {
                self.done = true;
                Some(v)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.done = true;
                None
            }
        }
    }

    pub fn is_done(&self) -> bool {
        self.done
    }
}

pub fn take<T: Send + 'static>(slot: &mut Option<Task<T>>) -> Option<T> {
    let value = slot.as_mut()?.poll();
    if value.is_some() || slot.as_ref().is_some_and(Task::is_done) {
        *slot = None;
    }
    value
}

pub fn drain<T: Send + 'static>(tasks: &mut Vec<Task<T>>) -> Vec<T> {
    let answers = tasks.iter_mut().filter_map(Task::poll).collect();
    tasks.retain(|t| !t.is_done());
    answers
}
