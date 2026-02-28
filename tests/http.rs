//! HTTP transport integration tests (#142).
//!
//! Spawns the skillet server with `--http` and exercises the MCP protocol
//! over HTTP: initialize, tools/list, tools/call, resources/read, health,
//! session management, and error handling.

use std::process::{Child, Command};
use std::time::Duration;

/// Find a free TCP port by binding to :0 and reading the assigned port.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Spawn the skillet HTTP server on the given port, returning the child process.
fn spawn_server(port: u16) -> Child {
    let bin = assert_cmd::cargo::cargo_bin!("skillet");
    let registry = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-registry");

    Command::new(bin)
        .args([
            "serve",
            "--http",
            &format!("127.0.0.1:{port}"),
            "--registry",
            registry.to_str().unwrap(),
            "--log-level",
            "error",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("failed to spawn skillet server")
}

/// Wait until the server health endpoint responds (up to 5 seconds).
async fn wait_for_server(port: u16) {
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}/health");
    for _ in 0..50 {
        if client.get(&url).send().await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("server did not start within 5 seconds on port {port}");
}

/// JSON-RPC request helper.
fn jsonrpc_request(method: &str, params: serde_json::Value, id: u64) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": id
    })
}

/// Initialize an MCP session and return the session ID.
async fn initialize(client: &reqwest::Client, base: &str) -> String {
    let body = jsonrpc_request(
        "initialize",
        serde_json::json!({
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": { "name": "test-client", "version": "0.1.0" }
        }),
        1,
    );

    let resp = client
        .post(base)
        .json(&body)
        .send()
        .await
        .expect("initialize request failed");

    assert_eq!(resp.status(), 200, "initialize should return 200");

    let session_id = resp
        .headers()
        .get("mcp-session-id")
        .expect("should return mcp-session-id header")
        .to_str()
        .unwrap()
        .to_string();

    assert!(!session_id.is_empty(), "session id should not be empty");
    session_id
}

/// Guard that kills the server process on drop.
struct ServerGuard(Child);

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

// ── Health endpoint ──────────────────────────────────────────────

#[tokio::test]
async fn http_health_endpoint() {
    let port = free_port();
    let _guard = ServerGuard(spawn_server(port));
    wait_for_server(port).await;

    let resp = reqwest::get(format!("http://127.0.0.1:{port}/health"))
        .await
        .expect("health request failed");

    assert_eq!(resp.status(), 200, "health endpoint should return 200 OK");
}

// ── Initialize + session management ─────────────────────────────

#[tokio::test]
async fn http_initialize_returns_session() {
    let port = free_port();
    let _guard = ServerGuard(spawn_server(port));
    wait_for_server(port).await;

    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{port}");
    let session_id = initialize(&client, &base).await;
    assert!(session_id.len() > 10, "session id should be substantial");
}

#[tokio::test]
async fn http_request_without_session_fails() {
    let port = free_port();
    let _guard = ServerGuard(spawn_server(port));
    wait_for_server(port).await;

    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{port}");

    // Send a non-initialize request without a session header
    let body = jsonrpc_request("tools/list", serde_json::json!({}), 1);
    let resp = client.post(&base).json(&body).send().await.unwrap();

    let json: serde_json::Value = resp.json().await.unwrap();
    let error = json.get("error").expect("should have error");
    let code = error.get("code").and_then(|c| c.as_i64()).unwrap();
    // -32006 = SessionRequired
    assert_eq!(code, -32006, "should return SessionRequired error");
}

#[tokio::test]
async fn http_request_with_invalid_session_fails() {
    let port = free_port();
    let _guard = ServerGuard(spawn_server(port));
    wait_for_server(port).await;

    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{port}");

    let body = jsonrpc_request("tools/list", serde_json::json!({}), 1);
    let resp = client
        .post(&base)
        .header("mcp-session-id", "nonexistent-session-id-12345")
        .json(&body)
        .send()
        .await
        .unwrap();

    let json: serde_json::Value = resp.json().await.unwrap();
    let error = json.get("error").expect("should have error");
    let code = error.get("code").and_then(|c| c.as_i64()).unwrap();
    // -32005 = SessionNotFound
    assert_eq!(code, -32005, "should return SessionNotFound error");
}

// ── Tool listing ────────────────────────────────────────────────

