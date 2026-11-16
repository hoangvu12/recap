//! API keys: in Windows Credential Manager ([`Keyring`]), never in files or
//! the database. The UI only ever sees [`mask`]ed keys.

use std::{collections::HashMap, sync::Mutex};

#[derive(Debug, thiserror::Error)]
#[error("credential store: {0}")]
pub struct SecretError(pub String);

/// Where keys are kept, by provider id.
pub trait Secrets: Send + Sync {
    fn get(&self, id: &str) -> Result<Option<String>, SecretError>;
    fn set(&self, id: &str, key: &str) -> Result<(), SecretError>;
    /// Forget `id`'s key (nothing to forget is fine).
    fn delete(&self, id: &str) -> Result<(), SecretError>;
}

/// Windows Credential Manager: generic credentials named `<service>:<id>`.
pub struct Keyring {
    service: String,
}

impl Keyring {
    /// The app's store (service "Recap").
    pub fn new() -> Self {
        Self::with_service("Recap")
    }

    /// A store of its own (an integration test's).
    pub fn with_service(service: impl Into<String>) -> Self {
        Self { service: service.into() }
    }
}

impl Default for Keyring {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(windows)]
impl Secrets for Keyring {
    fn get(&self, id: &str) -> Result<Option<String>, SecretError> {
        let entry = keyring::Entry::new(&self.service, id).map_err(|e| SecretError(e.to_string()))?;
        match entry.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SecretError(e.to_string())),
        }
    }

    fn set(&self, id: &str, key: &str) -> Result<(), SecretError> {
        let entry = keyring::Entry::new(&self.service, id).map_err(|e| SecretError(e.to_string()))?;
        entry.set_password(key).map_err(|e| SecretError(e.to_string()))
    }

    fn delete(&self, id: &str) -> Result<(), SecretError> {
        let entry = keyring::Entry::new(&self.service, id).map_err(|e| SecretError(e.to_string()))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(SecretError(e.to_string())),
        }
    }
}

#[cfg(not(windows))]
impl Secrets for Keyring {
    fn get(&self, _id: &str) -> Result<Option<String>, SecretError> {
        Err(SecretError("no credential store on this platform".into()))
    }
    fn set(&self, _id: &str, _key: &str) -> Result<(), SecretError> {
        Err(SecretError("no credential store on this platform".into()))
    }
    fn delete(&self, _id: &str) -> Result<(), SecretError> {
        Ok(())
    }
}

/// Keys in memory: tests.
#[derive(Default)]
pub struct MemorySecrets(Mutex<HashMap<String, String>>);

impl Secrets for MemorySecrets {
    fn get(&self, id: &str) -> Result<Option<String>, SecretError> {
        Ok(self.0.lock().expect("secrets").get(id).cloned())
    }
    fn set(&self, id: &str, key: &str) -> Result<(), SecretError> {
        self.0.lock().expect("secrets").insert(id.into(), key.into());
        Ok(())
    }
    fn delete(&self, id: &str) -> Result<(), SecretError> {
        self.0.lock().expect("secrets").remove(id);
        Ok(())
    }
}

const DOTS: &str = "••••";

/// A key as the UI shows it: its prefix (up to a `-` or `_` in its first six
/// characters: `sk_`, `sk-or-`), dots, and its last four. Short keys are
/// all dots.
pub fn mask(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    if chars.len() < 12 {
        return DOTS.into();
    }
    let prefix_len = chars.iter().take(6).rposition(|c| *c == '-' || *c == '_').map_or(0, |i| i + 1);
    let prefix: String = chars[..prefix_len].iter().collect();
    let last: String = chars[chars.len() - 4..].iter().collect();
    format!("{prefix}{DOTS}{last}")
}
