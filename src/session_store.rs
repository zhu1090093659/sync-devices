use crate::auth::CloudflareAccount;
use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

const SERVICE_NAME: &str = "sync-devices";
const SESSION_ACCOUNT: &str = "session";

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
}

pub struct SessionStore {
    entry: Entry,
}

impl SessionStore {
    pub fn new() -> Result<Self, SessionStoreError> {
        let entry = Entry::new(SERVICE_NAME, SESSION_ACCOUNT)?;
        Ok(Self { entry })
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
        self.entry.set_password(&payload)?;
        Ok(())
    }

    pub fn load(&self) -> Result<Option<StoredSession>, SessionStoreError> {
        match self.entry.get_password() {
            Ok(payload) => match serde_json::from_str::<StoredSession>(&payload) {
                Ok(session) => Ok(Some(session)),
                // Old session format (GitHub-based) or corrupted data
                Err(_) => Ok(None),
            },
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(SessionStoreError::Keyring(error)),
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
        self.entry.set_password(&payload)?;
        Ok(())
    }

    pub fn clear(&self) -> Result<bool, SessionStoreError> {
        match self.entry.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(error) => Err(SessionStoreError::Keyring(error)),
        }
    }
}

fn unix_timestamp_now() -> Result<u64, SessionStoreError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| SessionStoreError::Time(error.to_string()))
}
