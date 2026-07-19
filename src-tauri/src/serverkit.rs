//! Read-only ServerKit API client (M3).
//!
//! Auth: `X-API-Key` header (see ServerKit `app/middleware/api_key_auth.py`).
//! Note: ServerKit's `X-API-Key` middleware only authenticates the RBAC
//! `auth_required()` decorator; endpoints using bare flask `@jwt_required()`
//! (which includes the WordPress hub `GET /api/v1/wordpress/sites` today)
//! reject API keys — we surface that as a clear error until the
//! `serverkit-localkit` extension (M4) provides the API-key surface.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerKitConnection {
    pub id: String,
    pub label: String,
    pub url: String,
    pub api_key: String,
    pub created_at: String,
}

/// Result of a successful connection test.
#[derive(Debug, Clone, Serialize)]
pub struct ServerKitInfo {
    pub status: String,
    pub service: String,
    pub canonical_domain: Option<String>,
    pub canonical_origin: Option<String>,
    pub staging: bool,
    pub api_key_valid: bool,
    /// Whether the serverkit-localkit extension (M4 push/pull) is installed.
    pub localkit_extension: bool,
}

/// A remote WordPress site as listed by `GET /api/v1/wordpress/sites`.
#[derive(Debug, Clone, Serialize)]
pub struct RemoteWpSite {
    pub id: i64,
    pub name: String,
    pub url: Option<String>,
    pub status: String,
    pub wp_version: Option<String>,
    pub environment_count: i64,
}

const USER_AGENT: &str = concat!("LocalKit/", env!("CARGO_PKG_VERSION"));

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))
}

/// Normalize a user-entered base URL: add scheme if missing, strip trailing
/// slashes. Returns Err on obviously bad input.
pub fn normalize_base_url(url: &str) -> Result<String, String> {
    let mut u = url.trim().trim_end_matches('/').to_string();
    if u.is_empty() {
        return Err("Server URL is required".into());
    }
    if !u.starts_with("http://") && !u.starts_with("https://") {
        u = format!("https://{u}");
    }
    Ok(u)
}

fn request_error(url: &str, e: &reqwest::Error) -> String {
    if e.is_connect() || e.is_timeout() {
        format!("Could not reach {url} — check the URL and that ServerKit is running.")
    } else if e.is_request() {
        format!("Invalid server URL {url}: {e}")
    } else {
        format!("Request to {url} failed: {e}")
    }
}

#[derive(Deserialize)]
struct HealthResponse {
    status: Option<String>,
    service: Option<String>,
    canonical_domain: Option<String>,
    canonical_origin: Option<String>,
    #[serde(default)]
    staging: bool,
}

/// Verify a ServerKit connection:
/// 1. `GET /api/v1/system/health` (public — no key sent, so an invalid key
///    can't mask an unreachable/wrong server) — confirms it's a ServerKit API.
/// 2. `GET /api/v1/setup-health/account` (`@auth_required()` — accepts
///    `X-API-Key`) — validates the API key.
pub async fn test_connection(url: &str, api_key: &str) -> Result<ServerKitInfo, String> {
    let base = normalize_base_url(url)?;
    let http = client()?;

    // Step 1: is this a ServerKit server? (key intentionally NOT sent — the
    // ServerKit middleware 401s any request carrying an invalid key, even
    // public routes, which would confuse the diagnosis.)
    let health_url = format!("{base}/api/v1/system/health");
    let resp = http
        .get(&health_url)
        .send()
        .await
        .map_err(|e| request_error(&base, &e))?;
    if !resp.status().is_success() {
        return Err(format!(
            "{base} answered with HTTP {} on /api/v1/system/health — is this a ServerKit server?",
            resp.status()
        ));
    }
    let health: HealthResponse = resp
        .json()
        .await
        .map_err(|_| format!("{base} did not return a ServerKit health response — is this a ServerKit server?"))?;
    if health.service.as_deref() != Some("serverkit-api") {
        return Err(format!(
            "{base} does not look like a ServerKit server (unexpected /api/v1/system/health response)."
        ));
    }

    // Step 2: validate the API key against an auth_required endpoint.
    let account_url = format!("{base}/api/v1/setup-health/account");
    let resp = http
        .get(&account_url)
        .header("X-API-Key", api_key)
        .send()
        .await
        .map_err(|e| request_error(&base, &e))?;
    match resp.status().as_u16() {
        200 => {}
        401 | 403 => {
            return Err("ServerKit is reachable, but the API key was rejected (401). Check the key.".into())
        }
        code => return Err(format!("API key validation failed with HTTP {code}.")),
    }

    // Step 3 (best-effort): is the serverkit-localkit extension installed?
    let localkit_extension = http
        .get(format!("{base}/api/v1/localkit/pair"))
        .header("X-API-Key", api_key)
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false);

    Ok(ServerKitInfo {
        status: health.status.unwrap_or_else(|| "unknown".into()),
        service: health.service.unwrap_or_default(),
        canonical_domain: health.canonical_domain.filter(|d| !d.is_empty()),
        canonical_origin: health.canonical_origin,
        staging: health.staging,
        api_key_valid: true,
        localkit_extension,
    })
}

