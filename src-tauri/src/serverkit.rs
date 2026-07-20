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

/// Extension capability names reported by `GET /pair` (plan 18). An older
/// extension simply omits one, and the matching UI is disabled instead of
/// failing halfway through an operation.
pub const FEATURE_PULL_CODE: &str = "pull-code";
/// Chunked, resumable transfers (plan 19). Absent = talk v1 to this server.
pub const FEATURE_SYNC_V2: &str = "sync-v2";

/// Per-chunk request budget. reqwest's `timeout` is a *total* request budget,
/// so with one chunk per request this is exactly the per-chunk timeout the
/// plan calls for: the whole operation is bounded by liveness, not duration,
/// and a two-hour upload never trips a clock as long as chunks keep landing.
const CHUNK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// How many times one chunk is retried before the transfer gives up. A failed
/// transfer is resumable anyway, so this only saves the user from having to
/// press the button again after a momentary blip.
const CHUNK_ATTEMPTS: u32 = 3;

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
    /// Capabilities the extension advertises. Empty for extensions predating
    /// the `features` array — callers must treat "absent" as "unsupported",
    /// never as "unknown, try anyway".
    pub features: Vec<String>,
}

/// A remote WordPress site as listed by `GET /api/v1/wordpress/sites`.
#[derive(Debug, Clone, Serialize)]
pub struct RemoteWpSite {
    pub id: i64,
    pub name: String,
    pub url: Option<String>,
    pub status: String,
    pub wp_version: Option<String>,
    pub php_version: Option<String>,
    /// Multisite installs are refused by the import flow — one local compose
    /// project cannot represent a network of sites.
    pub multisite: bool,
    pub environment_count: i64,
}

const USER_AGENT: &str = concat!("LocalKit/", env!("CARGO_PKG_VERSION"));

fn client() -> Result<reqwest::Client, String> {
    build_client(std::time::Duration::from_secs(15))
}

/// Client for archive/dump transfers. The 15 s probe timeout is a *total*
/// request budget in reqwest, so it would abort any real push or pull the
/// moment the payload outgrew a fast link — bulk transfers get their own
/// generous ceiling instead.
fn transfer_client() -> Result<reqwest::Client, String> {
    build_client(std::time::Duration::from_secs(1800))
}

fn build_client(timeout: std::time::Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(timeout)
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

    // Step 3 (best-effort): is the serverkit-localkit extension installed, and
    // what can this build of it do?
    let pair = pair(&base, api_key).await;
    let localkit_extension = pair.is_some();
    let features = pair.map(|p| p.features).unwrap_or_default();

    Ok(ServerKitInfo {
        status: health.status.unwrap_or_else(|| "unknown".into()),
        service: health.service.unwrap_or_default(),
        canonical_domain: health.canonical_domain.filter(|d| !d.is_empty()),
        canonical_origin: health.canonical_origin,
        staging: health.staging,
        api_key_valid: true,
        localkit_extension,
        features,
    })
}

#[derive(Deserialize)]
struct PairResponse {
    #[serde(default)]
    features: Vec<String>,
}

/// `GET /pair` — extension presence probe. `None` means "not installed or
/// unreachable"; the features list is empty on extensions predating plan 18.
async fn pair(base: &str, api_key: &str) -> Option<PairResponse> {
    let resp = client()
        .ok()?
        .get(format!("{base}/api/v1/localkit/pair"))
        .header("X-API-Key", api_key)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    // A 200 without a parseable body still proves the extension is there.
    Some(resp.json().await.unwrap_or(PairResponse { features: vec![] }))
}

