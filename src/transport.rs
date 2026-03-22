use crate::auth::SessionUser;
use crate::model::SyncManifest;
use crate::session_store::{SessionStore, SessionStoreError, StoredSession};
use reqwest::{Client, Method, Response, StatusCode, Url};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;
use tokio::time::sleep;

const MAX_ATTEMPTS: usize = 2;
const RETRY_DELAY_MS: u64 = 300;
const CONNECT_TIMEOUT_SECS: u64 = 5;
const REQUEST_TIMEOUT_SECS: u64 = 15;

pub struct ApiTransport {
    http: Client,
    base_url: Url,
    session: StoredSession,
}

#[derive(Debug, Deserialize)]
pub struct SessionResponse {
    pub user: SessionUser,
    pub token: SessionMetadata,
}

#[derive(Debug, Deserialize)]
pub struct SessionMetadata {
    pub issuer: Option<String>,
    pub subject: Option<String>,
    pub issued_at: Option<u64>,
    pub expires_at: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RemoteConfigRecord {
    pub id: String,
    pub tool: String,
    pub category: String,
    pub rel_path: String,
    pub content: String,
    pub content_hash: String,
    pub last_modified: u64,
    pub device_id: String,
    pub is_device_specific: bool,
    pub updated_at: u64,
}

#[derive(Debug, Default, Clone)]
pub struct ConfigListFilters {
    pub tool: Option<String>,
    pub category: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ConfigUploadRequest {
    pub content: String,
    pub content_hash: Option<String>,
    pub last_modified: u64,
    pub device_id: Option<String>,
    pub is_device_specific: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: String,
    error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ConfigListResponse {
    items: Vec<RemoteConfigRecord>,
}

#[derive(Debug, Deserialize)]
struct ConfigItemResponse {
    item: RemoteConfigRecord,
}

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("No stored session found. Run `sync-devices login` first.")]
    MissingSession,
    #[error(transparent)]
    SessionStore(#[from] SessionStoreError),
    #[error("HTTP client initialization failed: {0}")]
    ClientBuild(reqwest::Error),
    #[error("Invalid API base URL: {0}")]
    InvalidBaseUrl(String),
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("API request failed with status {status}: {message}")]
    Api { status: StatusCode, message: String },
}

impl ApiTransport {
    pub fn from_session_store() -> Result<Self, TransportError> {
        let store = SessionStore::new()?;
        let session = store.load()?.ok_or(TransportError::MissingSession)?;
        let base_url = Url::parse(&session.api_base_url)
            .map_err(|error| TransportError::InvalidBaseUrl(error.to_string()))?;
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(TransportError::ClientBuild)?;

        Ok(Self {
            http,
            base_url,
            session,
        })
    }

    pub async fn get_session(&self) -> Result<SessionResponse, TransportError> {
        self.send_json(Method::GET, self.api_url("api/session")?, None::<&()>)
            .await
    }

    pub async fn get_manifest(&self) -> Result<SyncManifest, TransportError> {
        self.send_json(Method::GET, self.api_url("api/manifest")?, None::<&()>)
            .await
    }

    pub async fn list_configs(
        &self,
        filters: ConfigListFilters,
    ) -> Result<Vec<RemoteConfigRecord>, TransportError> {
        let mut url = self.api_url("api/configs")?;
        {
            let mut pairs = url.query_pairs_mut();
            if let Some(tool) = filters.tool.as_deref() {
                pairs.append_pair("tool", tool);
            }
            if let Some(category) = filters.category.as_deref() {
                pairs.append_pair("category", category);
            }
        }

        let response: ConfigListResponse = self.send_json(Method::GET, url, None::<&()>).await?;
        Ok(response.items)
    }

    pub async fn upload_config(
        &self,
        tool: &str,
        category: &str,
        rel_path: &str,
        payload: &ConfigUploadRequest,
    ) -> Result<RemoteConfigRecord, TransportError> {
        let mut url = self.base_url.clone();
        {
            let mut segments = url.path_segments_mut().map_err(|_| {
                TransportError::InvalidBaseUrl("Base URL cannot be a base.".to_string())
            })?;
            segments.pop_if_empty();
            segments.push("api");
            segments.push("configs");
            segments.push(tool);
            segments.push(category);
            for part in rel_path.split('/').filter(|part| !part.is_empty()) {
                segments.push(part);
            }
        }

        let response: ConfigItemResponse = self.send_json(Method::PUT, url, Some(payload)).await?;
        Ok(response.item)
    }

    pub async fn delete_config(&self, id: &str) -> Result<RemoteConfigRecord, TransportError> {
        let url = self.api_url(&format!("api/configs/{id}"))?;
        let response: ConfigItemResponse = self.send_json(Method::DELETE, url, None::<&()>).await?;
        Ok(response.item)
    }

    async fn send_json<T, B>(
        &self,
        method: Method,
        url: Url,
        body: Option<&B>,
    ) -> Result<T, TransportError>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let response = self.send(method, url, body).await?;
        Ok(response.json::<T>().await?)
    }