#[derive(Deserialize)]
struct SitesResponse {
    #[serde(default)]
    sites: Vec<RawSite>,
}

#[derive(Deserialize)]
struct RawSite {
    id: i64,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    wp_version: Option<String>,
    #[serde(default)]
    environment_count: i64,
}

/// List remote WordPress sites via the serverkit-localkit extension
/// (`GET /api/v1/localkit/sites` — the API-key-friendly surface; the core
/// `/api/v1/wordpress/sites` route is JWT-only).
pub async fn list_wp_sites(url: &str, api_key: &str) -> Result<Vec<RemoteWpSite>, String> {
    let base = normalize_base_url(url)?;
    let http = client()?;
    let sites_url = format!("{base}/api/v1/localkit/sites");
    let resp = http
        .get(&sites_url)
        .header("X-API-Key", api_key)
        .send()
        .await
        .map_err(|e| request_error(&base, &e))?;

    let code = resp.status().as_u16();
    match code {
        200 => {}
        404 => {
            return Err(
                "The serverkit-localkit extension is not installed on this ServerKit server (404 on /api/v1/localkit/sites)."
                    .into(),
            )
        }
        401 | 403 => {
            return Err("The API key was rejected (or lacks admin rights). Check the key.".into());
        }
        409 => {
            let body = resp.text().await.unwrap_or_default();
            return Err(extract_error(&body)
                .unwrap_or_else(|| "The WordPress extension is not installed on this server.".into()));
        }
        _ => return Err(format!("Listing WordPress sites failed with HTTP {code}.")),
    }

    let parsed: SitesResponse = resp
        .json()
        .await
        .map_err(|e| format!("failed to parse WordPress sites response: {e}"))?;
    Ok(parsed
        .sites
        .into_iter()
        .map(|s| RemoteWpSite {
            id: s.id,
            name: s.name.unwrap_or_else(|| format!("site-{}", s.id)),
            url: s.url,
            status: s.status.unwrap_or_else(|| "unknown".into()),
            wp_version: s.wp_version,
            environment_count: s.environment_count,
        })
        .collect())
}

/// Pull the `error` field out of a ServerKit JSON error body.
fn extract_error(body: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()?
        .get("error")?
        .as_str()
        .map(|s| s.to_string())
}

