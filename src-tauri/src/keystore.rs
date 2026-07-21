//! OS keyring storage for ServerKit API keys (plan 25).
//!
//! Keys live in the platform credential store — Windows Credential Manager,
//! macOS Keychain, Linux Secret Service — under service `localkit`, account
//! `connection/<id>`. This replaces the plaintext `serverkit_connections.api_key`
//! column as the source of truth; `db.rs` migrates legacy plaintext keys into
//! the keyring the first time a connection is read.
//!
//! **Graceful degradation is the whole contract.** On a machine with no
//! keyring — headless Linux, a locked keychain, or the `LOCALKIT_DISABLE_KEYRING`
//! escape hatch — every call here is a no-op that returns "not stored", so the
//! caller falls back to the SQLite column instead of failing hard. `lk` on a
//! server keeps working; the only cost is the key sits in the DB as before.

use std::sync::atomic::{AtomicBool, Ordering};

/// Credential-store service name. The account within it is `connection/<id>`.
const SERVICE: &str = "localkit";

/// So the "keyring unavailable" note is logged once per process, not per read.
static WARNED: AtomicBool = AtomicBool::new(false);

/// Force the SQLite fallback. Set for hermetic tests and by anyone who would
/// rather keep keys in the local DB (e.g. a shared service account on a box
/// whose keyring can't be unlocked non-interactively).
fn disabled() -> bool {
    std::env::var_os("LOCALKIT_DISABLE_KEYRING").is_some()
}

fn account(connection_id: &str) -> String {
    format!("connection/{connection_id}")
}

fn entry(connection_id: &str) -> Option<keyring::Entry> {
    if disabled() {
        return None;
    }
    match keyring::Entry::new(SERVICE, &account(connection_id)) {
        Ok(e) => Some(e),
        Err(e) => {
            warn_once(&e.to_string());
            None
        }
    }
}

fn warn_once(msg: &str) {
    if !WARNED.swap(true, Ordering::Relaxed) {
        eprintln!(
            "[keystore] OS keyring unavailable — ServerKit API keys will be kept \
             in the local database instead: {msg}"
        );
    }
}

/// Store `key` for `connection_id`. Returns `true` only when it actually
/// landed in the keyring; `false` tells the caller to fall back to SQLite.
pub fn store(connection_id: &str, key: &str) -> bool {
    let Some(entry) = entry(connection_id) else {
        return false;
    };
    match entry.set_password(key) {
        Ok(()) => true,
        Err(e) => {
            warn_once(&e.to_string());
            false
        }
    }
}

/// Retrieve the key for `connection_id`, or `None` if it isn't in the keyring
/// (never stored there, or the keyring is unavailable).
pub fn retrieve(connection_id: &str) -> Option<String> {
    let entry = entry(connection_id)?;
    match entry.get_password() {
        Ok(k) => Some(k),
        Err(keyring::Error::NoEntry) => None,
        Err(e) => {
            warn_once(&e.to_string());
            None
        }
    }
}

/// Best-effort delete of a connection's key. A missing entry is success —
/// removing a key that was never in the keyring (SQLite-fallback machine) is a
/// no-op, not an error.
pub fn delete(connection_id: &str) {
    let Some(entry) = entry(connection_id) else {
        return;
    };
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(e) => warn_once(&e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With the escape hatch set, every operation degrades to the "not stored"
    /// path — this is exactly the headless-server contract, and it's what keeps
    /// the connection round-trip tests in `db.rs` hermetic on CI.
    #[test]
    fn disabled_keyring_is_a_no_op() {
        // SAFETY: single-threaded test; no other code reads the var concurrently.
        std::env::set_var("LOCALKIT_DISABLE_KEYRING", "1");
        assert!(!store("keystore-test-id", "secret"));
        assert_eq!(retrieve("keystore-test-id"), None);
        delete("keystore-test-id"); // must not panic
        std::env::remove_var("LOCALKIT_DISABLE_KEYRING");
    }
}
