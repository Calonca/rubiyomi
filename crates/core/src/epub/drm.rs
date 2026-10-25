// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::io::{Read, Seek};

use quick_xml::Reader;
use quick_xml::events::Event;

use super::{Archive, EpubError};

const FONT_OBFUSCATION: [&str; 2] = ["http://www.idpf.org/2008/embedding", "http://ns.adobe.com/pdf/enc#RC"];

pub(super) fn check<R: Read + Seek>(archive: &mut Archive<R>) -> Result<(), EpubError> {
    if archive.has("META-INF/rights.xml") {
        return Err(EpubError::Drm);
    }
    let Some(xml) = archive.read_string("META-INF/encryption.xml") else {
        return Ok(());
    };
    if encrypts_content(&xml) { Err(EpubError::Drm) } else { Ok(()) }
}

fn encrypts_content(xml: &str) -> bool {
    let mut reader = Reader::from_str(xml);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e) | Event::Empty(e)) if e.local_name().as_ref() == "EncryptionMethod" => {
                let algorithm = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.local_name().as_ref() == "Algorithm")
                    .and_then(|a| a.normalized_value(quick_xml::XmlVersion::Implicit1_0).ok().map(|v| v.into_owned()));
                match algorithm {
                    Some(a) if FONT_OBFUSCATION.contains(&a.trim()) => {}
                    _ => return true,
                }
            }
            Ok(Event::Eof) => return false,
            Err(_) => return true,
            _ => {}
        }
    }
}
