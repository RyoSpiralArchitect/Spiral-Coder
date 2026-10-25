use super::*;
use serde_json::json;

#[tokio::test]
async fn http_read_ranges_grant_complete_read_only_when_every_byte_is_rendered() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("source.txt"), "one\ntwo\nthree\n").unwrap();
    for (args, status, expected) in [
        (
            json!({"path":"source.txt","start_line":1,"end_line":3}),
            "200",
            Some((true, "one\ntwo\nthree\n")),
        ),
        (
            json!({"path":"source.txt"}),
            "200",
            Some((true, "one\ntwo\nthree\n")),
        ),
        (
            json!({"path":"source.txt","start_line":2,"end_line":2}),
            "200",
            Some((false, "two\n")),
        ),
        (json!({"path":"source.txt","start_line":0}), "400", None),
        (json!({"path":"source.txt","start_line":"2"}), "400", None),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let state = AppState {
            client: reqwest::Client::new(),
            defaults: PartialConfig::default(),
            pending_edits: crate::pending_edits::PendingEditStore::new(),
            pending_commands: crate::pending_commands::PendingCommandStore::new(),
            workspace_root: root.path().to_path_buf(),
        };
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            api_read_file(&mut socket, state, &serde_json::to_vec(&args).unwrap())
                .await
                .unwrap();
        });
        let mut client = TcpStream::connect(address).await.unwrap();
        let mut wire = String::new();
        tokio::time::timeout(Duration::from_secs(3), client.read_to_string(&mut wire))
            .await
            .unwrap()
            .unwrap();
        task.await.unwrap();
        assert!(wire.starts_with(&format!("HTTP/1.1 {status}")), "{wire}");
        if let Some((complete, text)) = expected {
            let body: serde_json::Value =
                serde_json::from_str(wire.split_once("\r\n\r\n").unwrap().1).unwrap();
            assert_eq!(body["complete_read"], complete);
            assert_eq!(
                body["content"]
                    .as_str()
                    .unwrap()
                    .split_once('\n')
                    .unwrap()
                    .1,
                text
            );
        }
    }
}
