use anyhow::{anyhow, Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Command;

use crate::chatbot::ChatBot;
use crate::config::{PartialConfig, ProviderKind};
use crate::modes::Mode;
use crate::personas;
use crate::providers;
use crate::types::ChatMessage;

const INDEX_HTML: &str = include_str!("../web/index.html");
const APP_JS: &str = include_str!("../web/app.js");
const STYLES_CSS: &str = include_str!("../web/styles.css");
const CORE_SANDBOX_JS: &str = include_str!("../web/core/sandbox.js");
const CORE_EXEC_JS: &str = include_str!("../web/core/exec.js");
const CORE_STATE_JS: &str = include_str!("../web/core/state.js");
const CORE_UI_JS: &str = include_str!("../web/core/ui.js");
const OBSERVER_LOGIC_JS: &str = include_str!("../web/observer/logic.js");
const REACT_JS: &str = include_str!("../web/vendor/react.production.min.js");
const REACT_DOM_JS: &str = include_str!("../web/vendor/react-dom.production.min.js");

fn dev_assets_root() -> Option<PathBuf> {
    if let Ok(v) = std::env::var("SPIRAL_CODER_ASSETS_DIR") {
        let p = PathBuf::from(v.trim());
        if !p.as_os_str().is_empty() {
            return Some(p);
        }
    }

    // Debug builds: allow fast UI iteration without rebuilding the Rust binary.
    if cfg!(debug_assertions) {
        return Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web"));
    }

    None
}

fn read_dev_asset(rel: &str) -> Option<Vec<u8>> {
    let root = dev_assets_root()?;
    let path = root.join(rel);
    std::fs::read(path).ok()
}

fn asset_content_type(rel: &str) -> &'static str {
    let low = rel.trim().to_ascii_lowercase();
    if low.ends_with(".js") {
        "text/javascript; charset=utf-8"
    } else if low.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if low.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if low.ends_with(".png") {
        "image/png"
    } else if low.ends_with(".svg") {
        "image/svg+xml"
    } else if low.ends_with(".woff2") {
        "font/woff2"
    } else {
        "application/octet-stream"
    }
}

fn is_safe_asset_rel(rel: &str) -> bool {
    if rel.trim().is_empty() {
        return false;
    }
    if rel.contains('\0') {
        return false;
    }
    // Be strict: accept only normal path components (no absolute paths, no "..").
    let p = Path::new(rel);
    for c in p.components() {
        match c {
            Component::Normal(_) => {}
            _ => return false,
        }
    }
    true
}

async fn serve_asset(stream: &mut TcpStream, req_path: &str) -> Result<()> {
    let rel = req_path.trim_start_matches("/assets/");
    if !is_safe_asset_rel(rel) {
        return write_text(
            stream,
            400,
            "Bad Request",
            "text/plain; charset=utf-8",
            "invalid asset path\n",
        )
        .await;
    }

    let ctype = asset_content_type(rel);
    if rel == "governor_contract.js" {
        let js = crate::governor_contract::browser_fallback_script();
        return write_response(stream, 200, "OK", ctype, js.as_bytes()).await;
    }
    if let Some(bytes) = read_dev_asset(rel) {
        return write_response(stream, 200, "OK", ctype, &bytes).await;
    }

    let bytes: Option<&'static [u8]> = match rel {
        "app.js" => Some(APP_JS.as_bytes()),
        "styles.css" => Some(STYLES_CSS.as_bytes()),
        "core/sandbox.js" => Some(CORE_SANDBOX_JS.as_bytes()),
        "core/exec.js" => Some(CORE_EXEC_JS.as_bytes()),
        "core/state.js" => Some(CORE_STATE_JS.as_bytes()),
        "core/ui.js" => Some(CORE_UI_JS.as_bytes()),
        "observer/logic.js" => Some(OBSERVER_LOGIC_JS.as_bytes()),
        "vendor/react.production.min.js" => Some(REACT_JS.as_bytes()),
        "vendor/react-dom.production.min.js" => Some(REACT_DOM_JS.as_bytes()),
        _ => None,
    };

    if let Some(b) = bytes {
        return write_response(stream, 200, "OK", ctype, b).await;
    }

    write_text(
        stream,
        404,
        "Not Found",
        "text/plain; charset=utf-8",
        "not found\n",
    )
    .await
}

fn openai_compat_chat_urls(base_url: &str) -> Vec<String> {
    let base = base_url.trim_end_matches('/');
    // Codestral uses a singular endpoint: /v1/chat/completion
    if base.contains("codestral.mistral.ai") {
        vec![
            format!("{base}/chat/completion"),
            format!("{base}/chat/completions"),
        ]
    } else {
        vec![
            format!("{base}/chat/completions"),
            format!("{base}/chat/completion"),
        ]
    }
}

fn should_use_v1_completions(status: reqwest::StatusCode, body: &str) -> bool {
    let msg = body.to_ascii_lowercase();
    if msg.contains("not a chat model") {
        return true;
    }
    if status == reqwest::StatusCode::NOT_FOUND
        && msg.contains("v1/completions")
        && msg.contains("chat/complet")
    {
        return true;
    }
    false
}

fn should_swap_to_max_completion_tokens(status: reqwest::StatusCode, body: &str) -> bool {
    status == reqwest::StatusCode::BAD_REQUEST
        && body.contains("max_completion_tokens")
        && body.contains("max_tokens")
}

fn swap_max_tokens_to_max_completion_tokens(payload: &mut serde_json::Value) {
    if let Some(obj) = payload.as_object_mut() {
        if let Some(mt) = obj.remove("max_tokens") {
            obj.insert("max_completion_tokens".to_string(), mt);
        }
    }
}

fn apply_openai_temperature_compat(payload: &mut serde_json::Value, base_url: &str, model: &str) {
    if !crate::config::should_send_temperature(&ProviderKind::OpenAiCompatible, base_url, model) {
        if let Some(obj) = payload.as_object_mut() {
            obj.remove("temperature");
        }
    }
}

fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
        || status == reqwest::StatusCode::REQUEST_TIMEOUT
}

fn retry_after_duration(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let v = headers.get("retry-after")?;
    let s = v.to_str().ok()?.trim();
    let secs: u64 = s.parse().ok()?;
    Some(Duration::from_secs(secs.min(15)))
}

fn backoff_delay(attempt: usize, retry_after: Option<Duration>) -> Duration {
    let pow = attempt.min(5) as u32;
    let factor = 1u64.checked_shl(pow).unwrap_or(u64::MAX);
    let base_ms = 500u64.saturating_mul(factor);
    let mut d = Duration::from_millis(base_ms.min(6000));
    if let Some(ra) = retry_after {
        if ra > d {
            d = ra;
        }
    }
    d
}

fn is_retryable_send_error(e: &reqwest::Error) -> bool {
    e.is_timeout() || e.is_connect()
}

fn prompt_from_chat_messages(messages: &[serde_json::Value]) -> String {
    let mut out = String::new();

    let mut sys: Vec<String> = Vec::new();
    for m in messages {
        if m.get("role").and_then(|x| x.as_str()) == Some("system") {
            if let Some(s) = m.get("content").and_then(|x| x.as_str()) {
                if !s.trim().is_empty() {
                    sys.push(s.trim_end().to_string());
                }
            }
        }
    }
    if !sys.is_empty() {
        out.push_str(sys.join("\n").trim());
        out.push_str("\n\n");
    }

    for m in messages {
        let role = m.get("role").and_then(|x| x.as_str()).unwrap_or("");
        if role == "system" {
            continue;
        }
        let content = m.get("content").and_then(|x| x.as_str()).unwrap_or("");
        if content.trim().is_empty() {
            continue;
        }
        let label = match role {
            "user" => "User",
            "assistant" => "Assistant",
            other => other,
        };
        out.push_str(label);
        out.push_str(": ");
        out.push_str(content.trim_end());
        out.push('\n');
    }

    out.push_str("Assistant: ");
    out
}

#[derive(Parser, Clone, Debug)]
pub struct ServeArgs {
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    #[arg(long, default_value_t = 8080)]
    pub port: u16,
}

#[derive(Clone)]
struct AppState {
    client: reqwest::Client,
    defaults: PartialConfig,
    pending_edits: crate::pending_edits::PendingEditStore,
    pending_commands: crate::pending_commands::PendingCommandStore,
    workspace_root: PathBuf,
}

pub async fn run(args: ServeArgs, defaults: PartialConfig) -> Result<()> {
    let addr: SocketAddr = format!("{}:{}", args.host, args.port)
        .parse()
        .context("invalid --host/--port")?;

    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;

    let workspace_root = std::env::current_dir().unwrap_or_default();
    let state = AppState {
        client: reqwest::Client::new(),
        defaults,
        pending_edits: crate::pending_edits::PendingEditStore::new(),
        pending_commands: crate::pending_commands::PendingCommandStore::new(),
        workspace_root,
    };

    println!("Spiral-Coder UI: http://{addr}/");

    loop {
        let (stream, _peer) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_connection(stream, state).await {
                eprintln!("serve: {err}");
            }
        });
    }
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    path: String,
    target: String,
    /// Value of the `Origin:` header, if present.
    origin: Option<String>,
    body: Vec<u8>,
}

/// Returns true only for origins that are the local server itself.
/// Rejects cross-site requests from arbitrary web pages.
fn is_localhost_origin(origin: &str) -> bool {
    let o = origin.trim();
    if o.is_empty() {
        return false;
    }

    // IMPORTANT: do not use string prefix matching here.
    // "https://localhost.evil.com" must NOT be treated as a localhost origin.
    let u = match reqwest::Url::parse(o) {
        Ok(u) => u,
        Err(_) => return false,
    };

    match u.scheme() {
        "http" | "https" => {}
        _ => return false,
    }

    let Some(host) = u.host_str() else {
        return false;
    };

    if host == "localhost" || host == "127.0.0.1" {
        return true;
    }

    // IPv6 loopback. Some parsers return "::1", others include brackets.
    let host = host.trim_matches(|c| c == '[' || c == ']');
    match host.parse::<std::net::IpAddr>() {
        Ok(ip) => ip.is_loopback(),
        Err(_) => false,
    }
}