/// Does this server's extension advertise `feature`?
///
/// Used to gate the Import flow: without `pull-code` there is no way to fetch
/// the remote `wp-content`, and finding that out mid-import would leave a
/// half-built local site behind.
pub async fn has_feature(url: &str, api_key: &str, feature: &str) -> Result<bool, String> {
    let base = normalize_base_url(url)?;
    Ok(pair(&base, api_key)
        .await
        .is_some_and(|p| p.features.iter().any(|f| f == feature)))
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
    /// Explicit alias of `url` added by the plan-18 extension; either may be
    /// absent depending on the extension version.
    #[serde(default)]
    site_url: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    wp_version: Option<String>,
    #[serde(default)]
    php_version: Option<String>,
    #[serde(default)]
    multisite: bool,
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
            url: s.url.or(s.site_url).filter(|u| !u.is_empty()),
            status: s.status.unwrap_or_else(|| "unknown".into()),
            wp_version: s.wp_version,
            php_version: s.php_version,
            multisite: s.multisite,
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
    let http = transfer_client()?;
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
    download(url, api_key, "/api/v1/localkit/pull/db", remote_site_id, "database dump").await
}

/// Download a tar.gz of a remote site's `wp-content`
/// (`GET /api/v1/localkit/pull/code`, plan 18).
///
/// Only available on extensions advertising the `pull-code` feature — callers
/// should check `has_feature` first so the failure surfaces before any local
/// site has been provisioned.
pub async fn pull_code(url: &str, api_key: &str, remote_site_id: i64) -> Result<Vec<u8>, String> {
    download(url, api_key, "/api/v1/localkit/pull/code", remote_site_id, "wp-content archive").await
}

/// Shared `GET <path>?site_id=` binary download against the extension.
///
/// Downloads are not bounded by the server's 100 MB upload limit, but they are
/// still read fully into memory here — plan 19 (chunked sync) is what lifts
/// that for genuinely large sites.
async fn download(
    url: &str,
    api_key: &str,
    path: &str,
    remote_site_id: i64,
    what: &str,
) -> Result<Vec<u8>, String> {
    let base = normalize_base_url(url)?;
    let http = transfer_client()?;
    let resp = http
        .get(format!("{base}{path}"))
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
        .map_err(|e| format!("failed to download {what}: {e}"))
}

// ---------------------------------------------------------------------------
// Sync v2 — chunked, resumable transfers (plan 19)
// ---------------------------------------------------------------------------

/// Reports transfer progress as `(bytes_done, bytes_total)`.
///
/// A plain `dyn Fn` rather than a generic parameter: it crosses two async
/// functions and several await points, and monomorphizing that buys nothing
/// when it fires once per 8 MiB.
pub type ProgressFn<'a> = &'a (dyn Fn(u64, u64) + Send + Sync);

#[derive(Deserialize)]
struct InitResponse {
    transfer_id: String,
    #[serde(default)]
    chunk_size: Option<u64>,
    /// Offsets the server already holds — subtracting these from our own
    /// chunk plan is the entirety of resume.
    #[serde(default)]
    received: Vec<u64>,
}

/// Map an extension error response onto something a user can act on.
fn api_error(what: &str, code: u16, body: &str) -> String {
    match code {
        404 => extract_error(body).unwrap_or_else(|| {
            "The serverkit-localkit extension is not installed on this ServerKit server (404).".into()
        }),
        401 | 403 => "The API key was rejected (or lacks admin rights). Check the key.".into(),
        413 => "The upload is too large for the server (ServerKit limit is 100MB).".into(),
        _ => extract_error(body).unwrap_or_else(|| format!("{what} failed with HTTP {code}.")),
    }
}

