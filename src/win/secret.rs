//! API keys at rest: encrypted with DPAPI for the current Windows user, so
//! settings.json never holds a key in plain text and a copied file is
//! useless on another account or machine.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};

/// Prefix of an encrypted value in settings.json.
const PREFIX: &str = "dpapi:";

/// A secret string (an API key). Serialized encrypted; `Debug` never shows
/// it, so keys can't end up in logs.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// The plain-text key, trimmed. Empty when not set.
    pub fn expose(&self) -> &str {
        self.0.trim()
    }

    pub fn is_set(&self) -> bool {
        !self.expose().is_empty()
    }

    /// Editable plain text, for the key field in Settings.
    pub fn text_mut(&mut self) -> &mut String {
        &mut self.0
    }

    /// "•••• 4f2a": the last four characters, for showing which key is set.
    pub fn hint(&self) -> String {
        let k = self.expose();
        let tail: String = k.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
        format!("•••• {tail}")
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.is_set() { "Secret(<set>)" } else { "Secret(<empty>)" })
    }
}

impl Serialize for Secret {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if !self.is_set() {
            return s.serialize_str("");
        }
        match protect(self.expose().as_bytes()) {
            Some(blob) => s.serialize_str(&format!("{PREFIX}{}", base64_encode(&blob))),
            // Never fall back to writing the key in plain text.
            None => s.serialize_str(""),
        }
    }
}

impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        Ok(match raw.strip_prefix(PREFIX) {
            Some(b64) => base64_decode(b64)
                .and_then(|blob| unprotect(&blob))
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .map(Secret)
                .unwrap_or_default(),
            // A plain value (hand-edited file): accepted, encrypted on next save.
            None => Secret(raw),
        })
    }
}

fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 }
}

/// Copies DPAPI's output and frees it.
fn take(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
    unsafe {
        let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(out.pbData as *mut _)));
        v
    }
}

fn protect(data: &[u8]) -> Option<Vec<u8>> {
    let input = blob(data);
    let mut out = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(&input, None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out).ok()?;
    }
    Some(take(out))
}

fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
    let input = blob(data);
    let mut out = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(&input, None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out).ok()?;
    }
    Some(take(out))
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes().filter(|&c| c != b'=') {
        let v = B64.iter().position(|&b| b == c)? as u32;
        acc = acc << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trip() {
        for data in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar", &[0, 255, 7, 128]] {
            assert_eq!(base64_decode(&base64_encode(data)).unwrap(), data);
        }
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
    }

    #[test]
    fn dpapi_round_trip() {
        let s = Secret::new("abc:fx");
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.starts_with("\"dpapi:") && !json.contains("abc"));
        let back: Secret = serde_json::from_str(&json).unwrap();
        assert_eq!(back.expose(), "abc:fx");
        assert_eq!(format!("{back:?}"), "Secret(<set>)");
    }
}