async fn handle_connection(mut stream: TcpStream, state: AppState) -> Result<()> {
    let req = read_http_request(&mut stream).await?;

    // Block cross-origin requests to API endpoints.
    // Browsers include Origin for cross-site requests; if it's present and
    // doesn't look like localhost, we reject the request outright.
    if req.path.starts_with("/api/") {
        if let Some(ref origin) = req.origin {
            if !is_localhost_origin(origin) {
                return write_text(
                    &mut stream,
                    403,
                    "Forbidden",
                    "text/plain; charset=utf-8",
                    "cross-origin requests not allowed\n",
                )
                .await;
            }
        }
    }

    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/") => {
            if let Some(bytes) = read_dev_asset("index.html") {
                return write_response(&mut stream, 200, "OK", "text/html; charset=utf-8", &bytes)
                    .await;
            }
            write_response(
                &mut stream,
                200,
                "OK",
                "text/html; charset=utf-8",
                INDEX_HTML.as_bytes(),
            )
            .await
        }
        ("GET", p) if p.starts_with("/assets/") => serve_asset(&mut stream, p).await,
        ("GET", "/api/status") => api_status(&mut stream, state).await,
        ("POST", "/api/models") => api_models(&mut stream, state, &req.body).await,
        ("POST", "/api/chat") => api_chat(&mut stream, state, &req.body).await,
        ("POST", "/api/chat_stream") => api_chat_stream(&mut stream, state, &req.body).await,
        ("POST", "/api/exec") => api_exec(&mut stream, &req.body).await,
        ("POST", "/api/open") => api_open(&mut stream, &req.body).await,
        ("POST", "/api/chat_tools") => api_chat_tools(&mut stream, state, &req.body).await,
        ("POST", "/api/chat_tools_stream") => {
            api_chat_tools_stream(&mut stream, state, &req.body).await
        }
        ("POST", "/api/observer_engine") => api_observer_engine(&mut stream, &req.body).await,
        ("GET", "/api/pending_edits") => api_pending_edits(&mut stream, state).await,
        ("POST", "/api/queue_edit") => api_queue_edit(&mut stream, state, &req.body).await,
        ("POST", "/api/approve_edit") => api_approve_edit(&mut stream, state, &req.body).await,
        ("POST", "/api/reject_edit") => api_reject_edit(&mut stream, state, &req.body).await,
        ("GET", "/api/pending_commands") => api_pending_commands(&mut stream, state).await,
        ("POST", "/api/queue_command") => api_queue_command(&mut stream, state, &req.body).await,
        ("POST", "/api/approve_command") => {
            api_approve_command(&mut stream, state, &req.body).await
        }
        ("POST", "/api/reject_command") => api_reject_command(&mut stream, state, &req.body).await,
        ("GET", "/api/meta_prompts") => api_meta_prompts_get(&mut stream, state).await,
        ("POST", "/api/meta_prompts") => api_meta_prompts_post(&mut stream, state, &req.body).await,
        ("POST", "/api/meta_diagnose/list") => {
            api_meta_diagnose_list(&mut stream, state, &req.body).await
        }
        ("POST", "/api/meta_diagnose/read") => {
            api_meta_diagnose_read(&mut stream, state, &req.body).await
        }
        ("POST", "/api/meta_diagnose/save") => {
            api_meta_diagnose_save(&mut stream, state, &req.body).await
        }
        ("POST", "/api/write_file") => api_write_file(&mut stream, state, &req.body).await,
        ("POST", "/api/read_file") => api_read_file(&mut stream, state, &req.body).await,
        ("POST", "/api/patch_file") => api_patch_file(&mut stream, state, &req.body).await,
        ("POST", "/api/search_files") => {
            api_search_files_endpoint(&mut stream, state, &req.body).await
        }
        ("POST", "/api/list_dir") => api_list_dir(&mut stream, state, &req.body).await,
        ("POST", "/api/glob_files") => api_glob_files(&mut stream, state, &req.body).await,
        ("POST", "/api/apply_diff") => api_apply_diff(&mut stream, state, &req.body).await,
        ("POST", "/api/stat_path") => api_stat_path(&mut stream, state, &req.body).await,
        ("POST", "/api/rollback") => api_rollback(&mut stream, state, &req.body).await,
        ("GET", "/api/governor_contract") => api_governor_contract(&mut stream, state).await,
        ("GET", "/api/merge_gate") => api_merge_gate(&mut stream, state).await,
        ("POST", "/api/merge_gate/approve") => {
            api_merge_gate_approve(&mut stream, state, &req.body).await
        }
        ("POST", "/api/merge_gate/hold") => {
            api_merge_gate_hold(&mut stream, state, &req.body).await
        }
        ("GET", "/api/harness_promotions") => api_harness_promotions(&mut stream, state).await,
        ("POST", "/api/harness_promotions/approve") => {
            api_harness_promotions_approve(&mut stream, state, &req.body).await
        }
        ("POST", "/api/harness_promotions/hold") => {
            api_harness_promotions_hold(&mut stream, state, &req.body).await
        }
        ("POST", "/api/harness_promotions/apply") => {
            api_harness_promotions_apply(&mut stream, state, &req.body).await
        }
        ("GET", "/api/project/scan") => api_project_scan(&mut stream, req.target.as_str()).await,
        _ => {
            write_text(
                &mut stream,
                404,
                "Not Found",
                "text/plain; charset=utf-8",
                "not found\n",
            )
            .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        compact_meta_diagnose_timestamp, extract_query_param, is_localhost_origin,
        is_safe_meta_artifact_name, meta_artifact_slug,
    };

    #[test]
    fn localhost_origin_parsing_is_strict() {
        assert!(is_localhost_origin("http://127.0.0.1:18080"));
        assert!(is_localhost_origin("http://localhost:18080"));
        assert!(is_localhost_origin("https://localhost:18080"));
        assert!(is_localhost_origin("http://[::1]:18080"));
        assert!(is_localhost_origin("https://[::1]:18080"));

        assert!(!is_localhost_origin("https://localhost.evil.com"));
        assert!(!is_localhost_origin("http://127.0.0.1.evil.com"));
        assert!(!is_localhost_origin("null"));
        assert!(!is_localhost_origin(""));
    }

    #[test]
    fn compact_meta_diagnose_timestamp_formats_filename_stamp() {
        assert_eq!(
            compact_meta_diagnose_timestamp("2026-03-12T01:23:45.678Z").as_deref(),
            Some("20260312T012345Z")
        );
        assert_eq!(
            compact_meta_diagnose_timestamp("2026-03-12T01:23:45Z").as_deref(),
            Some("20260312T012345Z")
        );
        assert!(compact_meta_diagnose_timestamp("bad").is_none());
    }

    #[test]
    fn meta_artifact_slug_sanitizes_filename_components() {
        assert_eq!(meta_artifact_slug("pj-25/abeam", "thread"), "pj-25-abeam");
        assert_eq!(meta_artifact_slug("msg:abc?.json", "msg"), "msg-abc-json");
        assert_eq!(meta_artifact_slug("", "msg"), "msg");
    }

    #[test]
    fn meta_artifact_name_rejects_path_traversal() {
        assert!(is_safe_meta_artifact_name(
            "20260312T012345Z__thread-a__msg-b.json"
        ));
        assert!(!is_safe_meta_artifact_name("../secret.json"));
        assert!(!is_safe_meta_artifact_name("nested/file.json"));
        assert!(!is_safe_meta_artifact_name("/abs.json"));
    }

    #[test]
    fn extract_query_param_reads_root_from_request_target() {
        assert_eq!(
            extract_query_param("/api/project/scan?root=.tmp/thread-1", "root"),
            Some(".tmp/thread-1")
        );
    }
}

async fn api_chat_tools(stream: &mut TcpStream, state: AppState, body: &[u8]) -> Result<()> {
    use serde_json::json;

    #[derive(Deserialize)]
    struct Req {
        messages: Vec<serde_json::Value>,
        tools: Option<Vec<serde_json::Value>>,
        model: String,
        base_url: String,
        api_key: Option<String>,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        timeout_seconds: Option<u64>,
    }

    let req: Req = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => {
            #[derive(Serialize)]
            struct E {
                error: String,
            }
            return write_json(
                stream,
                400,
                "Bad Request",
                &E {
                    error: e.to_string(),
                },
            )
            .await;
        }
    };

    let base_url = req.base_url.trim_end_matches('/').to_string();
    let messages_for_prompt = req.messages.clone();

    let mut payload = json!({
        "model": req.model,
        "messages": req.messages,
        "temperature": req.temperature.unwrap_or(0.7),
        "max_tokens": req.max_tokens.unwrap_or(4096),
    });
    apply_openai_temperature_compat(&mut payload, &base_url, &req.model);

    if let Some(tools) = &req.tools {
        if !tools.is_empty() {
            payload["tools"] = json!(tools);
            // Prefer forcing tool calls for agentic execution; if a provider rejects this,
            // we will retry once without tool_choice.
            payload["tool_choice"] = json!("required");
        }
    }

    let timeout = Duration::from_secs(req.timeout_seconds.unwrap_or(120));
    let mut last_err: Option<String> = None;
    let mut want_completions = false;

    let payload_cur = payload.clone();
    let mut ok_json: Option<serde_json::Value> = None;

    for url in openai_compat_chat_urls(&base_url) {
        let mut http_req = state
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .timeout(timeout)
            .json(&payload_cur);

        if let Some(key) = req.api_key.as_deref().filter(|k| !k.is_empty()) {
            http_req = http_req.bearer_auth(key);
        }

        let resp = match http_req.send().await {
            Ok(r) => r,
            Err(e) => {
                #[derive(Serialize)]
                struct E {
                    error: String,
                }
                return write_json(
                    stream,
                    502,
                    "Bad Gateway",
                    &E {
                        error: e.to_string(),
                    },
                )
                .await;
            }
        };

        let status = resp.status();
        let resp_text = resp.text().await.unwrap_or_default();

        if status.is_success() {
            match serde_json::from_str(&resp_text) {
                Ok(v) => {
                    ok_json = Some(v);
                    break;
                }
                Err(e) => {
                    #[derive(Serialize)]
                    struct E {
                        error: String,
                    }
                    return write_json(
                        stream,
                        502,
                        "Bad Gateway",
                        &E {
                            error: format!("invalid JSON from API: {e}"),
                        },
                    )
                    .await;
                }
            }
        }

        // Retry once with max_completion_tokens if suggested.
        if should_swap_to_max_completion_tokens(status, &resp_text)
            && payload_cur.get("max_tokens").is_some()
        {
            let mut payload2 = payload_cur.clone();
            swap_max_tokens_to_max_completion_tokens(&mut payload2);

            let mut http_req2 = state
                .client
                .post(&url)
                .header("Content-Type", "application/json")
                .timeout(timeout)
                .json(&payload2);
            if let Some(key) = req.api_key.as_deref().filter(|k| !k.is_empty()) {
                http_req2 = http_req2.bearer_auth(key);
            }

            let resp2 = match http_req2.send().await {
                Ok(r) => r,
                Err(e) => {
                    #[derive(Serialize)]
                    struct E {
                        error: String,
                    }
                    return write_json(
                        stream,
                        502,
                        "Bad Gateway",
                        &E {
                            error: e.to_string(),
                        },
                    )
                    .await;
                }
            };
            let status2 = resp2.status();
            let resp_text2 = resp2.text().await.unwrap_or_default();
            if status2.is_success() {
                match serde_json::from_str(&resp_text2) {
                    Ok(v) => {
                        ok_json = Some(v);
                        break;
                    }
                    Err(e) => {
                        #[derive(Serialize)]
                        struct E {
                            error: String,
                        }
                        return write_json(
                            stream,
                            502,
                            "Bad Gateway",
                            &E {
                                error: format!("invalid JSON from API: {e}"),
                            },
                        )
                        .await;
                    }
                }
            }
            last_err = Some(format!("API error (HTTP {status2}): {resp_text2}"));
            continue;
        }

        if should_use_v1_completions(status, &resp_text) {
            want_completions = true;
            last_err = Some(format!("API error (HTTP {status}): {resp_text}"));
            break;
        }

        if status == reqwest::StatusCode::NOT_FOUND {
            last_err = Some(format!("API error (HTTP {status}): {resp_text}"));
            continue;
        }

        #[derive(Serialize)]
        struct E {
            error: String,
        }
        return write_json(
            stream,
            502,
            "Bad Gateway",
            &E {
                error: format!("API error (HTTP {status}): {resp_text}"),
            },
        )
        .await;
    }

    if ok_json.is_none() && want_completions {
        let url = format!("{}/completions", base_url);
        let mut comp_payload = json!({
            "model": req.model,
            "prompt": prompt_from_chat_messages(&messages_for_prompt),
            "temperature": req.temperature.unwrap_or(0.7),
            "max_tokens": req.max_tokens.unwrap_or(4096),
        });
        apply_openai_temperature_compat(&mut comp_payload, &base_url, &req.model);
        let mut http_req = state
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .timeout(timeout)
            .json(&comp_payload);
        if let Some(key) = req.api_key.as_deref().filter(|k| !k.is_empty()) {
            http_req = http_req.bearer_auth(key);
        }
        let resp = match http_req.send().await {
            Ok(r) => r,
            Err(e) => {
                #[derive(Serialize)]
                struct E {
                    error: String,
                }
                return write_json(
                    stream,
                    502,
                    "Bad Gateway",
                    &E {
                        error: e.to_string(),
                    },
                )
                .await;
            }
        };
        let status = resp.status();
        let resp_text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            if should_swap_to_max_completion_tokens(status, &resp_text)
                && comp_payload.get("max_tokens").is_some()
            {
                swap_max_tokens_to_max_completion_tokens(&mut comp_payload);
                let mut http_req2 = state
                    .client
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .timeout(timeout)
                    .json(&comp_payload);
                if let Some(key) = req.api_key.as_deref().filter(|k| !k.is_empty()) {
                    http_req2 = http_req2.bearer_auth(key);
                }
                let resp2 = match http_req2.send().await {
                    Ok(r) => r,
                    Err(e) => {
                        #[derive(Serialize)]
                        struct E {
                            error: String,
                        }
                        return write_json(
                            stream,
                            502,
                            "Bad Gateway",
                            &E {
                                error: e.to_string(),
                            },
                        )
                        .await;
                    }
                };
                let status2 = resp2.status();
                let resp_text2 = resp2.text().await.unwrap_or_default();
                if !status2.is_success() {
                    #[derive(Serialize)]
                    struct E {
                        error: String,
                    }
                    return write_json(
                        stream,
                        502,
                        "Bad Gateway",
                        &E {
                            error: format!("API error (HTTP {status2}): {resp_text2}"),
                        },
                    )
                    .await;
                }
                ok_json = Some(serde_json::from_str(&resp_text2).context("invalid JSON from API")?);
            } else {
                #[derive(Serialize)]
                struct E {
                    error: String,
                }
                return write_json(
                    stream,
                    502,
                    "Bad Gateway",
                    &E {
                        error: format!("API error (HTTP {status}): {resp_text}"),
                    },
                )
                .await;
            }
        } else {
            ok_json = Some(serde_json::from_str(&resp_text).context("invalid JSON from API")?);
        }
    }

    if let Some(v) = ok_json {
        write_json(stream, 200, "OK", &v).await
    } else {
        #[derive(Serialize)]
        struct E {
            error: String,
        }
        write_json(
            stream,
            502,
            "Bad Gateway",
            &E {
                error: last_err.unwrap_or_else(|| "API request failed".to_string()),
            },
        )
        .await
    }
}

async fn api_observer_engine(stream: &mut TcpStream, body: &[u8]) -> Result<()> {
    #[derive(Deserialize)]
    struct Req {
        messages: Option<Vec<serde_json::Value>>,
        transcript: Option<String>,
        memory: Option<crate::observer::memory::CritiqueMemory>,
    }

    #[derive(Serialize)]
    struct Resp {
        critique: crate::observer::Critique,
        formatted: String,
        memory: crate::observer::memory::CritiqueMemory,
    }

    let req: Req = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await;
        }
    };

    let mut mem = req.memory.unwrap_or_default();

    let critique = if let Some(messages) = req.messages {
        crate::observer::engine::run_observer(&messages, Some(&mut mem))
    } else if let Some(transcript) = req.transcript {
        crate::observer::engine::run_observer_from_transcript(&transcript, Some(&mut mem))
    } else {
        return write_json(
            stream,
            400,
            "Bad Request",
            &ApiError {
                error: "missing required field: messages or transcript".to_string(),
            },
        )
        .await;
    };

    let formatted = crate::observer::engine::format_critique_as_observer_blocks(&critique);

    write_json(
        stream,
        200,
        "OK",
        &Resp {
            critique,
            formatted,
            memory: mem,
        },
    )
    .await
}