/// Upload a staged payload in chunks, resuming whatever a previous attempt left
/// behind (`POST init` → `PUT chunk`… → `POST finish`).
///
/// `kind` is `"code"` or `"db"`. The server only processes anything in
/// `finish`, and only after the whole-file hash verifies — so abandoning this
/// mid-way (cancel, crash, dropped link) can never leave the remote site half
/// updated.
#[allow(clippy::too_many_arguments)]
pub async fn push_chunked(
    url: &str,
    api_key: &str,
    kind: &str,
    remote_site_id: i64,
    local_url: Option<&str>,
    staged: &crate::transfer::Staged,
    cancel: &crate::transfer::CancelToken,
    progress: ProgressFn<'_>,
) -> Result<serde_json::Value, String> {
    use crate::transfer;

    let base = normalize_base_url(url)?;
    let http = build_client(CHUNK_TIMEOUT)?;
    let total = staged.total();

    let mut init_body = serde_json::json!({
        "site_id": remote_site_id,
        "total_bytes": total,
        "chunk_size": transfer::CHUNK_SIZE,
        "sha256": staged.sha256(),
        "filename": if kind == "code" { "wp-content.tar.gz" } else { "dump.sql" },
    });
    if let Some(u) = local_url {
        init_body["local_url"] = serde_json::Value::String(u.to_string());
    }

    let init: InitResponse = post_json(
        &http,
        &base,
        &format!("/api/v1/localkit/push/{kind}/init"),
        api_key,
        &init_body,
        "Starting the upload",
    )
    .await?;

    // The server owns the offsets, so if it reports a chunk size, that is the
    // one the plan has to be built from.
    let chunk_size = init.chunk_size.filter(|c| *c > 0).unwrap_or(transfer::CHUNK_SIZE);
    let plan = transfer::remaining(total, chunk_size, &init.received);
    let mut done = total.saturating_sub(transfer::bytes_of(&plan));
    progress(done, total);

    for chunk in plan {
        cancel.check()?;
        let bytes = staged.read_chunk(chunk)?;
        let chunk_sha = transfer::sha256_hex(&bytes);
        put_chunk(&http, &base, api_key, kind, &init.transfer_id, chunk, &chunk_sha, bytes, cancel)
            .await?;
        done += chunk.len;
        progress(done, total);
    }

    cancel.check()?;
    // `finish` runs the server-side extract/import, which can take minutes on a
    // big site — it gets the generous transfer budget, not the chunk one.
    post_json(
        &transfer_client()?,
        &base,
        &format!("/api/v1/localkit/push/{kind}/finish"),
        api_key,
        &serde_json::json!({ "transfer_id": init.transfer_id }),
        "Finishing the upload",
    )
    .await
}

async fn post_json<T: serde::de::DeserializeOwned>(
    http: &reqwest::Client,
    base: &str,
    path: &str,
    api_key: &str,
    body: &serde_json::Value,
    what: &str,
) -> Result<T, String> {
    let resp = http
        .post(format!("{base}{path}"))
        .header("X-API-Key", api_key)
        .json(body)
        .send()
        .await
        .map_err(|e| request_error(base, &e))?;
    let code = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    if !(200..300).contains(&code) {
        return Err(api_error(what, code, &text));
    }
    serde_json::from_str(&text).map_err(|e| format!("{what}: unexpected server response ({e})"))
}

/// PUT one chunk, retrying transient failures.
///
/// Only transport errors and 5xx are retried: a 4xx means the server rejected
/// what we sent (bad offset, failed checksum), and sending the identical bytes
/// again would fail identically.
#[allow(clippy::too_many_arguments)]
async fn put_chunk(
    http: &reqwest::Client,
    base: &str,
    api_key: &str,
    kind: &str,
    transfer_id: &str,
    chunk: crate::transfer::Chunk,
    chunk_sha: &str,
    bytes: Vec<u8>,
    cancel: &crate::transfer::CancelToken,
) -> Result<(), String> {
    let url = format!("{base}/api/v1/localkit/push/{kind}/chunk");
    let mut last = String::new();
    for attempt in 1..=CHUNK_ATTEMPTS {
        cancel.check()?;
        let result = http
            .put(&url)
            .header("X-API-Key", api_key)
            .header("Content-Type", "application/octet-stream")
            .query(&[
                ("transfer_id", transfer_id.to_string()),
                ("offset", chunk.offset.to_string()),
                ("sha256", chunk_sha.to_string()),
            ])
            .body(bytes.clone())
            .send()
            .await;

        match result {
            Ok(resp) => {
                let code = resp.status().as_u16();
                if (200..300).contains(&code) {
                    return Ok(());
                }
                let body = resp.text().await.unwrap_or_default();
                last = api_error(&format!("Uploading the chunk at {}", chunk.offset), code, &body);
                if code < 500 {
                    return Err(last);
                }
            }
            Err(e) => last = request_error(base, &e),
        }

        if attempt < CHUNK_ATTEMPTS {
            tokio::time::sleep(std::time::Duration::from_secs(attempt as u64)).await;
        }
    }
    Err(last)
}