#[tokio::test]
async fn http_list_tools() {
    let port = free_port();
    let _guard = ServerGuard(spawn_server(port));
    wait_for_server(port).await;

    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{port}");
    let session_id = initialize(&client, &base).await;

    let body = jsonrpc_request("tools/list", serde_json::json!({}), 2);
    let resp = client
        .post(&base)
        .header("mcp-session-id", &session_id)
        .json(&body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    let tools = json["result"]["tools"].as_array().expect("tools array");
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();

    assert!(
        names.contains(&"search_skills"),
        "should have search_skills"
    );
    assert!(
        names.contains(&"install_skill"),
        "should have install_skill"
    );
    assert!(names.contains(&"info_skill"), "should have info_skill");
}

// ── Tool invocation ─────────────────────────────────────────────

#[tokio::test]
async fn http_search_skills() {
    let port = free_port();
    let _guard = ServerGuard(spawn_server(port));
    wait_for_server(port).await;

    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{port}");
    let session_id = initialize(&client, &base).await;

    let body = jsonrpc_request(
        "tools/call",
        serde_json::json!({
            "name": "search_skills",
            "arguments": { "query": "rust" }
        }),
        3,
    );
    let resp = client
        .post(&base)
        .header("mcp-session-id", &session_id)
        .json(&body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    let result = &json["result"];
    let content = result["content"].as_array().expect("content array");
    let text = content
        .iter()
        .filter_map(|c| c["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        text.contains("rust-dev"),
        "search for 'rust' should find rust-dev: {text}"
    );
}

#[tokio::test]
async fn http_info_skill() {
    let port = free_port();
    let _guard = ServerGuard(spawn_server(port));
    wait_for_server(port).await;

    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{port}");
    let session_id = initialize(&client, &base).await;

    let body = jsonrpc_request(
        "tools/call",
        serde_json::json!({
            "name": "info_skill",
            "arguments": { "owner": "joshrotenberg", "name": "rust-dev" }
        }),
        4,
    );
    let resp = client
        .post(&base)
        .header("mcp-session-id", &session_id)
        .json(&body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    let content = json["result"]["content"].as_array().expect("content array");
    let text = content
        .iter()
        .filter_map(|c| c["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        text.contains("joshrotenberg/rust-dev"),
        "should show skill name: {text}"
    );
    assert!(text.contains("2026.02.24"), "should show version: {text}");
}

// ── Resource reads ──────────────────────────────────────────────

#[tokio::test]
async fn http_read_skill_resource() {
    let port = free_port();
    let _guard = ServerGuard(spawn_server(port));
    wait_for_server(port).await;

    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{port}");
    let session_id = initialize(&client, &base).await;

    let body = jsonrpc_request(
        "resources/read",
        serde_json::json!({
            "uri": "skillet://skills/joshrotenberg/rust-dev"
        }),
        5,
    );
    let resp = client
        .post(&base)
        .header("mcp-session-id", &session_id)
        .json(&body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    let contents = json["result"]["contents"]
        .as_array()
        .expect("contents array");
    let text = contents
        .iter()
        .filter_map(|c| c["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        text.contains("Rust"),
        "skill content should mention Rust: {text}"
    );
}

#[tokio::test]
async fn http_read_metadata_resource() {
    let port = free_port();
    let _guard = ServerGuard(spawn_server(port));
    wait_for_server(port).await;

    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{port}");
    let session_id = initialize(&client, &base).await;

    let body = jsonrpc_request(
        "resources/read",
        serde_json::json!({
            "uri": "skillet://metadata/joshrotenberg/rust-dev"
        }),
        6,
    );
    let resp = client
        .post(&base)
        .header("mcp-session-id", &session_id)
        .json(&body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    let contents = json["result"]["contents"]
        .as_array()
        .expect("contents array");
    let text = contents
        .iter()
        .filter_map(|c| c["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        text.contains("rust-dev"),
        "metadata should contain skill name: {text}"
    );
}

// ── Session delete ──────────────────────────────────────────────

#[tokio::test]
async fn http_delete_session() {
    let port = free_port();
    let _guard = ServerGuard(spawn_server(port));
    wait_for_server(port).await;

    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{port}");
    let session_id = initialize(&client, &base).await;

    // Delete the session
    let resp = client
        .delete(&base)
        .header("mcp-session-id", &session_id)
        .send()
        .await
        .unwrap();

    // Should succeed (200 or 204)
    assert!(
        resp.status().is_success(),
        "delete should succeed: {}",
        resp.status()
    );

    // Subsequent request with deleted session should fail
    let body = jsonrpc_request("tools/list", serde_json::json!({}), 7);
    let resp = client
        .post(&base)
        .header("mcp-session-id", &session_id)
        .json(&body)
        .send()
        .await
        .unwrap();

    let json: serde_json::Value = resp.json().await.unwrap();
    let error = json.get("error").expect("should have error after delete");
    let code = error.get("code").and_then(|c| c.as_i64()).unwrap();
    assert_eq!(code, -32005, "should return SessionNotFound after delete");
}

// ── Invalid JSON handling ───────────────────────────────────────

#[tokio::test]
async fn http_invalid_json_returns_parse_error() {
    let port = free_port();
    let _guard = ServerGuard(spawn_server(port));
    wait_for_server(port).await;

    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{port}");

    let resp = client
        .post(&base)
        .header("content-type", "application/json")
        .body("not valid json {{{")
        .send()
        .await
        .unwrap();

    let json: serde_json::Value = resp.json().await.unwrap();
    let error = json.get("error").expect("should have error");
    let code = error.get("code").and_then(|c| c.as_i64()).unwrap();
    // -32700 = Parse error (standard JSON-RPC)
    assert_eq!(code, -32700, "should return parse error code");
}

// ── Multiple sessions ───────────────────────────────────────────

#[tokio::test]
async fn http_multiple_sessions_independent() {
    let port = free_port();
    let _guard = ServerGuard(spawn_server(port));
    wait_for_server(port).await;

    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{port}");

    let session1 = initialize(&client, &base).await;
    let session2 = initialize(&client, &base).await;

    assert_ne!(session1, session2, "sessions should be unique");

    // Both sessions should work independently
    let body = jsonrpc_request("tools/list", serde_json::json!({}), 2);
    for session_id in [&session1, &session2] {
        let resp = client
            .post(&base)
            .header("mcp-session-id", session_id)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
    }
}