async fn api_chat_tools_stream(stream: &mut TcpStream, state: AppState, body: &[u8]) -> Result<()> {
    use serde_json::json;
    use std::collections::HashMap;

    #[derive(Deserialize)]
    struct Req {
        messages: Vec<serde_json::Value>,
        tools: Option<Vec<serde_json::Value>>,
        model: String,
        base_url: String,
        api_key: Option<String>,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        timeout_seconds: Option<u64>,
        tool_root: Option<String>,
    }

    let req: Req = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => {
            #[derive(Serialize)]
            struct E {
                error: String,
            }
            return write_json(
                stream,
                400,
                "Bad Request",
                &E {
                    error: e.to_string(),
                },
            )
            .await;
        }
    };

    let base_url = req.base_url.trim_end_matches('/').to_string();

    // Project context + AGENTS.md injection for web UI path.
    let injected_messages = {
        let mut msgs = req.messages.clone();
        if let Some(ref root) = req.tool_root {
            let root = root.trim();
            if !root.is_empty() {
                if let Some(ctx) = crate::project::ProjectContext::scan(root).await {
                    // 1. Project context (stack, git, tree).
                    let txt = ctx.to_context_text();
                    if !txt.is_empty() {
                        let pos = msgs
                            .iter()
                            .position(|m| m["role"] == "system")
                            .map(|i| i + 1)
                            .unwrap_or(0);
                        msgs.insert(pos, serde_json::json!({"role":"system","content": txt}));
                    }
                    // 2. AGENTS.md / .spiral-coder.md — project-specific rules.
                    if let Some(agents_text) = ctx.agents_md {
                        if !agents_text.is_empty() {
                            let pos = msgs
                                .iter()
                                .position(|m| m["role"] == "system")
                                .map(|i| i + 2)
                                .unwrap_or(1);
                            msgs.insert(pos.min(msgs.len()), serde_json::json!({
                                "role": "system",
                                "content": format!("[Project Instructions — .spiral-coder.md / AGENTS.md]\n{agents_text}")
                            }));
                        }
                    }
                }
            }
        }
        msgs
    };

    // Expand @file references found in the last user message.
    let injected_messages = {
        let mut msgs = injected_messages;
        if let Some(ref root) = req.tool_root {
            let root_str = root.trim();
            if !root_str.is_empty() {
                if let Some(last_user_pos) = msgs.iter().rposition(|m| m["role"] == "user") {
                    let user_text = msgs[last_user_pos]["content"]
                        .as_str()
                        .unwrap_or("")
                        .to_string();
                    let at_refs = parse_at_refs_str(&user_text);
                    if !at_refs.is_empty() {
                        let mut inject_msgs: Vec<serde_json::Value> = Vec::new();
                        for ref_path in &at_refs {
                            let (content, is_err) =
                                crate::file_tools::tool_read_file(ref_path, Some(root_str));
                            if !is_err {
                                inject_msgs.push(serde_json::json!({
                                    "role": "system",
                                    "content": format!("[@{ref_path}]\n{content}")
                                }));
                            }
                        }
                        for (i, msg) in inject_msgs.into_iter().enumerate() {
                            msgs.insert(last_user_pos + i, msg);
                        }
                    }
                }
            }
        }
        msgs
    };

    let messages_for_prompt = injected_messages.clone();

    let mut payload = json!({
        "model": req.model,
        "messages": injected_messages,
        "temperature": req.temperature.unwrap_or(0.7),
        "max_tokens": req.max_tokens.unwrap_or(4096),
        "stream": true,
    });
    apply_openai_temperature_compat(&mut payload, &base_url, &req.model);

    if let Some(tools) = &req.tools {
        if !tools.is_empty() {
            payload["tools"] = json!(tools);
            payload["tool_choice"] = json!("required");
        }
    }

    let timeout = Duration::from_secs(req.timeout_seconds.unwrap_or(120));
    let mut last_err: Option<String> = None;
    let mut want_completions = false;
    let mut resp: Option<reqwest::Response> = None;

    const MAX_CONNECT_RETRIES: usize = 3;

    for url in openai_compat_chat_urls(&base_url) {
        let mut payload_try = payload.clone();
        let mut attempt: usize = 0;

        loop {
            let mut http_req = state
                .client
                .post(&url)
                .header("Content-Type", "application/json")
                .timeout(timeout)
                .json(&payload_try);

            if let Some(key) = req.api_key.as_deref().filter(|k| !k.is_empty()) {
                http_req = http_req.bearer_auth(key);
            }

            let r = match http_req.send().await {
                Ok(r) => r,
                Err(e) => {
                    if attempt < MAX_CONNECT_RETRIES && is_retryable_send_error(&e) {
                        last_err = Some(e.to_string());
                        let d = backoff_delay(attempt, None);
                        attempt = attempt.saturating_add(1);
                        tokio::time::sleep(d).await;
                        continue;
                    }
                    #[derive(Serialize)]
                    struct E {
                        error: String,
                    }
                    return write_json(
                        stream,
                        502,
                        "Bad Gateway",
                        &E {
                            error: e.to_string(),
                        },
                    )
                    .await;
                }
            };

            let status = r.status();
            if status.is_success() {
                resp = Some(r);
                break;
            }

            let ra = retry_after_duration(r.headers());
            let body_text = r.text().await.unwrap_or_default();

            if should_swap_to_max_completion_tokens(status, &body_text)
                && payload_try.get("max_tokens").is_some()
            {
                swap_max_tokens_to_max_completion_tokens(&mut payload_try);
                continue;
            }

            // Some providers reject tool_choice. Retry once without it.
            if status == reqwest::StatusCode::BAD_REQUEST
                && body_text.to_ascii_lowercase().contains("tool_choice")
                && (body_text.to_ascii_lowercase().contains("unsupported")
                    || body_text.to_ascii_lowercase().contains("unknown"))
            {
                if let Some(obj) = payload_try.as_object_mut() {
                    obj.remove("tool_choice");
                }
                continue;
            }

            if should_use_v1_completions(status, &body_text) {
                want_completions = true;
                last_err = Some(format!("API error (HTTP {status}): {body_text}"));
                break;
            }

            if status == reqwest::StatusCode::NOT_FOUND {
                last_err = Some(format!("API error (HTTP {status}): {body_text}"));
                break;
            }

            if attempt < MAX_CONNECT_RETRIES && is_retryable_status(status) {
                last_err = Some(format!("API error (HTTP {status}): {body_text}"));
                let d = backoff_delay(attempt, ra);
                attempt = attempt.saturating_add(1);
                tokio::time::sleep(d).await;
                continue;
            }

            #[derive(Serialize)]
            struct E {
                error: String,
            }
            return write_json(
                stream,
                502,
                "Bad Gateway",
                &E {
                    error: format!("API error (HTTP {status}): {body_text}"),
                },
            )
            .await;
        }

        if resp.is_some() || want_completions {
            break;
        }
    }

    if resp.is_none() && want_completions {
        let url = format!("{}/completions", base_url);
        let mut comp_payload = json!({
            "model": req.model,
            "prompt": prompt_from_chat_messages(&messages_for_prompt),
            "temperature": req.temperature.unwrap_or(0.7),
            "max_tokens": req.max_tokens.unwrap_or(4096),
            "stream": true,
        });
        apply_openai_temperature_compat(&mut comp_payload, &base_url, &req.model);

        let mut http_req = state
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .timeout(timeout)
            .json(&comp_payload);
        if let Some(key) = req.api_key.as_deref().filter(|k| !k.is_empty()) {
            http_req = http_req.bearer_auth(key);
        }
        let r = match http_req.send().await {
            Ok(r) => r,
            Err(e) => {
                #[derive(Serialize)]
                struct E {
                    error: String,
                }
                return write_json(
                    stream,
                    502,
                    "Bad Gateway",
                    &E {
                        error: e.to_string(),
                    },
                )
                .await;
            }
        };
        let status = r.status();
        if status.is_success() {
            resp = Some(r);
        } else {
            let body_text = r.text().await.unwrap_or_default();
            if should_swap_to_max_completion_tokens(status, &body_text)
                && comp_payload.get("max_tokens").is_some()
            {
                swap_max_tokens_to_max_completion_tokens(&mut comp_payload);
                let mut http_req2 = state
                    .client
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .timeout(timeout)
                    .json(&comp_payload);
                if let Some(key) = req.api_key.as_deref().filter(|k| !k.is_empty()) {
                    http_req2 = http_req2.bearer_auth(key);
                }
                let r2 = match http_req2.send().await {
                    Ok(r) => r,
                    Err(e) => {
                        #[derive(Serialize)]
                        struct E {
                            error: String,
                        }
                        return write_json(
                            stream,
                            502,
                            "Bad Gateway",
                            &E {
                                error: e.to_string(),
                            },
                        )
                        .await;
                    }
                };
                let status2 = r2.status();
                if status2.is_success() {
                    resp = Some(r2);
                } else {
                    let body2 = r2.text().await.unwrap_or_default();
                    #[derive(Serialize)]
                    struct E {
                        error: String,
                    }
                    return write_json(
                        stream,
                        502,
                        "Bad Gateway",
                        &E {
                            error: format!("API error (HTTP {status2}): {body2}"),
                        },
                    )
                    .await;
                }
            } else {
                #[derive(Serialize)]
                struct E {
                    error: String,
                }
                return write_json(
                    stream,
                    502,
                    "Bad Gateway",
                    &E {
                        error: format!("API error (HTTP {status}): {body_text}"),
                    },
                )
                .await;
            }
        }
    }

    let resp = match resp {
        Some(r) => r,
        None => {
            #[derive(Serialize)]
            struct E {
                error: String,
            }
            return write_json(
                stream,
                502,
                "Bad Gateway",
                &E {
                    error: last_err.unwrap_or_else(|| "API request failed".to_string()),
                },
            )
            .await;
        }
    };

    // Headers OK — switch to SSE mode.
    write_sse_header(stream, 200, "OK").await?;

    #[derive(Default)]
    struct TcAcc {
        id: String,
        name: String,
        arguments: String,
    }

    let mut tool_calls: HashMap<usize, TcAcc> = HashMap::new();
    let mut finish_reason = String::new();
    let mut buf: Vec<u8> = Vec::new();
    let mut done = false;

    let mut resp = resp;
    while let Some(chunk) = resp.chunk().await? {
        if done {
            break;
        }
        buf.extend_from_slice(&chunk);

        while let Some(frame) = take_next_sse_frame(&mut buf) {
            let frame_str = String::from_utf8_lossy(&frame);
            let mut data_lines: Vec<&str> = Vec::new();
            for line in frame_str.split('\n') {
                let line = line.trim_end_matches('\r');
                if line.is_empty() || line.starts_with(':') {
                    continue;
                }
                if let Some(rest) = line.strip_prefix("data:") {
                    data_lines.push(rest.trim_start());
                }
            }
            if data_lines.is_empty() {
                continue;
            }
            let data = data_lines.join("\n");
            if data.trim() == "[DONE]" {
                done = true;
                break;
            }

            let v: serde_json::Value = match serde_json::from_str(&data) {
                Ok(v) => v,
                Err(_) => continue,
            };

            if let Some(err) = v.get("error") {
                let d = serde_json::to_string(&json!({"error": err.to_string()}))?;
                write_sse_event(stream, "error", &d).await?;
                write_sse_event(stream, "done", "{}").await?;
                return Ok(());
            }

            // Track finish_reason.
            if let Some(fr) = v
                .pointer("/choices/0/finish_reason")
                .and_then(|x| x.as_str())
            {
                if !fr.is_empty() {
                    finish_reason = fr.to_string();
                }
            }

            // Text delta.
            let delta_text = v
                .pointer("/choices/0/delta/content")
                .and_then(|x| x.as_str())
                .or_else(|| v.pointer("/choices/0/delta/text").and_then(|x| x.as_str()))
                .or_else(|| v.pointer("/choices/0/text").and_then(|x| x.as_str()))
                .unwrap_or("");
            if !delta_text.is_empty() {
                let d = serde_json::to_string(&json!({"delta": delta_text}))?;
                write_sse_event(stream, "delta", &d).await?;
            }

            // Tool-call delta accumulation (OpenAI streaming format).
            if let Some(tc_arr) = v
                .pointer("/choices/0/delta/tool_calls")
                .and_then(|x| x.as_array())
            {
                for tc in tc_arr {
                    let idx = tc["index"].as_u64().unwrap_or(0) as usize;
                    let acc = tool_calls.entry(idx).or_default();
                    if let Some(id) = tc["id"].as_str() {
                        if !id.is_empty() {
                            acc.id = id.to_string();
                        }
                    }
                    if let Some(nm) = tc.pointer("/function/name").and_then(|x| x.as_str()) {
                        acc.name.push_str(nm);
                    }
                    if let Some(args) = tc.pointer("/function/arguments").and_then(|x| x.as_str()) {
                        acc.arguments.push_str(args);
                    }
                }
            }
        }
    }

    // Emit finish event with accumulated tool calls.
    let mut tc_sorted: Vec<usize> = tool_calls.keys().copied().collect();
    tc_sorted.sort_unstable();
    let tc_json: Vec<serde_json::Value> = tc_sorted
        .iter()
        .map(|idx| {
            let tc = &tool_calls[idx];
            json!({
                "id": tc.id,
                "type": "function",
                "function": { "name": tc.name, "arguments": tc.arguments }
            })
        })
        .collect();

    let finish_data = serde_json::to_string(&json!({
        "finish_reason": finish_reason,
        "tool_calls": tc_json,
    }))?;
    write_sse_event(stream, "finish", &finish_data).await?;
    write_sse_event(stream, "done", "{}").await?;
    Ok(())
}