/// Shared multipart POST to the serverkit-localkit extension.
async fn post_multipart(
    url: &str,
    api_key: &str,
    path: &str,
    fields: &[(&str, String)],
    file_name: &str,
    file_bytes: Vec<u8>,
) -> Result<serde_json::Value, String> {
    let base = normalize_base_url(url)?;
    let http = client()?;
    let mut form = reqwest::multipart::Form::new();
    for (k, v) in fields {
        form = form.text((*k).to_string(), v.clone());
    }
    let part = reqwest::multipart::Part::bytes(file_bytes).file_name(file_name.to_string());
    form = form.part("file", part);

    let resp = http
        .post(format!("{base}{path}"))
        .header("X-API-Key", api_key)
        .multipart(form)
        .send()
        .await
        .map_err(|e| request_error(&base, &e))?;

    let code = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();
    match code {
        200 | 201 => Ok(serde_json::from_str(&body).unwrap_or(serde_json::Value::Null)),
        404 => Err(
            "The serverkit-localkit extension is not installed on this ServerKit server (404)."
                .into(),
        ),
        401 | 403 => Err("The API key was rejected (or lacks admin rights). Check the key.".into()),
        413 => Err("The upload is too large for the server (ServerKit limit is 100MB).".into()),
        _ => Err(extract_error(&body).unwrap_or_else(|| format!("Push failed with HTTP {code}."))),
    }
}

/// Push a tar.gz of wp-content to a remote site (`POST /api/v1/localkit/push/code`).
pub async fn push_code(
    url: &str,
    api_key: &str,
    remote_site_id: i64,
    tgz: Vec<u8>,
) -> Result<serde_json::Value, String> {
    post_multipart(
        url,
        api_key,
        "/api/v1/localkit/push/code",
        &[("site_id", remote_site_id.to_string())],
        "wp-content.tar.gz",
        tgz,
    )
    .await
}

/// Push a SQL dump to a remote site (`POST /api/v1/localkit/push/db`).
/// `local_url` lets the server search-replace local -> remote URLs after import.
pub async fn push_db(
    url: &str,
    api_key: &str,
    remote_site_id: i64,
    local_url: &str,
    sql: Vec<u8>,
) -> Result<serde_json::Value, String> {
    post_multipart(
        url,
        api_key,
        "/api/v1/localkit/push/db",
        &[
            ("site_id", remote_site_id.to_string()),
            ("local_url", local_url.to_string()),
        ],
        "dump.sql",
        sql,
    )
    .await
}

/// Download a gzipped SQL dump of a remote site (`GET /api/v1/localkit/pull/db`).
pub async fn pull_db(url: &str, api_key: &str, remote_site_id: i64) -> Result<Vec<u8>, String> {
    let base = normalize_base_url(url)?;
    let http = client()?;
    let resp = http
        .get(format!("{base}/api/v1/localkit/pull/db"))
        .query(&[("site_id", remote_site_id.to_string())])
        .header("X-API-Key", api_key)
        .send()
        .await
        .map_err(|e| request_error(&base, &e))?;

    let code = resp.status().as_u16();
    if code != 200 {
        let body = resp.text().await.unwrap_or_default();
        return Err(match code {
            404 => extract_error(&body).unwrap_or_else(|| {
                "The serverkit-localkit extension is not installed on this ServerKit server (404)."
                    .into()
            }),
            401 | 403 => "The API key was rejected (or lacks admin rights). Check the key.".into(),
            _ => extract_error(&body).unwrap_or_else(|| format!("Pull failed with HTTP {code}.")),
        });
    }
    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("failed to download database dump: {e}"))
}

/// Provision a new remote WordPress site (`POST /api/v1/localkit/sites`).
pub async fn create_remote_site(
    url: &str,
    api_key: &str,
    name: &str,
) -> Result<serde_json::Value, String> {
    let base = normalize_base_url(url)?;
    let http = client()?;
    let resp = http
        .post(format!("{base}/api/v1/localkit/sites"))
        .header("X-API-Key", api_key)
        .json(&serde_json::json!({ "name": name }))
        .send()
        .await
        .map_err(|e| request_error(&base, &e))?;

    let code = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();
    match code {
        200 | 201 => serde_json::from_str(&body)
            .map_err(|e| format!("failed to parse create-site response: {e}")),
        404 => Err(
            "The serverkit-localkit extension is not installed on this ServerKit server (404)."
                .into(),
        ),
        401 | 403 => Err("The API key was rejected (or lacks admin rights). Check the key.".into()),
        _ => Err(extract_error(&body)
            .unwrap_or_else(|| format!("Creating the remote site failed with HTTP {code}."))),
    }
}
