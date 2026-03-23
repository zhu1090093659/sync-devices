use reqwest::multipart;
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use thiserror::Error;

const CF_API_BASE: &str = "https://api.cloudflare.com/client/v4";

#[derive(Debug, Error)]
pub enum CloudflareApiError {
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Cloudflare API error ({code}): {message}")]
    Api { code: u32, message: String },
    #[error("Unexpected API response: {0}")]
    UnexpectedResponse(String),
}

#[derive(Debug, Deserialize)]
struct CfEnvelope<T> {
    success: bool,
    result: Option<T>,
    #[serde(default)]
    errors: Vec<CfError>,
}

#[derive(Debug, Deserialize)]
struct CfError {
    code: u32,
    message: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KvNamespace {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkerSubdomain {
    pub subdomain: String,
}

pub struct CloudflareApiClient {
    http: Client,
    api_token: String,
    account_id: String,
}

impl CloudflareApiClient {
    pub fn new(api_token: &str, account_id: &str) -> Self {
        Self {
            http: Client::new(),
            api_token: api_token.to_string(),
            account_id: account_id.to_string(),
        }
    }

    // -- KV Namespace operations --

    pub async fn list_kv_namespaces(&self) -> Result<Vec<KvNamespace>, CloudflareApiError> {
        let url = format!(
            "{CF_API_BASE}/accounts/{}/storage/kv/namespaces?per_page=100",
            self.account_id
        );
        self.get_parsed(&url).await
    }

    pub async fn find_kv_namespace(
        &self,
        title: &str,
    ) -> Result<Option<KvNamespace>, CloudflareApiError> {
        let namespaces = self.list_kv_namespaces().await?;
        Ok(namespaces.into_iter().find(|ns| ns.title == title))
    }

    pub async fn create_kv_namespace(
        &self,
        title: &str,
    ) -> Result<KvNamespace, CloudflareApiError> {
        let url = format!(
            "{CF_API_BASE}/accounts/{}/storage/kv/namespaces",
            self.account_id
        );
        let body = serde_json::json!({ "title": title });
        self.post_parsed(&url, &body).await
    }

    /// Find existing or create a new KV namespace. Returns (namespace, created).
    pub async fn ensure_kv_namespace(
        &self,
        title: &str,
    ) -> Result<(KvNamespace, bool), CloudflareApiError> {
        if let Some(existing) = self.find_kv_namespace(title).await? {
            return Ok((existing, false));
        }
        let ns = self.create_kv_namespace(title).await?;
        Ok((ns, true))
    }

    pub async fn delete_kv_namespace(&self, namespace_id: &str) -> Result<(), CloudflareApiError> {
        let url = format!(
            "{CF_API_BASE}/accounts/{}/storage/kv/namespaces/{namespace_id}",
            self.account_id
        );
        self.delete_checked(&url).await
    }

    // -- Worker script operations --

    /// Deploy an ESM Worker script with a KV namespace binding.
    pub async fn deploy_worker(
        &self,
        script_name: &str,
        js_content: &str,
        kv_namespace_id: &str,
        kv_binding_name: &str,
    ) -> Result<(), CloudflareApiError> {
        let url = format!(
            "{CF_API_BASE}/accounts/{}/workers/scripts/{script_name}",
            self.account_id
        );

        let metadata = serde_json::json!({
            "main_module": "worker.js",
            "compatibility_date": "2024-09-23",
            "bindings": [{
                "type": "kv_namespace",
                "name": kv_binding_name,
                "namespace_id": kv_namespace_id,
            }]
        });

        let form = multipart::Form::new()
            .text("metadata", metadata.to_string())
            .part(
                "worker.js",
                multipart::Part::text(js_content.to_string())
                    .file_name("worker.js")
                    .mime_str("application/javascript+module")
                    .map_err(CloudflareApiError::Request)?,
            );

        let response = self
            .http
            .put(&url)
            .bearer_auth(&self.api_token)
            .multipart(form)
            .send()
            .await?;

        self.check_response(response).await
    }

    pub async fn delete_worker(&self, script_name: &str) -> Result<(), CloudflareApiError> {
        let url = format!(
            "{CF_API_BASE}/accounts/{}/workers/scripts/{script_name}",
            self.account_id
        );
        self.delete_checked(&url).await
    }

    // -- Subdomain / routing --

    pub async fn get_workers_subdomain(&self) -> Result<WorkerSubdomain, CloudflareApiError> {
        let url = format!(
            "{CF_API_BASE}/accounts/{}/workers/subdomain",
            self.account_id
        );
        self.get_parsed(&url).await
    }

    pub async fn enable_workers_dev_route(
        &self,
        script_name: &str,
    ) -> Result<(), CloudflareApiError> {
        let url = format!(
            "{CF_API_BASE}/accounts/{}/workers/scripts/{script_name}/subdomain",
            self.account_id
        );
        let body = serde_json::json!({ "enabled": true });
        let response = self
            .http
            .post(&url)
            .bearer_auth(&self.api_token)
            .json(&body)
            .send()
            .await?;
        self.check_response(response).await
    }

    /// Build the full Worker URL from account subdomain and script name.
    pub async fn resolve_worker_url(
        &self,
        script_name: &str,
    ) -> Result<String, CloudflareApiError> {
        let subdomain = self.get_workers_subdomain().await?;
        Ok(format!(
            "https://{script_name}.{}.workers.dev",
            subdomain.subdomain
        ))
    }

    // -- Internal helpers --

    async fn get_parsed<T: DeserializeOwned>(&self, url: &str) -> Result<T, CloudflareApiError> {
        let response = self
            .http
            .get(url)
            .bearer_auth(&self.api_token)
            .send()
            .await?;
        self.parse_result(response).await
    }

    async fn post_parsed<T: DeserializeOwned>(
        &self,
        url: &str,
        body: &serde_json::Value,
    ) -> Result<T, CloudflareApiError> {
        let response = self
            .http
            .post(url)
            .bearer_auth(&self.api_token)
            .json(body)
            .send()
            .await?;
        self.parse_result(response).await
    }

    async fn delete_checked(&self, url: &str) -> Result<(), CloudflareApiError> {
        let response = self
            .http
            .delete(url)
            .bearer_auth(&self.api_token)
            .send()
            .await?;
        self.check_response(response).await
    }

    async fn parse_result<T: DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T, CloudflareApiError> {
        let text = response.text().await?;
        let envelope: CfEnvelope<T> = serde_json::from_str(&text)
            .map_err(|_| CloudflareApiError::UnexpectedResponse(truncate(&text, 200)))?;
        if !envelope.success {
            return Err(extract_api_error(envelope.errors));
        }
        envelope
            .result
            .ok_or_else(|| CloudflareApiError::UnexpectedResponse("Missing result field".into()))
    }

    async fn check_response(&self, response: reqwest::Response) -> Result<(), CloudflareApiError> {
        let text = response.text().await?;
        let envelope: CfEnvelope<serde_json::Value> = serde_json::from_str(&text)
            .map_err(|_| CloudflareApiError::UnexpectedResponse(truncate(&text, 200)))?;
        if !envelope.success {
            return Err(extract_api_error(envelope.errors));
        }
        Ok(())
    }
}

fn extract_api_error(errors: Vec<CfError>) -> CloudflareApiError {
    let err = errors.into_iter().next().unwrap_or(CfError {
        code: 0,
        message: "Unknown error".into(),
    });
    CloudflareApiError::Api {
        code: err.code,
        message: err.message,
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}