async fn api_exec(stream: &mut TcpStream, body: &[u8]) -> Result<()> {
    #[derive(Deserialize)]
    struct Req {
        command: String,
        cwd: Option<String>,
    }
    #[derive(Serialize)]
    struct Res {
        stdout: String,
        stderr: String,
        exit_code: i32,
        duration_ms: u64,
    }

    let req: Req = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(_) => {
            #[derive(Serialize)]
            struct E {
                error: String,
            }
            return write_json(
                stream,
                400,
                "Bad Request",
                &E {
                    error: "invalid JSON".into(),
                },
            )
            .await;
        }
    };

    let cmd_str = req.command.trim();
    if cmd_str.is_empty() {
        #[derive(Serialize)]
        struct E {
            error: String,
        }
        return write_json(
            stream,
            400,
            "Bad Request",
            &E {
                error: "command is empty".into(),
            },
        )
        .await;
    }

    if let Some(cwd) = req.cwd.as_deref().filter(|s| !s.trim().is_empty()) {
        if let Err(err) = crate::exec::validate_cwd(cwd) {
            #[derive(Serialize)]
            struct E {
                error: String,
            }
            return write_json(
                stream,
                400,
                "Bad Request",
                &E {
                    error: format!("invalid cwd: {err}"),
                },
            )
            .await;
        }
        let cwd_path = Path::new(cwd);
        if let Err(err) = std::fs::create_dir_all(cwd_path) {
            #[derive(Serialize)]
            struct E {
                error: String,
            }
            return write_json(
                stream,
                400,
                "Bad Request",
                &E {
                    error: format!("invalid cwd (create_dir_all failed): {err}"),
                },
            )
            .await;
        }
    }

    // Delegate to shared command runner for:
    // - prompt marker stripping ($ / PS> / >)
    // - dangerous command blocklist
    // - poison proxy env scrubbing
    // - Windows output decoding (CP932 fallback)
    let start = Instant::now();
    let r = crate::exec::run_command(&req.command, req.cwd.as_deref()).await;
    let duration_ms: u64 = start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let out = match r {
        Ok(r) => Res {
            stdout: r.stdout,
            stderr: r.stderr,
            exit_code: r.exit_code,
            duration_ms,
        },
        Err(e) => Res {
            stdout: String::new(),
            stderr: format!("spawn failed: {e:#}"),
            exit_code: -1,
            duration_ms,
        },
    };
    write_json(stream, 200, "OK", &out).await
}

async fn api_pending_commands(stream: &mut TcpStream, state: AppState) -> Result<()> {
    #[derive(Serialize)]
    struct Res {
        pending: Vec<crate::pending_commands::PendingCommandView>,
    }
    let pending = state.pending_commands.list().await;
    write_json(stream, 200, "OK", &Res { pending }).await
}

async fn api_governor_contract(stream: &mut TcpStream, state: AppState) -> Result<()> {
    let path = state.workspace_root.join("shared/governor_contract.json");
    if let Ok(contract) = crate::governor_contract::load_from_path(&path) {
        write_json(stream, 200, "OK", &contract).await
    } else {
        write_json(stream, 200, "OK", crate::governor_contract::contract()).await
    }
}

async fn api_harness_promotions(stream: &mut TcpStream, state: AppState) -> Result<()> {
    match crate::harness_gate::load_board(&state.workspace_root) {
        Ok(board) => write_json(stream, 200, "OK", &board).await,
        Err(e) => {
            write_json(
                stream,
                500,
                "Internal Server Error",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await
        }
    }
}

async fn api_merge_gate(stream: &mut TcpStream, state: AppState) -> Result<()> {
    match crate::merge_gate::load_board(&state.workspace_root) {
        Ok(board) => write_json(stream, 200, "OK", &board).await,
        Err(e) => {
            write_json(
                stream,
                500,
                "Internal Server Error",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await
        }
    }
}

async fn api_merge_gate_approve(
    stream: &mut TcpStream,
    state: AppState,
    body: &[u8],
) -> Result<()> {
    api_merge_gate_action(stream, state, body, crate::merge_gate::approve).await
}

async fn api_merge_gate_hold(stream: &mut TcpStream, state: AppState, body: &[u8]) -> Result<()> {
    api_merge_gate_action(stream, state, body, crate::merge_gate::hold).await
}

async fn api_merge_gate_action(
    stream: &mut TcpStream,
    state: AppState,
    body: &[u8],
    action: fn(
        &std::path::Path,
        &str,
    ) -> anyhow::Result<crate::merge_gate::MergeGateActionResponse>,
) -> Result<()> {
    #[derive(Deserialize)]
    struct Req {
        id: String,
    }

    let req: Req = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await;
        }
    };
    let id = req.id.trim();
    if id.is_empty() {
        return write_json(
            stream,
            400,
            "Bad Request",
            &ApiError {
                error: "id is required".into(),
            },
        )
        .await;
    }

    match action(&state.workspace_root, id) {
        Ok(response) => write_json(stream, 200, "OK", &response).await,
        Err(e) => {
            write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await
        }
    }
}

async fn api_harness_promotions_approve(
    stream: &mut TcpStream,
    state: AppState,
    body: &[u8],
) -> Result<()> {
    api_harness_promotions_action(stream, state, body, crate::harness_gate::approve).await
}

async fn api_harness_promotions_hold(
    stream: &mut TcpStream,
    state: AppState,
    body: &[u8],
) -> Result<()> {
    api_harness_promotions_action(stream, state, body, crate::harness_gate::hold).await
}

async fn api_harness_promotions_apply(
    stream: &mut TcpStream,
    state: AppState,
    body: &[u8],
) -> Result<()> {
    api_harness_promotions_action(stream, state, body, crate::harness_gate::apply_to_contract).await
}

async fn api_harness_promotions_action(
    stream: &mut TcpStream,
    state: AppState,
    body: &[u8],
    action: fn(
        &std::path::Path,
        &str,
    ) -> anyhow::Result<crate::harness_gate::HarnessPromotionActionResponse>,
) -> Result<()> {
    #[derive(Deserialize)]
    struct Req {
        id: String,
    }

    let req: Req = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await;
        }
    };
    let id = req.id.trim();
    if id.is_empty() {
        return write_json(
            stream,
            400,
            "Bad Request",
            &ApiError {
                error: "id is required".into(),
            },
        )
        .await;
    }

    match action(&state.workspace_root, id) {
        Ok(response) => write_json(stream, 200, "OK", &response).await,
        Err(e) => {
            write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await
        }
    }
}

async fn api_queue_command(stream: &mut TcpStream, state: AppState, body: &[u8]) -> Result<()> {
    #[derive(Deserialize)]
    struct Req {
        command: String,
        cwd: Option<String>,
    }
    #[derive(Serialize)]
    struct Res {
        ok: bool,
        approval_id: String,
    }

    let req: Req = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await;
        }
    };

    let id = match state.pending_commands.queue(&req.command, req.cwd).await {
        Ok(id) => id,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await;
        }
    };

    write_json(
        stream,
        200,
        "OK",
        &Res {
            ok: true,
            approval_id: id,
        },
    )
    .await
}

async fn api_approve_command(stream: &mut TcpStream, state: AppState, body: &[u8]) -> Result<()> {
    #[derive(Deserialize)]
    struct Req {
        id: String,
    }
    let req: Req = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await;
        }
    };
    let id = req.id.trim();
    if id.is_empty() {
        return write_json(
            stream,
            400,
            "Bad Request",
            &ApiError {
                error: "id is required".into(),
            },
        )
        .await;
    }

    let item = match state.pending_commands.approve(id).await {
        Ok(it) => it,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await;
        }
    };
    write_json(
        stream,
        200,
        "OK",
        &crate::pending_commands::PendingCommandResolveResponse { ok: true, item },
    )
    .await
}

async fn api_reject_command(stream: &mut TcpStream, state: AppState, body: &[u8]) -> Result<()> {
    #[derive(Deserialize)]
    struct Req {
        id: String,
    }
    let req: Req = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await;
        }
    };
    let id = req.id.trim();
    if id.is_empty() {
        return write_json(
            stream,
            400,
            "Bad Request",
            &ApiError {
                error: "id is required".into(),
            },
        )
        .await;
    }

    let item = match state.pending_commands.reject(id).await {
        Ok(it) => it,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await;
        }
    };
    write_json(
        stream,
        200,
        "OK",
        &crate::pending_commands::PendingCommandResolveResponse { ok: true, item },
    )
    .await
}

async fn api_pending_edits(stream: &mut TcpStream, state: AppState) -> Result<()> {
    #[derive(Serialize)]
    struct Res {
        pending: Vec<crate::pending_edits::PendingEditView>,
    }
    let pending = state.pending_edits.list().await;
    write_json(stream, 200, "OK", &Res { pending }).await
}

async fn api_queue_edit(stream: &mut TcpStream, state: AppState, body: &[u8]) -> Result<()> {
    #[derive(Deserialize)]
    struct Req {
        action: Option<String>,
        path: String,
        content: String,
    }
    #[derive(Serialize)]
    struct Res {
        ok: bool,
        approval_id: String,
    }

    let req: Req = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await;
        }
    };

    let action = req.action.unwrap_or_else(|| "write_file".to_string());
    let path = req.path.trim();
    if path.is_empty() {
        return write_json(
            stream,
            400,
            "Bad Request",
            &ApiError {
                error: "path is required".into(),
            },
        )
        .await;
    }
    let approval_id = match state
        .pending_edits
        .queue_write_file(&state.workspace_root, path, &req.content, &action)
        .await
    {
        Ok(id) => id,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await;
        }
    };

    write_json(
        stream,
        200,
        "OK",
        &Res {
            ok: true,
            approval_id,
        },
    )
    .await
}

async fn api_approve_edit(stream: &mut TcpStream, state: AppState, body: &[u8]) -> Result<()> {
    #[derive(Deserialize)]
    struct Req {
        id: String,
    }

    let req: Req = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await;
        }
    };
    let id = req.id.trim();
    if id.is_empty() {
        return write_json(
            stream,
            400,
            "Bad Request",
            &ApiError {
                error: "id is required".into(),
            },
        )
        .await;
    }

    let item = match state.pending_edits.approve(&state.workspace_root, id).await {
        Ok(it) => it,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await;
        }
    };
    write_json(
        stream,
        200,
        "OK",
        &crate::pending_edits::PendingEditResolveResponse { ok: true, item },
    )
    .await
}

async fn api_reject_edit(stream: &mut TcpStream, state: AppState, body: &[u8]) -> Result<()> {
    #[derive(Deserialize)]
    struct Req {
        id: String,
    }

    let req: Req = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await;
        }
    };
    let id = req.id.trim();
    if id.is_empty() {
        return write_json(
            stream,
            400,
            "Bad Request",
            &ApiError {
                error: "id is required".into(),
            },
        )
        .await;
    }

    let item = match state.pending_edits.reject(id).await {
        Ok(it) => it,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await;
        }
    };
    write_json(
        stream,
        200,
        "OK",
        &crate::pending_edits::PendingEditResolveResponse { ok: true, item },
    )
    .await
}

fn meta_prompts_rel_path() -> &'static str {
    ".spiral-coder/meta_prompts.json"
}

fn load_meta_prompts(workspace_root: &Path) -> serde_json::Value {
    let p = workspace_root.join(meta_prompts_rel_path());
    let Ok(s) = std::fs::read_to_string(&p) else {
        return serde_json::json!({});
    };
    serde_json::from_str(&s).unwrap_or_else(|_| serde_json::json!({}))
}

async fn api_meta_prompts_get(stream: &mut TcpStream, state: AppState) -> Result<()> {
    #[derive(Serialize)]
    struct Res {
        ok: bool,
        path: &'static str,
        prompts: serde_json::Value,
    }
    let prompts = load_meta_prompts(&state.workspace_root);
    write_json(
        stream,
        200,
        "OK",
        &Res {
            ok: true,
            path: meta_prompts_rel_path(),
            prompts,
        },
    )
    .await
}

async fn api_meta_prompts_post(stream: &mut TcpStream, state: AppState, body: &[u8]) -> Result<()> {
    #[derive(Deserialize)]
    struct Req {
        op: String,     // set | append
        target: String, // coder | observer
        text: String,
    }
    #[derive(Serialize)]
    struct Res {
        ok: bool,
        approval_id: String,
    }

    let req: Req = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await;
        }
    };

    let op = req.op.trim().to_ascii_lowercase();
    let target = req.target.trim().to_ascii_lowercase();
    let text = req.text;
    if op != "set" && op != "append" {
        return write_json(
            stream,
            400,
            "Bad Request",
            &ApiError {
                error: "op must be 'set' or 'append'".into(),
            },
        )
        .await;
    }
    if target != "coder" && target != "observer" {
        return write_json(
            stream,
            400,
            "Bad Request",
            &ApiError {
                error: "target must be 'coder' or 'observer'".into(),
            },
        )
        .await;
    }
    if text.len() > 20000 {
        return write_json(
            stream,
            400,
            "Bad Request",
            &ApiError {
                error: "text too large".into(),
            },
        )
        .await;
    }

    let mut cur = load_meta_prompts(&state.workspace_root);
    if !cur.is_object() {
        cur = serde_json::json!({});
    }
    let key = if target == "observer" {
        "observer_system_append"
    } else {
        "coder_system_append"
    };
    let base = cur
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let text = text.trim().to_string();
    let new_val = if op == "append" {
        if base.trim().is_empty() {
            text
        } else {
            format!("{}\n{}", base.trim_end(), text)
        }
    } else {
        text
    };
    cur[key] = serde_json::Value::String(new_val);
    let new_json = serde_json::to_string_pretty(&cur).unwrap_or_else(|_| "{}".to_string());

    let approval_id = match state
        .pending_edits
        .queue_write_file(
            &state.workspace_root,
            meta_prompts_rel_path(),
            &new_json,
            "meta_prompts",
        )
        .await
    {
        Ok(id) => id,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await;
        }
    };

    write_json(
        stream,
        200,
        "OK",
        &Res {
            ok: true,
            approval_id,
        },
    )
    .await
}

fn resolve_api_root_path(workspace_root: &Path, root: Option<&str>) -> Result<PathBuf> {
    let raw = root.unwrap_or(".").trim();
    let raw = if raw.is_empty() { "." } else { raw };
    crate::exec::validate_cwd(raw)?;
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(workspace_root.join(path))
    }
}

fn compact_meta_diagnose_timestamp(ts: &str) -> Option<String> {
    let digits: String = ts.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 14 {
        return None;
    }
    Some(format!("{}T{}Z", &digits[..8], &digits[8..14]))
}

