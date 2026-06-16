use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use crate::auth::extractors::DeviceAuth;
use crate::AppState;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeployFile {
    pub path: String,
    pub content_base64: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeployResult {
    pub deployment_id: String,
    pub url: String,
    pub status: String, // "building" | "ready" | "error"
}

fn normalize_ready_state(ready_state: &str) -> &'static str {
    match ready_state {
        "READY" => "ready",
        "ERROR" | "CANCELED" => "error",
        _ => "building",
    }
}

fn with_scheme(url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("https://{url}")
    }
}

pub struct VercelClient {
    base_url: String,
    token: String,
    http: reqwest::Client,
}

impl VercelClient {
    pub fn new(token: String) -> Self {
        Self::with_base_url("https://api.vercel.com".to_string(), token)
    }

    pub fn with_base_url(base_url: String, token: String) -> Self {
        Self { base_url: base_url.trim_end_matches('/').to_string(), token, http: reqwest::Client::new() }
    }

    pub async fn create_deployment(&self, name: &str, files: &[DeployFile]) -> Result<DeployResult, String> {
        let vercel_files: Vec<_> = files
            .iter()
            .map(|f| json!({ "file": f.path, "data": f.content_base64, "encoding": "base64" }))
            .collect();
        let body = json!({
            "name": name,
            "files": vercel_files,
            "target": "production",
            "projectSettings": { "framework": serde_json::Value::Null }
        });
        let res = self
            .http
            .post(format!("{}/v13/deployments", self.base_url))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("vercel request failed: {e}"))?;
        let status = res.status();
        let value: serde_json::Value = res.json().await.map_err(|e| format!("vercel response not json: {e}"))?;
        if !status.is_success() {
            let msg = value["error"]["message"].as_str().unwrap_or("unknown error");
            return Err(format!("vercel rejected the deploy: {msg}"));
        }
        let deployment_id = value["id"].as_str().filter(|s| !s.is_empty())
            .ok_or_else(|| format!("vercel response missing id: {value}"))?.to_string();
        let url = value["url"].as_str().filter(|s| !s.is_empty())
            .ok_or_else(|| format!("vercel response missing url: {value}"))?;
        Ok(DeployResult {
            deployment_id,
            url: with_scheme(url),
            status: normalize_ready_state(value["readyState"].as_str().unwrap_or("QUEUED")).to_string(),
        })
    }

    pub async fn get_status(&self, deployment_id: &str) -> Result<DeployResult, String> {
        let res = self
            .http
            .get(format!("{}/v13/deployments/{}", self.base_url, deployment_id))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| format!("vercel request failed: {e}"))?;
        let status = res.status();
        let value: serde_json::Value = res.json().await.map_err(|e| format!("vercel response not json: {e}"))?;
        if !status.is_success() {
            let msg = value["error"]["message"].as_str().unwrap_or("unknown error");
            return Err(format!("vercel get_status failed: {msg}"));
        }
        Ok(DeployResult {
            deployment_id: value["id"].as_str().unwrap_or(deployment_id).to_string(),
            url: with_scheme(value["url"].as_str().unwrap_or_default()),
            status: normalize_ready_state(value["readyState"].as_str().unwrap_or("QUEUED")).to_string(),
        })
    }
}

/// v1 supports a single target. Maps a wizard target id → the vault provider id.
fn vault_provider_for(target: &str) -> Option<&'static str> {
    match target {
        "vercel" => Some("vercel"),
        _ => None,
    }
}

#[derive(Deserialize)]
pub struct SetDeployCredentialBody {
    pub target: String,
    pub token: String,
}

/// PUT /api/design/deploy/credential — store the user's host token in the vault.
pub async fn set_deploy_credential(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Json(body): Json<SetDeployCredentialBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let provider = vault_provider_for(&body.target)
        .ok_or((StatusCode::BAD_REQUEST, "unsupported deploy target".to_string()))?;
    if body.token.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "token is required".to_string()));
    }
    state
        .provider_keys
        .set(claims.engagement_id, provider, body.token.trim())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(json!({ "connected": true })))
}

/// GET /api/design/deploy/credential/{target} — report whether a token is stored
/// (never returns the token itself).
pub async fn get_deploy_credential_status(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Path(target): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let provider = vault_provider_for(&target)
        .ok_or((StatusCode::BAD_REQUEST, "unsupported deploy target".to_string()))?;
    let connected = state
        .provider_keys
        .get(claims.engagement_id, provider)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .is_some();
    Ok(Json(json!({ "connected": connected })))
}

#[derive(Deserialize)]
pub struct DeployRequestBody {
    pub target: String,
    pub project_name: String,
    pub files: Vec<DeployFile>,
}

/// POST /api/design/deploy — deploy an uploaded bundle to the target using the vaulted token.
pub async fn create_deploy(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Json(body): Json<DeployRequestBody>,
) -> Result<Json<DeployResult>, (StatusCode, String)> {
    let provider = vault_provider_for(&body.target)
        .ok_or((StatusCode::BAD_REQUEST, "unsupported deploy target".to_string()))?;
    if body.files.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "nothing to deploy".to_string()));
    }
    let token = state
        .provider_keys
        .get(claims.engagement_id, provider)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or((StatusCode::CONFLICT, "no login stored for this host".to_string()))?;
    let client = VercelClient::new(token);
    let result = client
        .create_deployment(&body.project_name, &body.files)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
    Ok(Json(result))
}

