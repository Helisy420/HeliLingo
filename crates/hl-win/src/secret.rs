//! API keys in Windows Credential Manager (generic credentials named
//! `HeliLingo/<engine>`), for the current Windows user. Keys never touch
//! settings.json, logs, or the webview: windows only see [`hint`].

use windows::Win32::Foundation::FILETIME;
use windows::Win32::Security::Credentials::{
    CRED_FLAGS, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree, CredReadW,
    CredWriteW,
};
use windows::core::{HSTRING, PWSTR};

fn target(name: &str) -> HSTRING {
    HSTRING::from(format!("HeliLingo/{name}"))
}

/// The stored key, or `None`.
pub fn get(name: &str) -> Option<String> {
    let mut cred: *mut CREDENTIALW = std::ptr::null_mut();
    unsafe {
        CredReadW(&target(name), CRED_TYPE_GENERIC, None, &mut cred).ok()?;
        let c = &*cred;
        let bytes = std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize);
        let wide: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|b| u16::from_le_bytes(*b)).collect();
        CredFree(cred as *const _);
        let key = String::from_utf16(&wide).ok()?;
        (!key.trim().is_empty()).then(|| key.trim().to_owned())
    }
}

/// Stores `key`; an empty key deletes the credential.
pub fn set(name: &str, key: &str) -> bool {
    let key = key.trim();
    if key.is_empty() {
        delete(name);
        return true;
    }
    let target = target(name);
    let blob: Vec<u8> = key.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let user = HSTRING::from("HeliLingo");
    let cred = CREDENTIALW {
        Flags: CRED_FLAGS(0),
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(target.as_ptr() as *mut _),
        Comment: PWSTR::null(),
        LastWritten: FILETIME::default(),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_ptr() as *mut u8,
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        AttributeCount: 0,
        Attributes: std::ptr::null_mut(),
        TargetAlias: PWSTR::null(),
        UserName: PWSTR(user.as_ptr() as *mut _),
    };
    unsafe { CredWriteW(&cred, 0).is_ok() }
}

pub fn delete(name: &str) {
    unsafe {
        let _ = CredDeleteW(&target(name), CRED_TYPE_GENERIC, None);
    }
}

/// "•••• 4f2a": which key is set, without revealing it.
pub fn hint(key: &str) -> String {
    let tail: String = key.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    format!("•••• {tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let name = "test-roundtrip";
        assert!(set(name, " sk-test-1234 "));
        assert_eq!(get(name).as_deref(), Some("sk-test-1234"));
        assert!(set(name, ""));
        assert_eq!(get(name), None);
        assert_eq!(hint("sk-test-1234"), "•••• 1234");
    }
}
