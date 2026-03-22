use crate::auth::SessionTokenResponse;
use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

const SERVICE_NAME: &str = "sync-devices";
const SESSION_ACCOUNT: &str = "session";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSession {
    pub api_base_url: String,
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub scope: String,
    pub user: crate::auth::SessionUser,
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
        api_base_url: &str,
        session: &SessionTokenResponse,
    ) -> Result<(), SessionStoreError> {
        let stored = StoredSession {
            api_base_url: api_base_url.to_string(),
            access_token: session.access_token.clone(),
            token_type: session.token_type.clone(),
            expires_in: session.expires_in,
            scope: session.scope.clone(),
            user: session.user.clone(),
            stored_at: unix_timestamp_now()?,
        };
        let payload = serde_json::to_string(&stored)?;
        self.entry.set_password(&payload)?;
        Ok(())
    }

    pub fn load(&self) -> Result<Option<StoredSession>, SessionStoreError> {
        match self.entry.get_password() {
            Ok(payload) => Ok(Some(serde_json::from_str(&payload)?)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(SessionStoreError::Keyring(error)),
        }
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