fn meta_artifact_slug(value: &str, fallback: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in value.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            prev_dash = false;
            Some(ch.to_ascii_lowercase())
        } else if ch == '-' || ch == '_' {
            prev_dash = false;
            Some(ch)
        } else if prev_dash {
            None
        } else {
            prev_dash = true;
            Some('-')
        };
        if let Some(ch) = mapped {
            out.push(ch);
        }
    }
    let trimmed = out.trim_matches('-').trim_matches('_');
    let base = if trimmed.is_empty() {
        fallback
    } else {
        trimmed
    };
    base.chars().take(64).collect()
}

fn meta_diagnose_rel_dir() -> &'static str {
    ".spiral-coder/meta-diagnose"
}

fn is_safe_meta_artifact_name(name: &str) -> bool {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.contains('\0') {
        return false;
    }
    let path = Path::new(trimmed);
    if path.is_absolute() {
        return false;
    }
    if !trimmed.ends_with(".json") {
        return false;
    }
    let mut comps = path.components();
    matches!(comps.next(), Some(Component::Normal(_))) && comps.next().is_none()
}

async fn api_meta_diagnose_list(
    stream: &mut TcpStream,
    state: AppState,
    body: &[u8],
) -> Result<()> {
    #[derive(Deserialize)]
    struct Req {
        root: Option<String>,
        limit: Option<usize>,
    }
    #[derive(Serialize)]
    struct Res {
        ok: bool,
        path: String,
        items: Vec<serde_json::Value>,
    }

    let req: Req = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await;
        }
    };

    let root = match resolve_api_root_path(&state.workspace_root, req.root.as_deref()) {
        Ok(p) => p,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: format!("invalid root: {e}"),
                },
            )
            .await;
        }
    };
    let index_path = root.join(meta_diagnose_rel_dir()).join("index.jsonl");
    let limit = req.limit.unwrap_or(120).clamp(1, 500);
    let Ok(text) = std::fs::read_to_string(&index_path) else {
        return write_json(
            stream,
            200,
            "OK",
            &Res {
                ok: true,
                path: index_path.to_string_lossy().into_owned(),
                items: Vec::new(),
            },
        )
        .await;
    };

    let mut items: Vec<serde_json::Value> = text
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }
            let mut value: serde_json::Value = serde_json::from_str(trimmed).ok()?;
            let name = value
                .get("path")
                .and_then(|v| v.as_str())
                .and_then(|p| Path::new(p).file_name().and_then(|x| x.to_str()))
                .unwrap_or("")
                .to_string();
            if let Some(obj) = value.as_object_mut() {
                obj.insert("name".into(), serde_json::Value::String(name));
            }
            Some(value)
        })
        .collect();
    items.reverse();
    items.truncate(limit);

    write_json(
        stream,
        200,
        "OK",
        &Res {
            ok: true,
            path: index_path.to_string_lossy().into_owned(),
            items,
        },
    )
    .await
}

async fn api_meta_diagnose_read(
    stream: &mut TcpStream,
    state: AppState,
    body: &[u8],
) -> Result<()> {
    #[derive(Deserialize)]
    struct Req {
        root: Option<String>,
        name: String,
    }
    #[derive(Serialize)]
    struct Res {
        ok: bool,
        path: String,
        artifact: serde_json::Value,
        raw: String,
        parse_ok: bool,
        parse_error: Option<String>,
    }

    let req: Req = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await;
        }
    };

    if !is_safe_meta_artifact_name(&req.name) {
        return write_json(
            stream,
            400,
            "Bad Request",
            &ApiError {
                error: "invalid artifact name".into(),
            },
        )
        .await;
    }

    let root = match resolve_api_root_path(&state.workspace_root, req.root.as_deref()) {
        Ok(p) => p,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: format!("invalid root: {e}"),
                },
            )
            .await;
        }
    };
    let path = root.join(meta_diagnose_rel_dir()).join(req.name.trim());
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: format!("failed to read artifact: {e}"),
                },
            )
            .await;
        }
    };
    let (artifact, parse_ok, parse_error) = match serde_json::from_str::<serde_json::Value>(&raw) {
        Ok(v) => (v, true, None),
        Err(e) => (
            serde_json::json!({}),
            false,
            Some(format!("invalid artifact json: {e}")),
        ),
    };

    write_json(
        stream,
        200,
        "OK",
        &Res {
            ok: true,
            path: path.to_string_lossy().into_owned(),
            artifact,
            raw,
            parse_ok,
            parse_error,
        },
    )
    .await
}

async fn api_meta_diagnose_save(
    stream: &mut TcpStream,
    state: AppState,
    body: &[u8],
) -> Result<()> {
    use serde_json::json;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Deserialize)]
    struct Req {
        root: Option<String>,
        artifact: serde_json::Value,
    }
    #[derive(Serialize)]
    struct Res {
        ok: bool,
        path: String,
        index_path: String,
    }

    let req: Req = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await;
        }
    };

    if !req.artifact.is_object() {
        return write_json(
            stream,
            400,
            "Bad Request",
            &ApiError {
                error: "artifact must be a JSON object".into(),
            },
        )
        .await;
    }

    let root = match resolve_api_root_path(&state.workspace_root, req.root.as_deref()) {
        Ok(p) => p,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: format!("invalid root: {e}"),
                },
            )
            .await;
        }
    };

    let ts = req
        .artifact
        .get("ts")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let stamp = compact_meta_diagnose_timestamp(&ts).unwrap_or_else(|| {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        format!("unix{secs}")
    });
    let thread_id = req
        .artifact
        .get("thread_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let target_message_id = req
        .artifact
        .get("target_message_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let thread_slug = meta_artifact_slug(&thread_id, "thread");
    let message_slug = meta_artifact_slug(&target_message_id, "msg");
    let dir = root.join(meta_diagnose_rel_dir());
    let artifact_base = dir.join(format!(
        "{stamp}__thread-{thread_slug}__msg-{message_slug}.json"
    ));
    let artifact_path = if artifact_base.exists() {
        let stem = artifact_base
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("meta-diagnose");
        let ext = artifact_base
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("json");
        let mut candidate = artifact_base.clone();
        for idx in 1..1000u32 {
            let next = dir.join(format!("{stem}__{idx:02}.{ext}"));
            if !next.exists() {
                candidate = next;
                break;
            }
        }
        candidate
    } else {
        artifact_base
    };
    if let Err(e) = crate::trace_writer::safe_mkdir(&artifact_path) {
        return write_json(
            stream,
            400,
            "Bad Request",
            &ApiError {
                error: format!("failed to create artifact dir: {e}"),
            },
        )
        .await;
    }

    let artifact_text = match serde_json::to_string_pretty(&req.artifact) {
        Ok(s) => s,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: format!("failed to serialize artifact: {e}"),
                },
            )
            .await;
        }
    };
    if let Err(e) = std::fs::write(&artifact_path, artifact_text.as_bytes()) {
        return write_json(
            stream,
            400,
            "Bad Request",
            &ApiError {
                error: format!("failed to write artifact: {e}"),
            },
        )
        .await;
    }

    let index_path = dir.join("index.jsonl");
    if let Err(e) = crate::trace_writer::safe_mkdir(&index_path) {
        return write_json(
            stream,
            400,
            "Bad Request",
            &ApiError {
                error: format!("failed to create index dir: {e}"),
            },
        )
        .await;
    }
    let mut index = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&index_path)
    {
        Ok(f) => f,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: format!("failed to open index: {e}"),
                },
            )
            .await;
        }
    };
    let index_line = json!({
        "ts": ts,
        "thread_id": thread_id,
        "target_message_id": target_message_id,
        "path": artifact_path.to_string_lossy(),
        "parse_ok": req.artifact.get("parse_ok").cloned().unwrap_or(serde_json::Value::Null),
        "parse_error": req.artifact.get("parse_error").cloned().unwrap_or(serde_json::Value::Null),
        "provider": req.artifact.get("provider").cloned().unwrap_or(serde_json::Value::Null),
        "model": req.artifact.get("model").cloned().unwrap_or(serde_json::Value::Null),
        "primary_failure": req
            .artifact
            .get("diagnosis")
            .and_then(|v| v.get("primary_failure"))
            .cloned()
            .unwrap_or(serde_json::Value::Null),
    });
    let index_text = format!("{index_line}\n");
    if let Err(e) = index.write_all(index_text.as_bytes()) {
        return write_json(
            stream,
            400,
            "Bad Request",
            &ApiError {
                error: format!("failed to append index: {e}"),
            },
        )
        .await;
    }

    write_json(
        stream,
        200,
        "OK",
        &Res {
            ok: true,
            path: artifact_path.to_string_lossy().into_owned(),
            index_path: index_path.to_string_lossy().into_owned(),
        },
    )
    .await
}

async fn api_write_file(stream: &mut TcpStream, state: AppState, body: &[u8]) -> Result<()> {
    #[derive(Deserialize)]
    struct Req {
        path: String,
        content: String,
    }
    #[derive(Serialize)]
    struct Res {
        ok: bool,
        bytes_written: usize,
    }

    let req: Req = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await;
        }
    };
    let rel_path = req.path.trim();
    if rel_path.is_empty() {
        return write_json(
            stream,
            400,
            "Bad Request",
            &ApiError {
                error: "path is required".into(),
            },
        )
        .await;
    }
    let rel = Path::new(rel_path);
    // Local path safety (keep consistent with pending store).
    if rel.is_absolute()
        || rel.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return write_json(
            stream,
            400,
            "Bad Request",
            &ApiError {
                error: format!("unsafe path: {rel_path}"),
            },
        )
        .await;
    }
    let abs = state.workspace_root.join(rel);
    if let Some(parent) = abs.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: format!("create_dir_all failed: {e}"),
                },
            )
            .await;
        }
    }
    if let Err(e) = std::fs::write(&abs, req.content.as_bytes()) {
        return write_json(
            stream,
            400,
            "Bad Request",
            &ApiError {
                error: format!("write failed: {e}"),
            },
        )
        .await;
    }
    write_json(
        stream,
        200,
        "OK",
        &Res {
            ok: true,
            bytes_written: req.content.as_bytes().len(),
        },
    )
    .await
}

async fn api_read_file(stream: &mut TcpStream, state: AppState, body: &[u8]) -> Result<()> {
    #[derive(Deserialize)]
    struct Req {
        path: String,
    }
    #[derive(Serialize)]
    struct Res {
        ok: bool,
        content: String,
        lines: usize,
        bytes: usize,
    }

    let req: Req = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await
        }
    };
    let base = state.workspace_root.to_string_lossy().into_owned();
    let (text, is_err) = crate::file_tools::tool_read_file(&req.path, Some(&base));
    if is_err {
        return write_json(stream, 400, "Bad Request", &ApiError { error: text }).await;
    }
    let lines = text.lines().count();
    let bytes = text.len();
    write_json(
        stream,
        200,
        "OK",
        &Res {
            ok: true,
            content: text,
            lines,
            bytes,
        },
    )
    .await
}

async fn api_patch_file(stream: &mut TcpStream, state: AppState, body: &[u8]) -> Result<()> {
    #[derive(Deserialize)]
    struct Req {
        path: String,
        search: String,
        replace: String,
    }
    #[derive(Serialize)]
    struct Res {
        ok: bool,
        message: String,
    }

    let req: Req = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await
        }
    };
    let base = state.workspace_root.to_string_lossy().into_owned();
    let (msg, is_err) =
        crate::file_tools::tool_patch_file(&req.path, &req.search, &req.replace, Some(&base));
    if is_err {
        return write_json(stream, 400, "Bad Request", &ApiError { error: msg }).await;
    }
    write_json(
        stream,
        200,
        "OK",
        &Res {
            ok: true,
            message: msg,
        },
    )
    .await
}

async fn api_search_files_endpoint(
    stream: &mut TcpStream,
    state: AppState,
    body: &[u8],
) -> Result<()> {
    #[derive(Deserialize)]
    struct Req {
        pattern: String,
        #[serde(default)]
        dir: String,
        #[serde(default)]
        case_insensitive: bool,
    }
    #[derive(Serialize)]
    struct Res {
        ok: bool,
        output: String,
    }

    let req: Req = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await
        }
    };
    let base = state.workspace_root.to_string_lossy().into_owned();
    let (output, is_err) = crate::file_tools::tool_search_files(
        &req.pattern,
        &req.dir,
        req.case_insensitive,
        Some(&base),
    );
    if is_err {
        return write_json(stream, 400, "Bad Request", &ApiError { error: output }).await;
    }
    write_json(stream, 200, "OK", &Res { ok: true, output }).await
}

async fn api_list_dir(stream: &mut TcpStream, state: AppState, body: &[u8]) -> Result<()> {
    #[derive(Deserialize)]
    struct Req {
        #[serde(default)]
        dir: String,
        #[serde(default)]
        max_entries: usize,
        #[serde(default)]
        include_hidden: bool,
    }

    #[derive(Serialize)]
    struct Res {
        ok: bool,
        output: String,
    }

    let req: Req = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await
        }
    };

    let base = state.workspace_root.to_string_lossy().into_owned();
    let (output, is_err) = crate::file_tools::tool_list_dir(
        &req.dir,
        req.max_entries,
        req.include_hidden,
        Some(&base),
    );
    if is_err {
        return write_json(stream, 400, "Bad Request", &ApiError { error: output }).await;
    }
    write_json(stream, 200, "OK", &Res { ok: true, output }).await
}

async fn api_glob_files(stream: &mut TcpStream, state: AppState, body: &[u8]) -> Result<()> {
    #[derive(Deserialize)]
    struct Req {
        pattern: String,
        #[serde(default)]
        dir: String,
    }
    #[derive(Serialize)]
    struct Res {
        ok: bool,
        output: String,
    }

    let req: Req = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await
        }
    };
    let base = state.workspace_root.to_string_lossy().into_owned();
    let (output, is_err) = crate::file_tools::tool_glob_files(&req.pattern, &req.dir, Some(&base));
    if is_err {
        return write_json(stream, 400, "Bad Request", &ApiError { error: output }).await;
    }
    write_json(stream, 200, "OK", &Res { ok: true, output }).await
}

