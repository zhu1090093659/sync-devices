use crate::auth::CloudflareAccount;
use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

const SERVICE_NAME: &str = "sync-devices";
const SESSION_ACCOUNT: &str = "session";
const SESSION_FILE_NAME: &str = "session.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSession {
    pub api_token: String,
    pub account_id: String,
    pub account_name: String,
    pub worker_url: Option<String>,
    pub stored_at: u64,
}

#[derive(Debug, Error)]
pub enum SessionStoreError {
    #[error("Keyring access failed: {0}")]
    Keyring(#[from] keyring::Error),
    #[error("Session serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("System time is invalid: {0}")]
    Time(String),
    #[error("File storage error: {0}")]
    Io(#[from] std::io::Error),
}

enum Backend {
    Keyring(Entry),
    File(PathBuf),
}

pub struct SessionStore {
    backend: Backend,
}

impl SessionStore {
    pub fn new() -> Result<Self, SessionStoreError> {
        // Try keyring first; probe with a read to detect platform failures early.
        if let Ok(entry) = Entry::new(SERVICE_NAME, SESSION_ACCOUNT) {
            match entry.get_password() {
                Ok(_) | Err(keyring::Error::NoEntry) => {
                    return Ok(Self {
                        backend: Backend::Keyring(entry),
                    });
                }
                Err(_) => {
                    // Keyring unavailable (e.g. headless Linux without D-Bus secret service).
                }
            }
        }

        // Fall back to file-based storage.
        let path = session_file_path()?;
        eprintln!(
            "Warning: system keyring unavailable, using file storage (~/.config/sync-devices/)."
        );
        Ok(Self {
            backend: Backend::File(path),
        })
    }

    pub fn save(
        &self,
        account: &CloudflareAccount,
        api_token: &str,
        worker_url: Option<&str>,
    ) -> Result<(), SessionStoreError> {
        let stored = StoredSession {
            api_token: api_token.to_string(),
            account_id: account.account_id.clone(),
            account_name: account.account_name.clone(),
            worker_url: worker_url.map(str::to_string),
            stored_at: unix_timestamp_now()?,
        };
        let payload = serde_json::to_string(&stored)?;

        match &self.backend {
            Backend::Keyring(entry) => {
                entry.set_password(&payload)?;
            }
            Backend::File(path) => {
                write_session_file(path, &payload)?;
            }
        }
        Ok(())
    }

    pub fn load(&self) -> Result<Option<StoredSession>, SessionStoreError> {
        let payload = match &self.backend {
            Backend::Keyring(entry) => match entry.get_password() {
                Ok(p) => p,
                Err(keyring::Error::NoEntry) => return Ok(None),
                Err(error) => return Err(SessionStoreError::Keyring(error)),
            },
            Backend::File(path) => match std::fs::read_to_string(path) {
                Ok(p) => p,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(SessionStoreError::Io(e)),
            },
        };

        match serde_json::from_str::<StoredSession>(&payload) {
            Ok(session) => Ok(Some(session)),
            // Old session format (GitHub-based) or corrupted data
            Err(_) => Ok(None),
        }
    }

    /// Update the worker_url in an existing session without re-validating the token.
    pub fn set_worker_url(&self, worker_url: &str) -> Result<(), SessionStoreError> {
        let mut session = self
            .load()?
            .ok_or_else(|| SessionStoreError::Time("No session to update".into()))?;
        session.worker_url = Some(worker_url.to_string());
        session.stored_at = unix_timestamp_now()?;
        let payload = serde_json::to_string(&session)?;

        match &self.backend {
            Backend::Keyring(entry) => {
                entry.set_password(&payload)?;
            }
            Backend::File(path) => {
                write_session_file(path, &payload)?;
            }
        }
        Ok(())
    }

    pub fn clear(&self) -> Result<bool, SessionStoreError> {
        match &self.backend {
            Backend::Keyring(entry) => match entry.delete_credential() {
                Ok(()) => Ok(true),
                Err(keyring::Error::NoEntry) => Ok(false),
                Err(error) => Err(SessionStoreError::Keyring(error)),
            },
            Backend::File(path) => match std::fs::remove_file(path) {
                Ok(()) => Ok(true),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(e) => Err(SessionStoreError::Io(e)),
            },
        }
    }
}

fn session_file_path() -> Result<PathBuf, SessionStoreError> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| SessionStoreError::Time("Cannot determine config directory".into()))?;
    Ok(config_dir.join("sync-devices").join(SESSION_FILE_NAME))
}

fn write_session_file(path: &PathBuf, payload: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, payload)?;
    // Restrict file permissions on Unix (owner read/write only).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn unix_timestamp_now() -> Result<u64, SessionStoreError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| SessionStoreError::Time(error.to_string()))
}
