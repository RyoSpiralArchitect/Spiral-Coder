use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn pick_free_port() -> u16 {
    let l = TcpListener::bind(("127.0.0.1", 0)).expect("bind ephemeral port");
    l.local_addr().expect("local addr").port()
}

struct ServerGuard {
    child: Child,
    log: tempfile::NamedTempFile,
    _workspace: tempfile::TempDir,
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let status = self.child.wait();
        if std::thread::panicking() {
            let log = std::fs::read_to_string(self.log.path()).unwrap_or_default();
            eprintln!("spiral-coder serve status: {status:?}\nserver log:\n{log}");
        }
    }
}

fn spawn_server(port: u16) -> ServerGuard {
    let workspace = tempfile::tempdir().expect("isolated server workspace");
    let log = tempfile::NamedTempFile::new().expect("server log");
    let child = Command::new(env!("CARGO_BIN_EXE_spiral-coder"))
        .args(["serve", "--host", "127.0.0.1", "--port", &port.to_string()])
        .current_dir(workspace.path())
        .env_remove("SPIRAL_CODER_ASSETS_DIR")
        .env_remove("OBSTRAL_ASSETS_DIR") // Ignore legacy developer overrides too.
        .stdin(Stdio::null())
        .stdout(Stdio::from(
            log.as_file().try_clone().expect("stdout log handle"),
        ))
        .stderr(Stdio::from(
            log.as_file().try_clone().expect("stderr log handle"),
        ))
        .spawn()
        .expect("spawn spiral-coder serve");
    ServerGuard {
        child,
        log,
        _workspace: workspace,
    }
}

#[tokio::test]
async fn serve_smoke_assets() {
    let port = pick_free_port();

    let mut server = spawn_server(port);
    let base = format!("http://127.0.0.1:{port}");

    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("reqwest client");

    // Wait until server is ready.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = server.child.try_wait().expect("poll server") {
            panic!("server exited before readiness: {status}");
        }
        match client.get(format!("{base}/")).send().await {
            Ok(r) if r.status().is_success() => {
                r.bytes().await.expect("consume readiness response");
                break;
            }
            _ => {
                if Instant::now() > deadline {
                    panic!("server did not become ready in time");
                }
                tokio::time::sleep(Duration::from_millis(120)).await;
            }
        }
    }

    let html = client
        .get(format!("{base}/"))
        .send()
        .await
        .expect("GET /")
        .error_for_status()
        .expect("successful GET / status")
        .text()
        .await
        .expect("read / body");
    assert!(
        html.contains("id=\"app-root\"") || html.contains("app-root"),
        "index.html should contain root node"
    );

    let app_js = client
        .get(format!("{base}/assets/app.js"))
        .send()
        .await
        .expect("GET app.js")
        .error_for_status()
        .expect("successful GET app.js status")
        .text()
        .await
        .expect("read app.js body");
    assert!(
        app_js.contains("sendObserver"),
        "app.js should include sendObserver"
    );

    let state_js = client
        .get(format!("{base}/assets/core/state.js"))
        .send()
        .await
        .expect("GET core/state.js")
        .error_for_status()
        .expect("successful state.js status")
        .text()
        .await
        .expect("read state.js body");
    assert!(
        state_js.contains("rootUserTextForRun"),
        "task and storage helpers should be served"
    );

    let styles = client
        .get(format!("{base}/assets/styles.css"))
        .send()
        .await
        .expect("GET styles.css")
        .error_for_status()
        .expect("successful GET styles.css status")
        .text()
        .await
        .expect("read styles.css body");
    assert!(
        styles.contains(".bubble"),
        "styles.css should include .bubble rules"
    );

    let status_json: serde_json::Value = client
        .get(format!("{base}/api/status"))
        .send()
        .await
        .expect("GET /api/status")
        .error_for_status()
        .expect("successful GET /api/status status")
        .json()
        .await
        .expect("parse /api/status JSON");
    assert!(
        status_json
            .get("host_os")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            != "",
        "/api/status should include host_os"
    );
    assert_eq!(
        status_json
            .pointer("/features/pending_edits")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        true,
        "/api/status features.pending_edits should be true"
    );
    assert_eq!(
        status_json
            .pointer("/features/meta_prompts")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        true,
        "/api/status features.meta_prompts should be true"
    );

    let pending_json: serde_json::Value = client
        .get(format!("{base}/api/pending_edits"))
        .send()
        .await
        .expect("GET /api/pending_edits")
        .error_for_status()
        .expect("successful GET /api/pending_edits status")
        .json()
        .await
        .expect("parse /api/pending_edits JSON");
    assert!(
        pending_json
            .get("pending")
            .map(|v| v.is_array())
            .unwrap_or(false),
        "/api/pending_edits should return {{ pending: [] }}"
    );
}