async fn api_apply_diff(stream: &mut TcpStream, state: AppState, body: &[u8]) -> Result<()> {
    #[derive(Deserialize)]
    struct Req {
        path: String,
        diff: String,
    }
    #[derive(Serialize)]
    struct Res {
        ok: bool,
        message: String,
    }

    let req: Req = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await
        }
    };
    let base = state.workspace_root.to_string_lossy().into_owned();
    let (msg, is_err) = crate::file_tools::tool_apply_diff(&req.path, &req.diff, Some(&base));
    if is_err {
        return write_json(stream, 400, "Bad Request", &ApiError { error: msg }).await;
    }
    write_json(
        stream,
        200,
        "OK",
        &Res {
            ok: true,
            message: msg,
        },
    )
    .await
}

async fn api_stat_path(stream: &mut TcpStream, state: AppState, body: &[u8]) -> Result<()> {
    #[derive(Deserialize)]
    struct Req {
        path: String,
    }
    #[derive(Serialize)]
    struct Res {
        ok: bool,
        exists: bool,
        is_file: bool,
        is_dir: bool,
        bytes: u64,
    }

    let req: Req = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await
        }
    };

    let rel_path = req.path.trim();
    if rel_path.is_empty() {
        return write_json(
            stream,
            400,
            "Bad Request",
            &ApiError {
                error: "path is required".into(),
            },
        )
        .await;
    }
    let rel = Path::new(rel_path);
    // Local path safety (keep consistent with pending store).
    if rel.is_absolute()
        || rel.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return write_json(
            stream,
            400,
            "Bad Request",
            &ApiError {
                error: format!("unsafe path: {rel_path}"),
            },
        )
        .await;
    }

    let abs = state.workspace_root.join(rel);
    match std::fs::metadata(&abs) {
        Ok(md) => {
            write_json(
                stream,
                200,
                "OK",
                &Res {
                    ok: true,
                    exists: true,
                    is_file: md.is_file(),
                    is_dir: md.is_dir(),
                    bytes: md.len(),
                },
            )
            .await
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            write_json(
                stream,
                200,
                "OK",
                &Res {
                    ok: true,
                    exists: false,
                    is_file: false,
                    is_dir: false,
                    bytes: 0,
                },
            )
            .await
        }
        Err(e) => {
            write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: format!("stat failed: {e}"),
                },
            )
            .await
        }
    }
}

async fn api_rollback(stream: &mut TcpStream, state: AppState, body: &[u8]) -> Result<()> {
    #[derive(Deserialize)]
    struct Req {
        #[serde(default)]
        checkpoint: String,
        // Back-compat: older UI versions used `{hash: "..."}`
        #[serde(default)]
        hash: String,
    }
    #[derive(Serialize)]
    struct Res {
        ok: bool,
        message: String,
    }

    let req: Req = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await
        }
    };
    let checkpoint_in = if req.checkpoint.trim().is_empty() {
        req.hash
    } else {
        req.checkpoint
    };
    let checkpoint = checkpoint_in.trim();
    // Safety: only allow hex strings (git hashes).
    if checkpoint.is_empty() || !checkpoint.chars().all(|c| c.is_ascii_hexdigit()) {
        return write_json(
            stream,
            400,
            "Bad Request",
            &ApiError {
                error: "invalid checkpoint hash".into(),
            },
        )
        .await;
    }
    let root = state.workspace_root.to_string_lossy().into_owned();
    let out = std::process::Command::new("git")
        .args(["-C", &root, "reset", "--hard", checkpoint])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let short = &checkpoint[..checkpoint.len().min(8)];
            write_json(
                stream,
                200,
                "OK",
                &Res {
                    ok: true,
                    message: format!("rolled back to {short}"),
                },
            )
            .await
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
            write_json(stream, 400, "Bad Request", &ApiError { error: stderr }).await
        }
        Err(e) => {
            write_json(
                stream,
                500,
                "Internal Server Error",
                &ApiError {
                    error: e.to_string(),
                },
            )
            .await
        }
    }
}

async fn api_open(stream: &mut TcpStream, body: &[u8]) -> Result<()> {
    #[derive(Deserialize)]
    struct Req {
        path: String,
        cwd: Option<String>,
    }
    #[derive(Serialize)]
    struct Res {
        ok: bool,
    }

    let req: Req = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(_) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: "invalid JSON".into(),
                },
            )
            .await
        }
    };

    let target = req.path.trim().to_string();
    if target.is_empty() {
        return write_json(
            stream,
            400,
            "Bad Request",
            &ApiError {
                error: "path is required".into(),
            },
        )
        .await;
    }

    // Resolve relative paths against cwd.
    let resolved = if let Some(cwd) = req.cwd.as_deref().filter(|s| !s.trim().is_empty()) {
        let t = Path::new(&target);
        if t.is_absolute() {
            target.clone()
        } else {
            Path::new(cwd).join(t).to_string_lossy().into_owned()
        }
    } else {
        target.clone()
    };

    #[cfg(target_os = "windows")]
    let spawn_result = Command::new("cmd")
        .args(["/c", "start", "", &resolved])
        .spawn();

    #[cfg(target_os = "macos")]
    let spawn_result = Command::new("open").arg(&resolved).spawn();

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let spawn_result = Command::new("xdg-open").arg(&resolved).spawn();

    match spawn_result {
        Ok(_) => write_json(stream, 200, "OK", &Res { ok: true }).await,
        Err(err) => {
            write_json(
                stream,
                500,
                "Internal Server Error",
                &ApiError {
                    error: err.to_string(),
                },
            )
            .await
        }
    }
}

async fn api_status(stream: &mut TcpStream, state: AppState) -> Result<()> {
    fn env_present(key: &str) -> bool {
        std::env::var(key)
            .ok()
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false)
    }

    let workspace_root = state.workspace_root.to_string_lossy().into_owned();
    let host_os = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };

    let resp = ApiStatusResponse {
        ok: true,
        version: env!("CARGO_PKG_VERSION"),
        host_os,
        providers: ApiStatusProviders {
            mistral: ApiProviderStatus {
                api_key_present: env_present("MISTRAL_API_KEY")
                    || env_present("SPIRAL_CODER_API_KEY"),
            },
            anthropic: ApiProviderStatus {
                api_key_present: env_present("ANTHROPIC_API_KEY"),
            },
            openai_compatible: ApiProviderStatus {
                api_key_present: env_present("SPIRAL_CODER_API_KEY")
                    || env_present("OPENAI_API_KEY"),
            },
        },
        features: ApiFeatures {
            exec: true,
            pending_edits: true,
            pending_commands: true,
            chat_tools: true,
            observer_engine: true,
            meta_prompts: true,
            open_file: true,
            harness_promotions: true,
            merge_gate: true,
            project_scan: true,
        },
        workspace_root,
    };

    write_json(stream, 200, "OK", &resp).await
}

async fn api_models(stream: &mut TcpStream, state: AppState, body: &[u8]) -> Result<()> {
    #[derive(Deserialize)]
    struct ApiModelsRequest {
        provider: Option<ProviderKind>,
        api_key: Option<String>,
        base_url: Option<String>,
    }

    #[derive(Serialize)]
    struct ApiModelsResponse {
        models: Vec<String>,
    }

    let req: ApiModelsRequest =
        serde_json::from_slice(body).context("invalid JSON body (expected ApiModelsRequest)")?;

    let mut partial = state.defaults;
    if let Some(provider) = req.provider {
        partial.provider = Some(provider);
    }
    if let Some(api_key) = req.api_key {
        partial.api_key = Some(api_key);
    }
    if let Some(base_url) = req.base_url {
        partial.base_url = Some(base_url);
    }

    let cfg = match partial.resolve() {
        Ok(cfg) => cfg,
        Err(err) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: err.to_string(),
                },
            )
            .await;
        }
    };

    let models_res: Result<Vec<String>> = match cfg.provider {
        ProviderKind::OpenAiCompatible | ProviderKind::Mistral => {
            let url = format!("{}/models", cfg.base_url);
            let mut req = state
                .client
                .get(url)
                .header("Accept", "application/json")
                .timeout(Duration::from_secs(cfg.timeout_seconds));
            if let Some(key) = &cfg.api_key {
                req = req.bearer_auth(key);
            }
            let resp = req.send().await.context("request failed")?;
            let status = resp.status();
            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                return Err(anyhow!("models API error (HTTP {status})\n{body}"));
            }

            let v: serde_json::Value = resp.json().await.context("invalid JSON response")?;
            let mut out: Vec<String> = Vec::new();
            if let Some(arr) = v.get("data").and_then(|x| x.as_array()) {
                for item in arr {
                    if let Some(id) = item.get("id").and_then(|x| x.as_str()) {
                        out.push(id.to_string());
                    }
                }
            } else if let Some(arr) = v.as_array() {
                for item in arr {
                    if let Some(id) = item.as_str() {
                        out.push(id.to_string());
                    }
                }
            }
            Ok(out)
        }
        ProviderKind::Anthropic => {
            let api_key = cfg
                .api_key
                .as_ref()
                .ok_or_else(|| anyhow!("missing API key for anthropic"))?;
            let url = format!("{}/models", cfg.base_url);
            let resp = state
                .client
                .get(url)
                .header("x-api-key", api_key)
                .header(
                    "anthropic-version",
                    crate::providers::anthropic::ANTHROPIC_VERSION,
                )
                .header("Accept", "application/json")
                .timeout(Duration::from_secs(cfg.timeout_seconds))
                .send()
                .await
                .context("request failed")?;
            let status = resp.status();
            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                return Err(anyhow!(
                    "Anthropic models API error (HTTP {status})\n{body}"
                ));
            }

            let v: serde_json::Value = resp.json().await.context("invalid JSON response")?;
            let mut out: Vec<String> = Vec::new();
            if let Some(arr) = v.get("data").and_then(|x| x.as_array()) {
                for item in arr {
                    if let Some(id) = item.get("id").and_then(|x| x.as_str()) {
                        out.push(id.to_string());
                    }
                }
            }
            Ok(out)
        }
        ProviderKind::Hf => Ok(Vec::new()),
    };

    match models_res {
        Ok(mut models) => {
            models.sort();
            models.dedup();
            write_json(stream, 200, "OK", &ApiModelsResponse { models }).await
        }
        Err(err) => {
            write_json(
                stream,
                502,
                "Bad Gateway",
                &ApiError {
                    error: err.to_string(),
                },
            )
            .await
        }
    }
}

async fn api_chat(stream: &mut TcpStream, state: AppState, body: &[u8]) -> Result<()> {
    let req: ApiChatRequest =
        serde_json::from_slice(body).context("invalid JSON body (expected ApiChatRequest)")?;

    let mut partial = state.defaults.clone();

    if req.vibe.unwrap_or(false) {
        partial.vibe = true;
    }

    if let Some(provider) = req.provider.clone() {
        // If provider changes, avoid accidentally reusing model/base_url/api_key from defaults.
        partial.provider = Some(provider);
        if req.model.is_none() {
            partial.model = None;
        }
        if req.chat_model.is_none() {
            partial.chat_model = None;
        }
        if req.code_model.is_none() {
            partial.code_model = None;
        }
        if req.base_url.is_none() {
            partial.base_url = None;
        }
        if req.api_key.is_none() {
            partial.api_key = None;
        }
    }

    if let Some(model) = req.model {
        partial.model = Some(model);
    }
    if let Some(chat_model) = req.chat_model {
        partial.chat_model = Some(chat_model);
    }
    if let Some(code_model) = req.code_model {
        partial.code_model = Some(code_model);
    }
    if let Some(api_key) = req.api_key {
        partial.api_key = Some(api_key);
    }
    if let Some(base_url) = req.base_url {
        partial.base_url = Some(base_url);
    }
    if let Some(mode) = req.mode {
        partial.mode = Some(mode);
    }
    if let Some(persona) = req.persona {
        partial.persona = Some(persona);
    }
    if let Some(temperature) = req.temperature {
        partial.temperature = Some(temperature);
    }
    if let Some(max_tokens) = req.max_tokens {
        partial.max_tokens = Some(max_tokens);
    }
    if let Some(timeout_seconds) = req.timeout_seconds {
        partial.timeout_seconds = Some(timeout_seconds);
    }

    let cfg = match partial.resolve() {
        Ok(cfg) => cfg,
        Err(err) => {
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: err.to_string(),
                },
            )
            .await;
        }
    };

    let history = req
        .history
        .unwrap_or_default()
        .into_iter()
        .filter_map(|m| {
            let role = m.role.trim().to_string();
            if role != "user" && role != "assistant" {
                return None;
            }
            Some(ChatMessage {
                role,
                content: m.content,
            })
        })
        .collect::<Vec<_>>();

    let provider = providers::build_provider(state.client.clone(), &cfg);
    let bot = ChatBot::new(provider);

    let cot_str = req.cot.as_deref().unwrap_or("brief");
    let resp = match bot
        .run(
            &req.input,
            &history,
            &cfg.mode,
            &cfg.persona,
            req.lang.as_deref(),
            cot_str,
            cfg.temperature,
            cfg.max_tokens,
            req.diff.as_deref(),
            None,
        )
        .await
    {
        Ok(resp) => resp,
        Err(err) => {
            let msg = anyhow!(err).to_string();
            return write_json(stream, 502, "Bad Gateway", &ApiError { error: msg }).await;
        }
    };

    write_json(
        stream,
        200,
        "OK",
        &ApiChatResponse {
            content: resp.content,
            model: resp.model,
        },
    )
    .await
}

