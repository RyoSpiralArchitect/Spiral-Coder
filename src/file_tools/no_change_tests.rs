use super::*;

#[test]
fn identical_edits_are_not_mutations_and_do_not_rewrite_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.txt");
    std::fs::write(&path, "before\n").unwrap();
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    for (name, (output, failed)) in [
        (
            "write_file",
            tool_write_file("file.txt", "before\n", dir.path().to_str()),
        ),
        (
            "patch_file",
            tool_patch_file("file.txt", "before", "before", dir.path().to_str()),
        ),
        (
            "apply_diff",
            tool_apply_diff("file.txt", "@@\n-before\n+before\n", dir.path().to_str()),
        ),
    ] {
        assert!(!failed, "{output}");
        assert!(output.starts_with("OK: unchanged '"), "{name}: {output}");
        assert!(!crate::execution_evidence::file_edit_succeeded(
            name, &output
        ));
    }
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        modified
    );
    assert_eq!(std::fs::read_to_string(path).unwrap(), "before\n");
}

#[test]
fn creating_an_empty_file_is_still_a_real_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let (output, failed) = tool_write_file("empty.txt", "", dir.path().to_str());
    assert!(!failed);
    assert!(dir.path().join("empty.txt").exists());
    assert!(crate::execution_evidence::file_edit_succeeded(
        "write_file",
        &output
    ));
}

#[tokio::test]
async fn approved_identical_edits_report_zero_written_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.txt");
    std::fs::write(&path, "before\n").unwrap();
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    for request in [
        serde_json::json!({"action":"write_file","path":"file.txt","content":"before\n"}),
        serde_json::json!({"action":"patch_file","path":"file.txt","search":"before","replace":"before"}),
        serde_json::json!({"action":"apply_diff","path":"file.txt","diff":"@@\n-before\n+before\n"}),
    ] {
        let store = crate::pending_edits::PendingEditStore::new();
        let (id, _) = store.queue_tool_edit(dir.path(), &request).await.unwrap();
        let resolved = store.approve(dir.path(), &id).await.unwrap();
        let result = resolved.result.unwrap();
        assert_eq!(result["bytes_written"], 0);
        assert!(result["message"]
            .as_str()
            .unwrap()
            .starts_with("OK: unchanged '"));
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            modified
        );
    }
}