/// Download to a temp file with byte progress, resume and cancel.
///
/// No protocol invention: HTTP `Range` already is the chunked protocol in this
/// direction. `session` pins one materialized export on the server so that the
/// ranges of an interrupted download all come from the same bytes; a server
/// that ignores it (or has reaped the session) answers a range request with
/// the whole body, and the `200` branch below simply starts over.
pub async fn download_resumable(
    url: &str,
    api_key: &str,
    path: &str,
    remote_site_id: i64,
    what: &str,
    cancel: &crate::transfer::CancelToken,
    progress: ProgressFn<'_>,
) -> Result<crate::transfer::TempFile, String> {
    use std::io::Write;

    let base = normalize_base_url(url)?;
    let http = build_client(CHUNK_TIMEOUT)?;
    let session = uuid::Uuid::new_v4().simple().to_string();
    let temp = crate::transfer::TempFile::new("download")?;

    let mut etag: Option<String> = None;
    let mut last = String::new();

    for attempt in 1..=CHUNK_ATTEMPTS {
        cancel.check()?;
        let have = temp.len();

        let mut req = http
            .get(format!("{base}{path}"))
            .query(&[
                ("site_id", remote_site_id.to_string()),
                ("session", session.clone()),
            ])
            .header("X-API-Key", api_key);
        if have > 0 {
            req = req.header("Range", format!("bytes={have}-"));
            // If-Range makes the resume safe: if the export changed under us,
            // the server owes us a 200 with the whole body rather than a tail
            // that would splice into nonsense.
            if let Some(tag) = &etag {
                req = req.header("If-Range", tag.clone());
            }
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                last = request_error(&base, &e);
                if attempt < CHUNK_ATTEMPTS {
                    tokio::time::sleep(std::time::Duration::from_secs(attempt as u64)).await;
                }
                continue;
            }
        };

        let code = resp.status().as_u16();
        if code != 200 && code != 206 {
            let body = resp.text().await.unwrap_or_default();
            return Err(api_error(&format!("Downloading the {what}"), code, &body));
        }

        etag = resp
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);

        // A 200 to a ranged request means "here is everything" — the partial
        // file is worthless, so throw it away rather than appending a second
        // copy of the head onto it.
        let mut done = if code == 206 { have } else { 0 };
        if code == 200 && have > 0 {
            temp.truncate()?;
        }
        let total = resp.content_length().map(|len| done + len).unwrap_or(0);
        progress(done, total);

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .append(true)
            .open(temp.path())
            .map_err(|e| format!("failed to open the download file: {e}"))?;

        let mut resp = resp;
        let mut stalled = None;
        loop {
            if cancel.cancelled() {
                return Err(crate::transfer::CANCELLED.to_string());
            }
            match resp.chunk().await {
                Ok(Some(bytes)) => {
                    file.write_all(&bytes)
                        .map_err(|e| format!("failed to write the download: {e}"))?;
                    done += bytes.len() as u64;
                    progress(done, total.max(done));
                }
                Ok(None) => break,
                Err(e) => {
                    // Mid-stream failure: flush what we have and resume from
                    // there on the next attempt.
                    stalled = Some(format!("failed to download the {what}: {e}"));
                    break;
                }
            }
        }
        file.flush().map_err(|e| format!("failed to write the download: {e}"))?;
        drop(file);

        match stalled {
            None => return Ok(temp),
            Some(e) => {
                last = e;
                if attempt < CHUNK_ATTEMPTS {
                    tokio::time::sleep(std::time::Duration::from_secs(attempt as u64)).await;
                }
            }
        }
    }
    Err(last)
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