async fn api_chat_stream(stream: &mut TcpStream, state: AppState, body: &[u8]) -> Result<()> {
    let req: ApiChatRequest =
        serde_json::from_slice(body).context("invalid JSON body (expected ApiChatRequest)")?;

    let lang = req.lang.clone();
    let (cfg, history, diff, cot) = match build_chat_request(state.defaults.clone(), req) {
        Ok(v) => v,
        Err(err) => {
            // Even for stream endpoint, return JSON error with 400 for easier debugging.
            return write_json(
                stream,
                400,
                "Bad Request",
                &ApiError {
                    error: err.to_string(),
                },
            )
            .await;
        }
    };

    write_sse_header(stream, 200, "OK").await?;

    // Observer streaming has two problems in practice:
    // 1) Some models ignore the requested language when streaming, and we cannot "rewrite" mid-stream.
    // 2) We want the server-side loop suppression + language enforcement logic in ChatBot.
    //
    // Solution: for Observer mode, run the non-stream ChatBot (with enforcement) and "fake stream"
    // the final text as deltas. This keeps the UI experience consistent while staying reliable.
    if matches!(cfg.mode, Mode::Observer) {
        let provider = providers::build_provider(state.client.clone(), &cfg);
        let bot = ChatBot::new(provider);
        let cot_str = cot.as_deref().unwrap_or("brief");

        let resp = match bot
            .run(
                &history.user_input,
                &history.messages,
                &cfg.mode,
                &cfg.persona,
                lang.as_deref(),
                cot_str,
                cfg.temperature,
                cfg.max_tokens,
                diff.as_deref(),
                None,
            )
            .await
        {
            Ok(r) => r,
            Err(err) => {
                let msg = anyhow!(err).to_string();
                let data = serde_json::to_string(&ApiError { error: msg })?;
                write_sse_event(stream, "error", &data).await?;
                write_sse_event(stream, "done", "{}").await?;
                return Ok(());
            }
        };

        let mut chunk = String::new();
        let mut n = 0usize;
        const CHUNK_CHARS: usize = 28;
        for ch in resp.content.chars() {
            chunk.push(ch);
            n += 1;
            if n >= CHUNK_CHARS {
                let data = serde_json::to_string(&serde_json::json!({ "delta": chunk }))?;
                write_sse_event(stream, "delta", &data).await?;
                chunk = String::new();
                n = 0;
            }
        }
        if !chunk.is_empty() {
            let data = serde_json::to_string(&serde_json::json!({ "delta": chunk }))?;
            write_sse_event(stream, "delta", &data).await?;
        }

        write_sse_event(stream, "done", "{}").await?;
        return Ok(());
    }

    let persona_def = personas::resolve_persona(&cfg.persona)?;
    let cot_str = cot.as_deref().unwrap_or("brief");
    let cot_instr = crate::modes::cot_instruction(cot_str, &cfg.mode);
    let lang_instr = crate::modes::language_instruction(lang.as_deref(), &cfg.mode);
    let system_text = format!(
        "[Language]\n{}\n\n{}{}\n\n[Persona]\n{}",
        lang_instr,
        crate::modes::mode_prompt(&cfg.mode),
        cot_instr,
        persona_def.prompt
    );
    let user_text =
        crate::modes::compose_user_text(&history.user_input, &cfg.mode, diff.as_deref(), None);

    // Stream the response as SSE deltas.
    let result = match cfg.provider {
        ProviderKind::OpenAiCompatible | ProviderKind::Mistral => {
            stream_openai_compat(
                stream,
                &state.client,
                &cfg,
                &system_text,
                &history.messages,
                &user_text,
            )
            .await
        }
        ProviderKind::Anthropic => {
            stream_anthropic(
                stream,
                &state.client,
                &cfg,
                &system_text,
                &history.messages,
                &user_text,
            )
            .await
        }
        ProviderKind::Hf => {
            stream_hf_subprocess(stream, &cfg, &system_text, &history.messages, &user_text).await
        }
    };

    if let Err(err) = result {
        let msg = anyhow!(err).to_string();
        let data = serde_json::to_string(&ApiError { error: msg })?;
        write_sse_event(stream, "error", &data).await?;
    }

    write_sse_event(stream, "done", "{}").await?;
    Ok(())
}

#[derive(Deserialize, Clone)]
struct ApiChatRequest {
    input: String,
    lang: Option<String>,

    vibe: Option<bool>,
    provider: Option<ProviderKind>,
    model: Option<String>,
    chat_model: Option<String>,
    code_model: Option<String>,
    api_key: Option<String>,
    base_url: Option<String>,
    mode: Option<Mode>,
    persona: Option<String>,
    cot: Option<String>,
    temperature: Option<f64>,
    max_tokens: Option<u32>,
    timeout_seconds: Option<u64>,

    history: Option<Vec<ApiMessage>>,
    diff: Option<String>,
}

#[derive(Deserialize, Clone)]
struct ApiMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ApiChatResponse {
    content: String,
    model: String,
}

#[derive(Serialize)]
struct ApiStatusResponse {
    ok: bool,
    version: &'static str,
    host_os: &'static str,
    providers: ApiStatusProviders,
    features: ApiFeatures,
    workspace_root: String,
}

#[derive(Serialize)]
struct ApiFeatures {
    exec: bool,
    pending_edits: bool,
    pending_commands: bool,
    chat_tools: bool,
    observer_engine: bool,
    meta_prompts: bool,
    open_file: bool,
    harness_promotions: bool,
    merge_gate: bool,
    project_scan: bool,
}

#[derive(Serialize)]
struct ApiStatusProviders {
    mistral: ApiProviderStatus,
    anthropic: ApiProviderStatus,
    #[serde(rename = "openai-compatible")]
    openai_compatible: ApiProviderStatus,
}

#[derive(Serialize)]
struct ApiProviderStatus {
    api_key_present: bool,
}

#[derive(Serialize)]
struct ApiError {
    error: String,
}

#[derive(Clone)]
struct BuiltHistory {
    user_input: String,
    messages: Vec<ChatMessage>,
}

fn build_chat_request(
    defaults: PartialConfig,
    req: ApiChatRequest,
) -> Result<(
    crate::config::RunConfig,
    BuiltHistory,
    Option<String>,
    Option<String>,
)> {
    let mut partial = defaults;

    if req.vibe.unwrap_or(false) {
        partial.vibe = true;
    }

    if let Some(provider) = req.provider.clone() {
        partial.provider = Some(provider);
        if req.model.is_none() {
            partial.model = None;
        }
        if req.chat_model.is_none() {
            partial.chat_model = None;
        }
        if req.code_model.is_none() {
            partial.code_model = None;
        }
        if req.base_url.is_none() {
            partial.base_url = None;
        }
        if req.api_key.is_none() {
            partial.api_key = None;
        }
    }

    if let Some(model) = req.model {
        partial.model = Some(model);
    }
    if let Some(chat_model) = req.chat_model {
        partial.chat_model = Some(chat_model);
    }
    if let Some(code_model) = req.code_model {
        partial.code_model = Some(code_model);
    }
    if let Some(api_key) = req.api_key {
        partial.api_key = Some(api_key);
    }
    if let Some(base_url) = req.base_url {
        partial.base_url = Some(base_url);
    }
    if let Some(mode) = req.mode {
        partial.mode = Some(mode);
    }
    if let Some(persona) = req.persona {
        partial.persona = Some(persona);
    }
    if let Some(temperature) = req.temperature {
        partial.temperature = Some(temperature);
    }
    if let Some(max_tokens) = req.max_tokens {
        partial.max_tokens = Some(max_tokens);
    }
    if let Some(timeout_seconds) = req.timeout_seconds {
        partial.timeout_seconds = Some(timeout_seconds);
    }

    let cfg = partial.resolve()?;

    let history = req
        .history
        .unwrap_or_default()
        .into_iter()
        .filter_map(|m| {
            let role = m.role.trim().to_string();
            if role != "user" && role != "assistant" {
                return None;
            }
            Some(ChatMessage {
                role,
                content: m.content,
            })
        })
        .collect::<Vec<_>>();

    Ok((
        cfg,
        BuiltHistory {
            user_input: req.input,
            messages: history,
        },
        req.diff,
        req.cot,
    ))
}

async fn write_sse_header(stream: &mut TcpStream, code: u16, reason: &str) -> Result<()> {
    let header = format!(
        "HTTP/1.1 {code} {reason}\r\n\
         Content-Type: text/event-stream; charset=utf-8\r\n\
         Cache-Control: no-cache\r\n\
         X-Content-Type-Options: nosniff\r\n\
         X-Frame-Options: DENY\r\n\
         Connection: close\r\n\r\n"
    );
    stream.write_all(header.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

async fn write_sse_event(stream: &mut TcpStream, event: &str, data: &str) -> Result<()> {
    // Single-line JSON payload is recommended for easy client parsing.
    let msg = format!("event: {event}\ndata: {data}\n\n");
    stream.write_all(msg.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

#[allow(dead_code)]
async fn stream_text_in_chunks(
    stream: &mut TcpStream,
    text: &str,
    chunk_chars: usize,
    delay: Duration,
) -> Result<()> {
    #[derive(Serialize)]
    struct Delta<'a> {
        delta: &'a str,
    }

    let mut start = 0;
    let chars: Vec<char> = text.chars().collect();
    while start < chars.len() {
        let end = (start + chunk_chars).min(chars.len());
        let chunk: String = chars[start..end].iter().collect();
        let data = serde_json::to_string(&Delta { delta: &chunk })?;
        write_sse_event(stream, "delta", &data).await?;
        start = end;
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
    }
    Ok(())
}

async fn stream_openai_compat(
    stream: &mut TcpStream,
    client: &reqwest::Client,
    cfg: &crate::config::RunConfig,
    system_text: &str,
    history: &[ChatMessage],
    user_text: &str,
) -> Result<()> {
    use serde_json::json;

    #[derive(Serialize)]
    struct Delta<'a> {
        delta: &'a str,
    }

    let mut messages: Vec<serde_json::Value> = Vec::with_capacity(1 + history.len() + 1);
    messages.push(json!({"role":"system","content":system_text}));
    for m in history {
        messages.push(json!({"role": m.role, "content": m.content}));
    }
    messages.push(json!({"role":"user","content":user_text}));

    let base_url = cfg.base_url.trim_end_matches('/').to_string();

    let payload = json!({
        "model": cfg.model,
        "messages": messages.clone(),
        "temperature": cfg.temperature,
        "max_tokens": cfg.max_tokens,
        "stream": true,
    });
    let mut payload = payload;
    apply_openai_temperature_compat(&mut payload, &base_url, &cfg.model);

    let provider_label = match cfg.provider {
        ProviderKind::Mistral => "Mistral",
        _ => "OpenAI-compatible",
    };
    let mut last_err: Option<anyhow::Error> = None;
    let mut want_completions = false;
    let mut resp: Option<reqwest::Response> = None;

    for url in openai_compat_chat_urls(&base_url) {
        let mut req = client
            .post(&url)
            .header("Content-Type", "application/json")
            .timeout(Duration::from_secs(cfg.timeout_seconds))
            .json(&payload);
        if let Some(key) = &cfg.api_key {
            req = req.bearer_auth(key);
        }

        let r = req.send().await.context("request failed")?;
        let status = r.status();
        if status.is_success() {
            resp = Some(r);
            break;
        }

        let body = r.text().await.unwrap_or_default();

        if should_swap_to_max_completion_tokens(status, &body)
            && payload.get("max_tokens").is_some()
        {
            let mut payload2 = payload.clone();
            swap_max_tokens_to_max_completion_tokens(&mut payload2);

            let mut req2 = client
                .post(&url)
                .header("Content-Type", "application/json")
                .timeout(Duration::from_secs(cfg.timeout_seconds))
                .json(&payload2);
            if let Some(key) = &cfg.api_key {
                req2 = req2.bearer_auth(key);
            }
            let r2 = req2.send().await.context("request failed")?;
            let status2 = r2.status();
            if status2.is_success() {
                resp = Some(r2);
                break;
            }
            let body2 = r2.text().await.unwrap_or_default();
            last_err = Some(anyhow!(
                "{provider_label} API error (HTTP {status2})\n{body2}"
            ));
            continue;
        }

        if should_use_v1_completions(status, &body) {
            want_completions = true;
            last_err = Some(anyhow!(
                "{provider_label} API error (HTTP {status})\n{body}"
            ));
            break;
        }

        if status == reqwest::StatusCode::NOT_FOUND {
            last_err = Some(anyhow!(
                "{provider_label} API error (HTTP {status})\n{body}"
            ));
            continue;
        }

        return Err(anyhow!(
            "{provider_label} API error (HTTP {status})\n{body}"
        ));
    }

    let mut resp = if let Some(r) = resp {
        r
    } else if want_completions {
        let url = format!("{}/completions", base_url);
        let mut comp_payload = json!({
            "model": cfg.model,
            "prompt": prompt_from_chat_messages(&messages),
            "temperature": cfg.temperature,
            "max_tokens": cfg.max_tokens,
            "stream": true,
        });
        apply_openai_temperature_compat(&mut comp_payload, &base_url, &cfg.model);
        let mut req = client
            .post(&url)
            .header("Content-Type", "application/json")
            .timeout(Duration::from_secs(cfg.timeout_seconds))
            .json(&comp_payload);
        if let Some(key) = &cfg.api_key {
            req = req.bearer_auth(key);
        }
        let r = req.send().await.context("request failed")?;
        let status = r.status();
        if status.is_success() {
            r
        } else {
            let body = r.text().await.unwrap_or_default();
            if should_swap_to_max_completion_tokens(status, &body)
                && comp_payload.get("max_tokens").is_some()
            {
                swap_max_tokens_to_max_completion_tokens(&mut comp_payload);
                let mut req2 = client
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .timeout(Duration::from_secs(cfg.timeout_seconds))
                    .json(&comp_payload);
                if let Some(key) = &cfg.api_key {
                    req2 = req2.bearer_auth(key);
                }
                let r2 = req2.send().await.context("request failed")?;
                let status2 = r2.status();
                if status2.is_success() {
                    r2
                } else {
                    let body2 = r2.text().await.unwrap_or_default();
                    return Err(anyhow!(
                        "{provider_label} API error (HTTP {status2})\n{body2}"
                    ));
                }
            } else {
                return Err(anyhow!(
                    "{provider_label} API error (HTTP {status})\n{body}"
                ));
            }
        }
    } else {
        return Err(last_err.unwrap_or_else(|| anyhow!("{provider_label} request failed")));
    };

    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        buf.extend_from_slice(&chunk);

        while let Some(frame) = take_next_sse_frame(&mut buf) {
            // Parse "data:" lines.
            let frame_str = String::from_utf8_lossy(&frame);
            let mut data_lines: Vec<&str> = Vec::new();
            for line in frame_str.split('\n') {
                let line = line.trim_end_matches('\r');
                if line.is_empty() {
                    continue;
                }
                if line.starts_with(':') {
                    continue;
                }
                if let Some(rest) = line.strip_prefix("data:") {
                    data_lines.push(rest.trim_start());
                }
            }
            if data_lines.is_empty() {
                continue;
            }
            let data = data_lines.join("\n");
            if data.trim() == "[DONE]" {
                return Ok(());
            }

            let v: serde_json::Value = match serde_json::from_str(&data) {
                Ok(v) => v,
                Err(_) => continue,
            };

            if let Some(err) = v.get("error") {
                return Err(anyhow!("provider error: {err}"));
            }

            let delta = v
                .pointer("/choices/0/delta/content")
                .and_then(|x| x.as_str())
                .or_else(|| v.pointer("/choices/0/delta/text").and_then(|x| x.as_str()))
                .or_else(|| v.pointer("/choices/0/text").and_then(|x| x.as_str()))
                .unwrap_or("");

            if !delta.is_empty() {
                let data = serde_json::to_string(&Delta { delta })?;
                write_sse_event(stream, "delta", &data).await?;
            }
        }
    }

    Ok(())
}

async fn stream_anthropic(
    stream: &mut TcpStream,
    client: &reqwest::Client,
    cfg: &crate::config::RunConfig,
    system_text: &str,
    history: &[ChatMessage],
    user_text: &str,
) -> Result<()> {
    use serde_json::json;

    #[derive(Serialize)]
    struct Delta<'a> {
        delta: &'a str,
    }

    let api_key = cfg
        .api_key
        .as_ref()
        .ok_or_else(|| anyhow!("missing API key for anthropic"))?;

    let url = format!("{}/messages", cfg.base_url);

    let mut messages: Vec<serde_json::Value> = Vec::with_capacity(history.len() + 1);
    for m in history {
        if m.role != "user" && m.role != "assistant" {
            continue;
        }
        messages.push(json!({"role": m.role, "content": m.content}));
    }
    messages.push(json!({"role":"user","content":user_text}));

    let payload = json!({
        "model": cfg.model,
        "messages": messages,
        "system": system_text,
        "temperature": cfg.temperature,
        "max_tokens": cfg.max_tokens,
        "stream": true,
    });

    let resp = client
        .post(url)
        .header("x-api-key", api_key)
        .header(
            "anthropic-version",
            crate::providers::anthropic::ANTHROPIC_VERSION,
        )
        .header("Accept", "text/event-stream")
        .header("Content-Type", "application/json")
        .timeout(Duration::from_secs(cfg.timeout_seconds))
        .json(&payload)
        .send()
        .await
        .context("request failed")?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("Anthropic API error (HTTP {status})\n{body}"));
    }

    let mut resp = resp;
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        buf.extend_from_slice(&chunk);

        while let Some(frame) = take_next_sse_frame(&mut buf) {
            let frame_str = String::from_utf8_lossy(&frame);

            let mut event: Option<&str> = None;
            let mut data_lines: Vec<&str> = Vec::new();
            for line in frame_str.split('\n') {
                let line = line.trim_end_matches('\r');
                if line.is_empty() {
                    continue;
                }
                if line.starts_with(':') {
                    continue;
                }
                if let Some(rest) = line.strip_prefix("event:") {
                    event = Some(rest.trim());
                }
                if let Some(rest) = line.strip_prefix("data:") {
                    data_lines.push(rest.trim_start());
                }
            }

            if data_lines.is_empty() {
                continue;
            }
            let data = data_lines.join("\n");
            if data.trim().is_empty() {
                continue;
            }
            if data.trim() == "[DONE]" {
                return Ok(());
            }

            let v: serde_json::Value = match serde_json::from_str(&data) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Prefer the JSON "type" field; fall back to the SSE "event" field.
            let ty = v
                .get("type")
                .and_then(|x| x.as_str())
                .or(event)
                .unwrap_or("");

            if ty == "error" {
                return Err(anyhow!("Anthropic stream error: {v}"));
            }

            if ty == "content_block_delta" {
                let delta = v
                    .pointer("/delta/text")
                    .and_then(|x| x.as_str())
                    .unwrap_or("");
                if !delta.is_empty() {
                    let data = serde_json::to_string(&Delta { delta })?;
                    write_sse_event(stream, "delta", &data).await?;
                }
            }

            if ty == "message_stop" {
                return Ok(());
            }
        }
    }

    Ok(())
}

