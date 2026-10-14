//! Is the PC online? Windows' own connectivity level for the profile that
//! carries internet traffic, checked at most every 2 seconds. Without a
//! connection the router skips cloud engines at once (offline models still
//! work) instead of waiting for them to time out.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows::Networking::Connectivity::{NetworkConnectivityLevel, NetworkInformation};

static LAST: Mutex<Option<(Instant, bool)>> = Mutex::new(None);

pub fn online() -> bool {
    let mut last = LAST.lock().unwrap();
    if let Some((at, v)) = *last
        && at.elapsed() < Duration::from_secs(2)
    {
        return v;
    }
    let v = check();
    *last = Some((Instant::now(), v));
    v
}

/// No profile, or one with no access at all, is offline. Local-only or
/// captive-portal access still counts as online: the engines find out.
fn check() -> bool {
    let Ok(profile) = NetworkInformation::GetInternetConnectionProfile() else { return false };
    match profile.GetNetworkConnectivityLevel() {
        Ok(level) => level != NetworkConnectivityLevel::None,
        // The API itself failed: don't block the cloud on a guess.
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn answers_and_caches() {
        let a = super::online();
        assert_eq!(super::online(), a);
    }
}
