//! Resumable downloads: `<file>.part` grows until it is complete, its
//! SHA-256 (when known) is checked, then it is renamed into place. A
//! cancelled download keeps its `.part` and resumes with a Range request;
//! a failed checksum deletes it so the next try starts clean.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use reqwest::StatusCode;
use reqwest::blocking::Client;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DlError {
    Cancelled,
    Network,
    /// The server answered with this status.
    Status(u16),
    /// Size or SHA-256 differ from what the source publishes.
    Checksum,
    Disk(String),
}

impl std::fmt::Display for DlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DlError::Cancelled => write!(f, "cancelled"),
            DlError::Network => write!(f, "network error"),
            DlError::Status(s) => write!(f, "HTTP {s}"),
            DlError::Checksum => write!(f, "checksum mismatch"),
            DlError::Disk(e) => write!(f, "disk: {e}"),
        }
    }
}

fn disk(e: std::io::Error) -> DlError {
    DlError::Disk(e.to_string())
}

/// A client for big downloads: no overall timeout, only a stall timeout.
pub fn client() -> Client {
    Client::builder()
        .connect_timeout(Duration::from_secs(20))
        // reqwest's blocking client has no read timeout: a stalled body read
        // ends when the connection does (TCP keep-alive), and the user can
        // cancel any time.
        .tcp_keepalive(Duration::from_secs(30))
        .timeout(None)
        .user_agent(concat!("HeliLingo/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("HTTP client")
}

pub fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    dest.with_file_name(name)
}

/// Downloads `url` to `dest`. `size` / `sha256`: what the source publishes.
/// `progress` gets the bytes written so far (including a resumed part).
pub fn fetch(
    http: &Client,
    url: &str,
    dest: &Path,
    size: Option<u64>,
    sha256: Option<&str>,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64),
) -> Result<(), DlError> {
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).map_err(disk)?;
    }
    // Already complete (verified before it was renamed into place).
    if let (Ok(m), Some(size)) = (std::fs::metadata(dest), size)
        && m.len() == size
    {
        progress(size);
        return Ok(());
    }
    let part = part_path(dest);
    let mut have = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    if size.is_some_and(|s| have > s) {
        have = 0;
    }
    let mut hasher = Sha256::new();
    if have > 0 && sha256.is_some() {
        // Resume: the hash has to cover what's already there.
        hash_file(&part, &mut hasher, cancel).map_err(disk)?;
        if cancel.load(Ordering::Relaxed) {
            return Err(DlError::Cancelled);
        }
    }

    let complete = size.is_some_and(|s| have == s);
    if !complete {
        let mut req = http.get(url);
        if have > 0 {
            req = req.header("Range", format!("bytes={have}-"));
        }
        let mut resp = req.send().map_err(|_| DlError::Network)?;
        let status = resp.status();
        let mut file = if status == StatusCode::PARTIAL_CONTENT && have > 0 {
            OpenOptions::new().append(true).open(&part).map_err(disk)?
        } else if status.is_success() {
            // The server ignored the range: start over.
            have = 0;
            hasher = Sha256::new();
            File::create(&part).map_err(disk)?
        } else if status == StatusCode::RANGE_NOT_SATISFIABLE {
            let _ = std::fs::remove_file(&part);
            return Err(DlError::Status(status.as_u16()));
        } else {
            return Err(DlError::Status(status.as_u16()));
        };
        progress(have);
        let mut buf = vec![0u8; 1 << 20];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(DlError::Cancelled);
            }
            let n = match resp.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => return Err(DlError::Network),
            };
            file.write_all(&buf[..n]).map_err(disk)?;
            if sha256.is_some() {
                hasher.update(&buf[..n]);
            }
            have += n as u64;
            progress(have);
        }
        file.flush().map_err(disk)?;
    }

    if size.is_some_and(|s| have != s) {
        // Short read (connection dropped): keep the part for a resume.
        return Err(if size.is_some_and(|s| have > s) { DlError::Checksum } else { DlError::Network });
    }
    if let Some(expected) = sha256 {
        let got = hex(&hasher.finalize());
        if !got.eq_ignore_ascii_case(expected) {
            let _ = std::fs::remove_file(&part);
            return Err(DlError::Checksum);
        }
    }
    let _ = std::fs::remove_file(dest);
    std::fs::rename(&part, dest).map_err(disk)?;
    Ok(())
}

fn hash_file(path: &Path, hasher: &mut Sha256, cancel: &AtomicBool) -> std::io::Result<()> {
    let mut f = File::open(path)?;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        let n = f.read(&mut buf)?;
        if n == 0 {
            return Ok(());
        }
        hasher.update(&buf[..n]);
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-256 of a file (tests).
#[cfg(test)]
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut h = Sha256::new();
    hash_file(path, &mut h, &AtomicBool::new(false))?;
    Ok(hex(&h.finalize()))
}

/// Unzips `zip` into `dest`. With `strip_top`, the archive's single top
/// folder is dropped (Argos packages: `translate-en_ru-1_9/...`). Paths
/// that would leave `dest` are skipped.
pub fn unzip(zip: &Path, dest: &Path, strip_top: bool) -> Result<(), DlError> {
    let file = File::open(zip).map_err(disk)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|_| DlError::Checksum)?;
    std::fs::create_dir_all(dest).map_err(disk)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|_| DlError::Checksum)?;
        let Some(rel) = entry.enclosed_name() else { continue };
        let rel: PathBuf = if strip_top { rel.components().skip(1).collect() } else { rel };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out = dest.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(disk)?;
            continue;
        }
        if let Some(dir) = out.parent() {
            std::fs::create_dir_all(dir).map_err(disk)?;
        }
        let mut f = File::create(&out).map_err(disk)?;
        // A corrupt entry fails its CRC check here.
        std::io::copy(&mut entry, &mut f).map_err(|_| DlError::Checksum)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_hash() {
        let dir = std::env::temp_dir().join(format!("hl-dl-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.txt");
        std::fs::write(&f, b"abc").unwrap();
        assert_eq!(sha256_file(&f).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(part_path(&f), dir.join("a.txt.part"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unzip_strips_the_top_folder() {
        let dir = std::env::temp_dir().join(format!("hl-zip-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let zip_path = dir.join("p.zip");
        {
            let mut w = zip::ZipWriter::new(File::create(&zip_path).unwrap());
            let opts = zip::write::SimpleFileOptions::default();
            w.add_directory("translate-en_ru-1_9/", opts).unwrap();
            w.start_file("translate-en_ru-1_9/model/config.json", opts).unwrap();
            w.write_all(b"{}").unwrap();
            w.start_file("translate-en_ru-1_9/sentencepiece.model", opts).unwrap();
            w.write_all(b"spm").unwrap();
            w.finish().unwrap();
        }
        let out = dir.join("en_ru");
        unzip(&zip_path, &out, true).unwrap();
        assert_eq!(std::fs::read(out.join("model").join("config.json")).unwrap(), b"{}");
        assert!(out.join("sentencepiece.model").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cancelled_before_start() {
        let dir = std::env::temp_dir().join(format!("hl-cancel-test-{}", std::process::id()));
        let dest = dir.join("x.bin");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(part_path(&dest), b"12").unwrap();
        let cancel = AtomicBool::new(true);
        // Resume hashing stops at once; nothing is fetched.
        let r = fetch(&client(), "http://127.0.0.1:9/never", &dest, Some(10), Some("00"), &cancel, |_| {});
        assert_eq!(r, Err(DlError::Cancelled));
        // The part stays for a later resume.
        assert!(part_path(&dest).is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