async fn stream_hf_subprocess(
    stream: &mut TcpStream,
    cfg: &crate::config::RunConfig,
    system_text: &str,
    history: &[ChatMessage],
    user_text: &str,
) -> Result<()> {
    use serde_json::json;

    let python = std::env::var("SPIRAL_CODER_HF_PYTHON").unwrap_or_else(|_| "python".to_string());
    let script_path = std::path::PathBuf::from("scripts").join("hf_infer.py");

    let mut messages: Vec<serde_json::Value> = Vec::with_capacity(1 + history.len() + 1);
    messages.push(json!({"role":"system","content":system_text}));
    for m in history {
        messages.push(json!({"role": m.role, "content": m.content}));
    }
    messages.push(json!({"role":"user","content":user_text}));

    let payload = json!({
        "model": cfg.model,
        "messages": messages,
        "max_new_tokens": cfg.max_tokens,
        "temperature": cfg.temperature,
        "device": cfg.hf_device,
        "local_only": cfg.hf_local_only,
        "stream": true,
    });
    let input = serde_json::to_vec(&payload).context("failed to serialize hf request")?;

    let mut child = Command::new(&python)
        .arg(&script_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn hf subprocess: {python} {script_path:?}"))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("failed to open stdin for hf subprocess"))?;
    stdin
        .write_all(&input)
        .await
        .context("failed to write request to hf subprocess")?;
    stdin
        .write_all(b"\n")
        .await
        .context("failed to write newline to hf subprocess")?;
    drop(stdin);

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("failed to open stdout for hf subprocess"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("failed to open stderr for hf subprocess"))?;

    let stderr_task = tokio::spawn(async move {
        let mut buf: Vec<u8> = Vec::new();
        let _ = stderr.read_to_end(&mut buf).await;
        buf
    });

    let timeout = Duration::from_secs(cfg.timeout_seconds);
    let mut buf = [0u8; 8192];

    let status = match tokio::time::timeout(timeout, async {
        loop {
            let n = stdout.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            stream.write_all(&buf[..n]).await?;
            stream.flush().await?;
        }
        child.wait().await
    })
    .await
    {
        Ok(res) => res.context("hf subprocess wait failed")?,
        Err(_) => {
            // Best-effort: terminate runaway subprocess on timeout.
            let _ = child.kill().await;
            return Err(anyhow!(
                "hf subprocess timed out after {}s",
                cfg.timeout_seconds
            ));
        }
    };

    let stderr_bytes = stderr_task.await.unwrap_or_default();
    if !status.success() {
        let stderr_text = String::from_utf8_lossy(&stderr_bytes);
        let msg = stderr_text.trim();
        if msg.is_empty() {
            return Err(anyhow!("hf subprocess failed (exit code {status})"));
        }
        return Err(anyhow!("hf subprocess failed: {msg}"));
    }

    Ok(())
}

fn take_next_sse_frame(buf: &mut Vec<u8>) -> Option<Vec<u8>> {
    // SSE frames are separated by a blank line. Accept both LF and CRLF forms.
    let pos_lf = find_subslice(buf, b"\n\n");
    let pos_crlf = find_subslice(buf, b"\r\n\r\n");
    let pos = match (pos_lf, pos_crlf) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }?;

    let sep_len = if buf[pos..].starts_with(b"\r\n\r\n") {
        4
    } else {
        2
    };

    let frame = buf[..pos].to_vec();
    buf.drain(..pos + sep_len);
    Some(frame)
}

/// Extract `@path` tokens from a message string (unique, ordered).
fn parse_at_refs_str(text: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for word in text.split_whitespace() {
        if !word.starts_with('@') {
            continue;
        }
        let path = word.trim_start_matches('@');
        let path = path.trim_end_matches(|c: char| matches!(c, ',' | ')' | ']' | ';' | ':' | '.'));
        if path.is_empty() {
            continue;
        }
        if seen.insert(path.to_string()) {
            refs.push(path.to_string());
        }
    }
    refs
}

fn extract_query_param<'a>(path: &'a str, key: &str) -> Option<&'a str> {
    let q_start = path.find('?')?;
    let query = &path[q_start + 1..];
    for pair in query.split('&') {
        if let Some(eq) = pair.find('=') {
            let k = &pair[..eq];
            let v = &pair[eq + 1..];
            if k == key {
                return Some(v);
            }
        }
    }
    None
}

fn url_decode(s: &str) -> String {
    let s = s.replace('+', " ");
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            ) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

async fn api_project_scan(stream: &mut TcpStream, path: &str) -> Result<()> {
    let raw_root = match extract_query_param(path, "root") {
        Some(r) => r,
        None => {
            return write_text(
                stream,
                400,
                "Bad Request",
                "text/plain; charset=utf-8",
                "missing ?root= parameter\n",
            )
            .await;
        }
    };

    let root = url_decode(raw_root);

    // Path traversal guard.
    if root.contains("..") {
        return write_text(
            stream,
            400,
            "Bad Request",
            "text/plain; charset=utf-8",
            "invalid path: '..' not allowed\n",
        )
        .await;
    }

    let result = match crate::project::ProjectContext::scan(&root).await {
        Some(ctx) => ctx.to_scan_result(),
        None => crate::project::ProjectScanResult::default(),
    };

    write_json(stream, 200, "OK", &result).await
}

async fn write_json<T: Serialize>(
    stream: &mut TcpStream,
    code: u16,
    reason: &str,
    value: &T,
) -> Result<()> {
    let body = serde_json::to_vec(value).context("failed to serialize JSON")?;
    write_response(
        stream,
        code,
        reason,
        "application/json; charset=utf-8",
        &body,
    )
    .await
}

async fn write_text(
    stream: &mut TcpStream,
    code: u16,
    reason: &str,
    content_type: &str,
    text: &str,
) -> Result<()> {
    write_response(stream, code, reason, content_type, text.as_bytes()).await
}

async fn write_response(
    stream: &mut TcpStream,
    code: u16,
    reason: &str,
    content_type: &str,
    body: &[u8],
) -> Result<()> {
    let header = format!(
        "HTTP/1.1 {code} {reason}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Cache-Control: no-store\r\n\
         Pragma: no-cache\r\n\
         X-Content-Type-Options: nosniff\r\n\
         X-Frame-Options: DENY\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes()).await?;
    stream.write_all(body).await?;
    Ok(())
}

async fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest> {
    const MAX_HEADER_BYTES: usize = 64 * 1024;
    const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;

    let mut buf: Vec<u8> = Vec::with_capacity(4096);

    let header_end = loop {
        if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
            break pos;
        }
        if buf.len() >= MAX_HEADER_BYTES {
            return Err(anyhow!("request headers too large"));
        }
        let mut tmp = [0u8; 2048];
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            return Err(anyhow!("unexpected EOF while reading request"));
        }
        buf.extend_from_slice(&tmp[..n]);
    };

    let (head, rest) = buf.split_at(header_end + 4);
    let head_str = String::from_utf8_lossy(head);
    let mut lines = head_str.split("\r\n").filter(|l| !l.is_empty());

    let start = lines
        .next()
        .ok_or_else(|| anyhow!("missing request line"))?;
    let mut start_parts = start.split_whitespace();
    let method = start_parts
        .next()
        .ok_or_else(|| anyhow!("invalid request line"))?
        .to_string();
    let target = start_parts
        .next()
        .ok_or_else(|| anyhow!("invalid request line"))?
        .to_string();
    let path = target.split('?').next().unwrap_or("/").to_string();

    let mut content_length: usize = 0;
    let mut origin: Option<String> = None;
    for line in lines {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let key = k.trim();
        if key.eq_ignore_ascii_case("content-length") {
            content_length = v.trim().parse::<usize>().unwrap_or(0);
        } else if key.eq_ignore_ascii_case("origin") {
            origin = Some(v.trim().to_string());
        }
    }

    if content_length > MAX_BODY_BYTES {
        return Err(anyhow!("request body too large"));
    }

    let mut body: Vec<u8> = Vec::with_capacity(content_length.min(4096));
    body.extend_from_slice(rest);

    while body.len() < content_length {
        let mut tmp = vec![0u8; (content_length - body.len()).min(8192)];
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            return Err(anyhow!("unexpected EOF while reading body"));
        }
        body.extend_from_slice(&tmp[..n]);
    }

    if body.len() > content_length {
        body.truncate(content_length);
    }

    Ok(HttpRequest {
        method,
        path,
        target,
        origin,
        body,
    })
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}
