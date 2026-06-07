//! `KeychainAccess` trait + `OsKeychain` skeleton + `FakeKeychain`
//! for tests.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::AppResult;

/// Abstraction over the OS keychain. Production uses [`OsKeychain`]
/// (which delegates to the `keyring` crate). Tests inject
/// [`FakeKeychain`].
pub trait KeychainAccess: Send + Sync {
    /// Get the value for `account`, or `None` if no entry exists.
    fn get(&self, account: &str) -> AppResult<Option<String>>;
    /// Set `value` for `account`, overwriting any existing entry.
    fn set(&self, account: &str, value: &str) -> AppResult<()>;
    /// Delete the entry for `account`. No-op if no entry exists.
    fn delete(&self, account: &str) -> AppResult<()>;
}

/// Production keychain implementation. Backed by the `keyring`
/// crate (service name `"agentyx"`).
#[cfg(feature = "keyring")]
pub struct OsKeychain;

#[cfg(feature = "keyring")]
impl KeychainAccess for OsKeychain {
    fn get(&self, account: &str) -> AppResult<Option<String>> {
        let entry = keyring::Entry::new("agentyx", account).map_err(|e| crate::AppError::Io {
            op: format!("keychain entry {account}"),
            reason: e.to_string(),
        })?;
        match entry.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(crate::AppError::Io {
                op: format!("keychain get {account}"),
                reason: e.to_string(),
            }),
        }
    }

    fn set(&self, account: &str, value: &str) -> AppResult<()> {
        let entry = keyring::Entry::new("agentyx", account).map_err(|e| crate::AppError::Io {
            op: format!("keychain entry {account}"),
            reason: e.to_string(),
        })?;
        entry.set_password(value).map_err(|e| crate::AppError::Io {
            op: format!("keychain set {account}"),
            reason: e.to_string(),
        })
    }

    fn delete(&self, account: &str) -> AppResult<()> {
        let entry = keyring::Entry::new("agentyx", account).map_err(|e| crate::AppError::Io {
            op: format!("keychain entry {account}"),
            reason: e.to_string(),
        })?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(crate::AppError::Io {
                op: format!("keychain delete {account}"),
                reason: e.to_string(),
            }),
        }
    }
}

/// In-memory keychain for tests. Stores entries in a `HashMap`
/// behind a `Mutex`. Cheap to clone.
#[derive(Clone, Default)]
pub struct FakeKeychain {
    inner: Arc<Mutex<HashMap<String, String>>>,
}

impl FakeKeychain {
    /// Create an empty fake keychain.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pre-populate with `entries`. Used by tests that need a
    /// pre-set secret.
    #[must_use]
    pub fn with_entries(entries: &[(&str, &str)]) -> Self {
        let mut m = HashMap::new();
        for (k, v) in entries {
            m.insert((*k).to_string(), (*v).to_string());
        }
        Self {
            inner: Arc::new(Mutex::new(m)),
        }
    }
}

impl KeychainAccess for FakeKeychain {
    fn get(&self, account: &str) -> AppResult<Option<String>> {
        // Poison recovery: a test panic while holding the lock
        // would otherwise kill the in-memory keychain for the
        // rest of the process. Recovering is safe because
        // FakeKeychain is per-test and never shared.
        let guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(guard.get(account).cloned())
    }

    fn set(&self, account: &str, value: &str) -> AppResult<()> {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.insert(account.to_string(), value.to_string());
        Ok(())
    }

    fn delete(&self, account: &str) -> AppResult<()> {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.remove(account);
        Ok(())
    }
}
