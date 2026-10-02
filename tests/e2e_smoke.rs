use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn pick_free_port() -> u16 {
    let l = TcpListener::bind(("127.0.0.1", 0)).expect("bind ephemeral port");
    l.local_addr().expect("local addr").port()
}

fn spawn_server(exe: &str, port: u16) -> Child {
    Command::new(exe)
        .args(["serve", "--host", "127.0.0.1", "--port", &port.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn spiral-coder serve")
}

#[tokio::test]
async fn serve_smoke_assets() {
    let root = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo");
    let exe_path = std::path::Path::new(&root)
        .join("target")
        .join("debug")
        .join(if cfg!(windows) {
            "spiral-coder.exe"
        } else {
            "spiral-coder"
        });
    assert!(
        exe_path.exists(),
        "expected spiral-coder binary at {}",
        exe_path.display()
    );
    let exe = exe_path.to_string_lossy().to_string();
    let port = pick_free_port();

    let mut child = spawn_server(&exe, port);
    let base = format!("http://127.0.0.1:{port}");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("reqwest client");

    // Wait until server is ready.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match client.get(format!("{base}/")).send().await {
            Ok(r) if r.status().is_success() => break,
            _ => {
                if Instant::now() > deadline {
                    let _ = child.kill();
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
        .text()
        .await
        .expect("read app.js body");
    assert!(
        app_js.contains("sendObserver"),
        "app.js should include sendObserver"
    );

    let styles = client
        .get(format!("{base}/assets/styles.css"))
        .send()
        .await
        .expect("GET styles.css")
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

    let _ = child.kill();
    let _ = child.wait();
}
