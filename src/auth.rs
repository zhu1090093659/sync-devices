use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use std::env;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::time::sleep;

const DEFAULT_API_BASE_URL: &str = "https://sync-devices-worker.1090093659.workers.dev";
const SLOW_DOWN_DELAY_SECONDS: u64 = 5;

pub struct DeviceFlowClient {
    http: Client,
    base_url: Url,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SessionTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub scope: String,
    pub user: SessionUser,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SessionUser {
    pub id: u64,
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DeviceCodePayload {
    Success(DeviceCodeResponse),
    Error(ApiErrorResponse),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SessionTokenPayload {
    Success(SessionTokenResponse),
    Error(ApiErrorResponse),
}

#[derive(Debug, Clone, Deserialize)]
struct ApiErrorResponse {
    error: String,
    error_description: Option<String>,
}

#[derive(Debug, Error)]
pub enum AuthClientError {
    #[error("Invalid API base URL: {0}")]
    InvalidBaseUrl(String),
    #[error("Authentication request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("The server returned an invalid JSON payload: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("{0}")]
    Server(String),
    #[error("Authorization was denied: {0}")]
    AuthorizationDenied(String),
    #[error("The device code expired before authorization completed: {0}")]
    AuthorizationExpired(String),
}

impl DeviceFlowClient {
    pub fn from_env() -> Result<Self, AuthClientError> {
        let raw = env::var("SYNC_DEVICES_API_BASE_URL")
            .unwrap_or_else(|_| DEFAULT_API_BASE_URL.to_string());
        let trimmed = raw.trim().trim_end_matches('/');
        if trimmed.is_empty() {
            return Err(AuthClientError::InvalidBaseUrl(
                "SYNC_DEVICES_API_BASE_URL must not be empty.".to_string(),
            ));
        }

        let base_url = Url::parse(trimmed)
            .map_err(|error| AuthClientError::InvalidBaseUrl(error.to_string()))?;

        Ok(Self {
            http: Client::new(),
            base_url,
        })
    }

    pub async fn request_device_code(&self) -> Result<DeviceCodeResponse, AuthClientError> {
        let url = self.api_url("api/auth/device/code")?;
        let response = self.http.post(url).send().await?;
        let status = response.status();
        let payload = serde_json::from_str::<DeviceCodePayload>(&response.text().await?)?;

        match payload {
            DeviceCodePayload::Success(body) if status.is_success() => Ok(body),
            DeviceCodePayload::Error(error) => Err(AuthClientError::Server(format!(
                "Device code request failed: {}",
                describe_api_error(&error)
            ))),
            DeviceCodePayload::Success(_) => Err(AuthClientError::Server(format!(
                "Device code request returned unexpected status {}.",
                status
            ))),
        }
    }

    pub async fn poll_for_session_token(
        &self,
        device_code: &DeviceCodeResponse,
    ) -> Result<SessionTokenResponse, AuthClientError> {
        let deadline = Instant::now() + Duration::from_secs(device_code.expires_in);
        let mut poll_interval = Duration::from_secs(device_code.interval.max(1));

        loop {
            if Instant::now() >= deadline {
                return Err(AuthClientError::AuthorizationExpired(
                    "GitHub device code timeout reached.".to_string(),
                ));
            }

            sleep(poll_interval).await;

            match self.exchange_device_code(&device_code.device_code).await? {
                SessionPollState::Authorized(session) => return Ok(session),
                SessionPollState::Pending => continue,
                SessionPollState::SlowDown => {
                    poll_interval += Duration::from_secs(SLOW_DOWN_DELAY_SECONDS);
                }
                SessionPollState::Denied(message) => {
                    return Err(AuthClientError::AuthorizationDenied(message));
                }
                SessionPollState::Expired(message) => {
                    return Err(AuthClientError::AuthorizationExpired(message));
                }
            }
        }
    }

    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    async fn exchange_device_code(
        &self,
        device_code: &str,
    ) -> Result<SessionPollState, AuthClientError> {
        let url = self.api_url("api/auth/device/token")?;
        let response = self
            .http
            .post(url)
            .json(&serde_json::json!({ "device_code": device_code }))
            .send()
            .await?;
        let status = response.status();
        let payload = serde_json::from_str::<SessionTokenPayload>(&response.text().await?)?;

        match payload {
            SessionTokenPayload::Success(body) if status.is_success() => {
                Ok(SessionPollState::Authorized(body))
            }
            SessionTokenPayload::Error(error) if status.is_success() => Ok(map_poll_error(error)),
            SessionTokenPayload::Error(error) => Err(AuthClientError::Server(format!(
                "Session token request failed: {}",
                describe_api_error(&error)
            ))),
            SessionTokenPayload::Success(_) => Err(AuthClientError::Server(format!(
                "Session token request returned unexpected status {}.",
                status
            ))),
        }
    }

    fn api_url(&self, path: &str) -> Result<Url, AuthClientError> {
        self.base_url
            .join(path)
            .map_err(|error| AuthClientError::InvalidBaseUrl(error.to_string()))
    }
}

enum SessionPollState {
    Authorized(SessionTokenResponse),
    Pending,
    SlowDown,
    Denied(String),
    Expired(String),
}

fn map_poll_error(error: ApiErrorResponse) -> SessionPollState {
    let message = describe_api_error(&error);
    match error.error.as_str() {
        "authorization_pending" => SessionPollState::Pending,
        "slow_down" => SessionPollState::SlowDown,
        "access_denied" => SessionPollState::Denied(message),
        "expired_token" => SessionPollState::Expired(message),
        _ => SessionPollState::Denied(message),
    }
}

fn describe_api_error(error: &ApiErrorResponse) -> String {
    error
        .error_description
        .clone()
        .unwrap_or_else(|| error.error.clone())
}