    async fn send<B>(
        &self,
        method: Method,
        url: Url,
        body: Option<&B>,
    ) -> Result<Response, TransportError>
    where
        B: Serialize + ?Sized,
    {
        for attempt in 1..=MAX_ATTEMPTS {
            let mut request = self
                .http
                .request(method.clone(), url.clone())
                .bearer_auth(&self.session.access_token);

            if let Some(payload) = body {
                request = request.json(payload);
            }

            let response = request.send().await?;
            if response.status().is_success() {
                return Ok(response);
            }

            if attempt < MAX_ATTEMPTS && should_retry(response.status()) {
                sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                continue;
            }

            let status = response.status();
            let message = read_error_message(response).await?;
            return Err(TransportError::Api { status, message });
        }

        unreachable!("retry loop must return or error");
    }

    fn api_url(&self, path: &str) -> Result<Url, TransportError> {
        self.base_url
            .join(path)
            .map_err(|error| TransportError::InvalidBaseUrl(error.to_string()))
    }
}

fn should_retry(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

async fn read_error_message(response: Response) -> Result<String, TransportError> {
    let text = response.text().await?;
    if text.trim().is_empty() {
        return Ok("empty response body".to_string());
    }

    if let Ok(error) = serde_json::from_str::<ErrorResponse>(&text) {
        return Ok(error.error_description.unwrap_or(error.error));
    }

    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    const LIVE_POLL_ATTEMPTS: usize = 20;
    const LIVE_POLL_DELAY_MS: u64 = 500;

    #[tokio::test]
    #[ignore = "requires a stored session and live backend access"]
    async fn live_roundtrip_config_api() -> Result<(), Box<dyn std::error::Error>> {
        if std::env::var("SYNC_DEVICES_RUN_LIVE_TESTS").as_deref() != Ok("1") {
            return Ok(());
        }

        let client = ApiTransport::from_session_store()?;
        let session = client.get_session().await?;
        assert!(!session.user.login.is_empty());

        delete_smoke_configs(&client).await?;
        let manifest_before = client.get_manifest().await?;
        let _ = manifest_before.items.len();
        let listed_before = client.list_configs(ConfigListFilters::default()).await?;
        let _ = listed_before.len();

        let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
        let rel_path = format!("transport-smoke/{}.json", now.as_millis());
        let created = client
            .upload_config(
                "codex",
                "settings",
                &rel_path,
                &ConfigUploadRequest {
                    content: "{\"transport\":\"ok\"}".to_string(),
                    content_hash: None,
                    last_modified: now.as_secs(),
                    device_id: Some("transport-smoke".to_string()),
                    is_device_specific: Some(false),
                },
            )
            .await?;
        assert_eq!(created.tool, "codex");
        assert_eq!(created.category, "settings");
        assert_eq!(created.rel_path, rel_path);

        let manifest_after_upload = client.get_manifest().await?;
        let _ = manifest_after_upload.items.len();
        let listed_after_upload = client.list_configs(ConfigListFilters::default()).await?;
        let _ = listed_after_upload.len();

        let deleted = delete_config_with_retry(&client, &created.id).await?;
        assert_eq!(deleted.id, created.id);

        let manifest_after_delete = client.get_manifest().await?;
        let _ = manifest_after_delete.items.len();
        let listed_after_delete = client.list_configs(ConfigListFilters::default()).await?;
        let _ = listed_after_delete.len();

        Ok(())
    }

    async fn delete_smoke_configs(client: &ApiTransport) -> Result<(), Box<dyn std::error::Error>> {
        let existing = list_smoke_configs(client).await?;
        for config in existing {
            delete_config_with_retry(client, &config.id).await?;
        }

        Ok(())
    }

    async fn delete_config_with_retry(
        client: &ApiTransport,
        config_id: &str,
    ) -> Result<RemoteConfigRecord, Box<dyn std::error::Error>> {
        for attempt in 0..LIVE_POLL_ATTEMPTS {
            match client.delete_config(config_id).await {
                Ok(record) => return Ok(record),
                Err(TransportError::Api { status, .. })
                    if status == StatusCode::NOT_FOUND && attempt + 1 < LIVE_POLL_ATTEMPTS =>
                {
                    sleep(Duration::from_millis(LIVE_POLL_DELAY_MS)).await;
                }
                Err(error) => return Err(error.into()),
            }
        }

        Err(format!("timed out deleting smoke config {config_id}").into())
    }

    async fn list_smoke_configs(
        client: &ApiTransport,
    ) -> Result<Vec<RemoteConfigRecord>, TransportError> {
        let configs = client
            .list_configs(ConfigListFilters {
                tool: Some("codex".to_string()),
                category: Some("settings".to_string()),
            })
            .await?;

        Ok(configs
            .into_iter()
            .filter(|config| config.rel_path.starts_with("transport-smoke/"))
            .collect())
    }
}