#[derive(Deserialize)]
pub struct DeployStatusQuery {
    pub target: String,
}

/// GET /api/design/deploy/status/{deployment_id}?target=vercel — poll the deployment.
pub async fn get_deploy_status(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Path(deployment_id): Path<String>,
    Query(q): Query<DeployStatusQuery>,
) -> Result<Json<DeployResult>, (StatusCode, String)> {
    let provider = vault_provider_for(&q.target)
        .ok_or((StatusCode::BAD_REQUEST, "unsupported deploy target".to_string()))?;
    let token = state
        .provider_keys
        .get(claims.engagement_id, provider)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or((StatusCode::CONFLICT, "no login stored for this host".to_string()))?;
    let client = VercelClient::new(token);
    let result = client.get_status(&deployment_id).await.map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
    Ok(Json(result))
}

#[derive(Serialize, Clone)]
pub struct Recommendation {
    pub choice: String,
    pub reason: String,
}

pub(crate) fn fallback_recommendation(options: &[String]) -> Recommendation {
    let choice = options.first().cloned().unwrap_or_else(|| "vercel".to_string());
    Recommendation {
        choice,
        reason: "A reliable default that fits most projects.".to_string(),
    }
}

#[derive(Deserialize)]
pub struct RecommendBody {
    pub project_summary: String,
    pub options: Vec<String>,
}

/// POST /api/design/deploy/recommend — one-line "Ask AI" pick among the given options.
pub async fn recommend(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Json(body): Json<RecommendBody>,
) -> Result<Json<Recommendation>, (StatusCode, String)> {
    if body.options.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "no options to choose from".to_string()));
    }
    let key = match state.provider_keys.get(claims.engagement_id, "anthropic").await {
        Ok(Some(k)) => k,
        _ => return Ok(Json(fallback_recommendation(&body.options))),
    };
    let prompt = format!(
        "A non-technical person is choosing where to publish their project. \
         Project: {}. Options: {}. Reply with ONLY a compact JSON object \
         {{\"choice\":\"<one option exactly>\",\"reason\":\"<one short plain sentence, no jargon>\"}}.",
        body.project_summary,
        body.options.join(", ")
    );
    let client = crate::llm::anthropic::AnthropicClient::with_base_url(
        "https://api.anthropic.com".to_string(),
        key,
    );
    let messages = vec![json!({ "role": "user", "content": prompt })];
    let value = match client.create_message("claude-haiku-4-5-20251001", messages, vec![]).await {
        Ok(v) => v,
        Err(_) => return Ok(Json(fallback_recommendation(&body.options))),
    };
    let text = value["content"][0]["text"].as_str().unwrap_or("");
    let parsed: Option<Recommendation> = serde_json::from_str::<serde_json::Value>(text.trim())
        .ok()
        .and_then(|v| {
            let choice = v["choice"].as_str()?.to_string();
            let reason = v["reason"].as_str().unwrap_or("").to_string();
            if body.options.iter().any(|o| o == &choice) {
                Some(Recommendation { choice, reason })
            } else {
                None
            }
        });
    Ok(Json(parsed.unwrap_or_else(|| fallback_recommendation(&body.options))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn recommend_falls_back_to_first_option_without_a_key() {
        let options = vec!["vercel".to_string(), "netlify".to_string()];
        let rec = fallback_recommendation(&options);
        assert_eq!(rec.choice, "vercel");
        assert!(!rec.reason.is_empty());
    }

    #[tokio::test]
    async fn create_deployment_posts_files_and_returns_id_and_url() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v13/deployments"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "dpl_123",
                "url": "my-site-abc.vercel.app",
                "readyState": "QUEUED"
            })))
            .mount(&server)
            .await;

        let client = VercelClient::with_base_url(server.uri(), "tok_test".to_string());
        let files = vec![DeployFile {
            path: "index.html".to_string(),
            content_base64: "PGgxPmhpPC9oMT4=".to_string(),
        }];
        let res = client.create_deployment("my-site", &files).await.unwrap();
        assert_eq!(res.deployment_id, "dpl_123");
        assert_eq!(res.url, "https://my-site-abc.vercel.app");
        assert_eq!(res.status, "building");
    }

    #[tokio::test]
    async fn get_status_maps_ready_state() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v13/deployments/dpl_123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "dpl_123",
                "url": "my-site-abc.vercel.app",
                "readyState": "READY"
            })))
            .mount(&server)
            .await;

        let client = VercelClient::with_base_url(server.uri(), "tok_test".to_string());
        let res = client.get_status("dpl_123").await.unwrap();
        assert_eq!(res.status, "ready");
        assert_eq!(res.url, "https://my-site-abc.vercel.app");
    }

    #[tokio::test]
    async fn get_status_errors_on_non_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v13/deployments/dpl_x"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "error": { "message": "not found" }
            })))
            .mount(&server)
            .await;

        let client = VercelClient::with_base_url(server.uri(), "tok_test".to_string());
        assert!(client.get_status("dpl_x").await.is_err());
    }
}
