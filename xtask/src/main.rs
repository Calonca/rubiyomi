// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

// Bump to the newest jmdict-simplified English release before each Rubiyomi release.
const JMDICT_VERSION: &str = "3.6.2+20261019180022";
const JMDICT_URL: &str =
    "https://github.com/scriptin/jmdict-simplified/releases/download/3.6.2%2B20261019180022/jmdict-eng-3.6.2%2B20261019180022.json.tgz";
const JMDICT_SHA256: &str = "1dca86908663dcddb8c3bcff9dc5e2bc73037bbdeed9c705c896a3845e7635a5";

const IPADIC_VERSION: &str = "2.7.0-20250920";

const DATA_LICENCE: &str = "\
jmdict.sqlite is an index built by Rubiyomi's `cargo xtask dict` from JMdict (English),
through the jmdict-simplified release named in version.json. No copyright is claimed
over the data.

JMdict is the property of the Electronic Dictionary Research and Development Group
(https://www.edrdg.org/), and is used in conformance with the Group's licence
(https://www.edrdg.org/edrdg/licence.html). Project page:
https://www.edrdg.org/wiki/index.php/JMdict-EDICT_Dictionary_Project

The data, and this index derived from it, are licensed under the Creative Commons
Attribution-ShareAlike 4.0 International licence (CC BY-SA 4.0):
https://creativecommons.org/licenses/by-sa/4.0/

ipadic.tar holds mecab-ipadic, compiled by lindera; its notice, with the NO WARRANTY
section, is in THIRD_PARTY_NOTICES.md.
";

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("dict") => dict(),
        _ => {
            eprintln!("usage: cargo xtask dict");
            std::process::exit(2);
        }
    }
}

fn run(cmd: &mut Command) -> Result<()> {
    let status = cmd.status()?;
    if !status.success() {
        return Err(format!("{cmd:?} failed").into());
    }
    Ok(())
}

fn dict() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().ok_or("no workspace root")?.to_path_buf();
    let build = root.join("dist/build");
    let out = root.join("dist/dict");
    std::fs::create_dir_all(&build)?;
    std::fs::create_dir_all(&out)?;

    let date = jmdict(&build)?;
    ipadic(&root, &build)?;
    for (from, to) in [("jmdict.sqlite", "jmdict.sqlite.xz"), ("ipadic.tar", "ipadic.tar.xz")] {
        eprintln!("compressing {from}");
        run(Command::new("xz").args(["-9e", "-T0", "-k", "-f", "-c"]).arg(build.join(from)).stdout(std::fs::File::create(out.join(to))?))?;
    }
    let version = serde_json::json!({
        "format": rubiyomi_core::dict::FORMAT,
        "jmdict": JMDICT_VERSION,
        "jmdict_date": date,
        "jmdict_licence": rubiyomi_core::dict::jmdict::LICENCE,
        "ipadic": IPADIC_VERSION,
    });
    std::fs::write(out.join("version.json"), serde_json::to_string_pretty(&version)?)?;
    std::fs::write(out.join("DATA-LICENCE.txt"), DATA_LICENCE)?;
    for f in ["jmdict.sqlite.xz", "ipadic.tar.xz", "version.json", "DATA-LICENCE.txt"] {
        eprintln!("{:>12} bytes  dist/dict/{f}", std::fs::metadata(out.join(f))?.len());
    }
    Ok(())
}

fn jmdict(build: &Path) -> Result<String> {
    let tgz = build.join("jmdict-eng.json.tgz");
    let ok = |p: &Path| std::fs::read(p).ok().is_some_and(|b| hex(&Sha256::digest(&b)) == JMDICT_SHA256);
    if !ok(&tgz) {
        eprintln!("downloading {JMDICT_URL}");
        run(Command::new("curl").args(["-fL", "--retry", "3", "-o"]).arg(&tgz).arg(JMDICT_URL))?;
        if !ok(&tgz) {
            return Err("the JMdict download doesn't match its checksum".into());
        }
    }
    let unpacked = build.join("jmdict-eng");
    let _ = std::fs::remove_dir_all(&unpacked);
    std::fs::create_dir_all(&unpacked)?;
    run(Command::new("tar").arg("-xzf").arg(&tgz).arg("-C").arg(&unpacked))?;
    let json = std::fs::read_dir(&unpacked)?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x == "json"))
        .ok_or("the JMdict archive holds no JSON")?;
    eprintln!("building the JMdict index");
    let info = rubiyomi_core::dict::jmdict::build(std::fs::File::open(&json)?, &build.join("jmdict.sqlite"), |n| {
        if n % 50_000 == 0 {
            eprintln!("  {n} words");
        }
    })?;
    eprintln!("  {} words, JMdict of {}", info.words, info.date);
    Ok(info.date)
}

fn ipadic(root: &Path, build: &Path) -> Result<()> {
    let cache = build.join("lindera");
    eprintln!("building IPADIC with lindera");
    run(Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .current_dir(root)
        .args(["build", "-q", "--release", "-p", "lindera-ipadic", "--target-dir"])
        .arg(build.join("target"))
        .env("LINDERA_BUILD_DICTIONARY_CACHE_DIR", &cache))?;
    let dir = find_dir(&cache, "lindera-ipadic").ok_or("lindera left no lindera-ipadic directory")?;
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)?.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
    files.sort();
    run(Command::new("tar").arg("-cf").arg(build.join("ipadic.tar")).arg("-C").arg(&dir).args(files.iter().filter_map(|p| p.file_name())))
}

fn find_dir(base: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(base).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == name) && path.join("dict.words").exists() {
                return Some(path);
            }
            if let Some(found) = find_dir(&path, name) {
                return Some(found);
            }
        }
    }
    None
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
