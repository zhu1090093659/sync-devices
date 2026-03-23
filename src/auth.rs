use reqwest::Client;
use serde::Deserialize;
use thiserror::Error;

const CF_API_BASE: &str = "https://api.cloudflare.com/client/v4";

#[derive(Debug, Clone)]
pub struct CloudflareAccount {
    pub account_id: String,
    pub account_name: String,
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("HTTP request to Cloudflare API failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Cloudflare API token is invalid or expired.")]
    InvalidToken,
    #[error("Cloudflare API returned an unexpected response: {0}")]
    UnexpectedResponse(String),
    #[error("No Cloudflare accounts found for this token.")]
    NoAccounts,
}

#[derive(Debug, Deserialize)]
struct CfApiEnvelope<T> {
    success: bool,
    result: Option<T>,
}

#[derive(Debug, Deserialize)]
struct TokenVerifyResult {
    status: String,
}

#[derive(Debug, Deserialize)]
struct AccountResult {
    id: String,
    name: String,
}

/// Verify a Cloudflare API token and return the first associated account.
pub async fn verify_cf_token(api_token: &str) -> Result<CloudflareAccount, AuthError> {
    let http = Client::new();

    // Step 1: Verify token is active
    let verify_url = format!("{CF_API_BASE}/user/tokens/verify");
    let verify_resp = http.get(&verify_url).bearer_auth(api_token).send().await?;

    if !verify_resp.status().is_success() {
        return Err(AuthError::InvalidToken);
    }

    let envelope: CfApiEnvelope<TokenVerifyResult> = verify_resp.json().await?;
    if !envelope.success {
        return Err(AuthError::InvalidToken);
    }

    let verify_result = envelope.result.ok_or(AuthError::UnexpectedResponse(
        "missing result in verify response".into(),
    ))?;

    if verify_result.status != "active" {
        return Err(AuthError::InvalidToken);
    }

    // Step 2: Fetch associated accounts
    let accounts_url = format!("{CF_API_BASE}/accounts?per_page=5");
    let accounts_resp = http
        .get(&accounts_url)
        .bearer_auth(api_token)
        .send()
        .await?;

    if !accounts_resp.status().is_success() {
        return Err(AuthError::UnexpectedResponse(
            "failed to fetch Cloudflare accounts".into(),
        ));
    }

    let accounts_envelope: CfApiEnvelope<Vec<AccountResult>> = accounts_resp.json().await?;
    let accounts = accounts_envelope.result.unwrap_or_default();

    let account = accounts.into_iter().next().ok_or(AuthError::NoAccounts)?;

    Ok(CloudflareAccount {
        account_id: account.id,
        account_name: account.name,
    })
}
