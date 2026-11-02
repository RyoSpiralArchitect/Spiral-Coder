use super::{edit_preview, tool_apply_diff};
use crate::pending_edits::PendingEditStore;
use serde_json::json;

#[test]
fn diff_framing_recovers_only_a_trailing_protocol_line() {
    let body = "@@\n start\n-old\n+new\n end\n";
    let native = edit_preview::diff("f", "start\nold\nend\n", body).unwrap();
    for ending in ["*** End Patch", "*** End Patch\n", "*** End Patch\r\n"] {
        let (content, receipt) =
            edit_preview::diff("f", "start\nold\nend\n", &format!("{body}{ending}")).unwrap();
        assert_eq!(content, native.0);
        assert!(
            receipt.contains("ignored trailing `*** End Patch`"),
            "{receipt}"
        );
    }
    // A tool may legitimately edit a file containing patch examples.
    let literal = "@@\n-old\n+new\n *** End Patch\n+*** Begin Patch";
    assert_eq!(
        edit_preview::diff("f", "old\n*** End Patch\n", literal)
            .unwrap()
            .0,
        "new\n*** End Patch\n*** Begin Patch\n"
    );
}

#[test]
fn diff_framing_rejects_ambiguous_or_partial_repairs_without_a_write() {
    let source = "first\nrepeat\nrepeat\nlonger anchor\n";
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("f");
    std::fs::write(&file, source).unwrap();
    let first = "@@\n-first\n+changed\n";
    for suffix in [
        "@@\n-unknown\n+y\n*** End Patch",
        "@@\n-repeat\n+y\n*** End Patch",
        "@@\n-anchor\n+y\n*** End Patch",
        "@@\n \n+y\n*** End Patch",
        "*** End Patch\n@@\n-repeat\n+y",
        "*** End Patch\n*** End Patch",
        "*** End Patch\n\n",
        "*** Update File: other\n@@\n-first\n+y\n*** End Patch",
        "--- a/other\n+++ b/other\n@@\n-first\n+y\n*** End Patch",
    ] {
        let (error, failed) =
            tool_apply_diff("f", &format!("{first}{suffix}"), dir.path().to_str());
        assert!(failed && error.contains("***"), "{suffix:?}: {error}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), source);
    }
    let context = "@@\n-old\n+new\n*** End Patch";
    assert!(edit_preview::diff("f", "old\n*** End Patch\n", context).is_err());
    let wrong_path = "*** Begin Patch\n*** Update File: other\n@@\n-first\n+new\n*** End Patch";
    assert!(edit_preview::diff("f", source, wrong_path).is_err());
    assert!(edit_preview::diff("f", "猫\n猫\n猫\n", "@@\n-猫\n-猫\n+new\n*** End Patch").is_err());
}

#[tokio::test]
async fn diff_framing_approval_previews_the_same_repair_and_preserves_stale_guard() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("f");
    std::fs::write(&file, "old\n").unwrap();
    let request = json!({"action":"apply_diff","path":"f","diff":"@@\n-old\n+new\n*** End Patch"});
    let store = PendingEditStore::new();
    let (stale, _) = store.queue_tool_edit(dir.path(), &request).await.unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "old\n");
    std::fs::write(&file, "somebody else's edit\n").unwrap();
    assert!(store.approve(dir.path(), &stale).await.is_err());
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "somebody else's edit\n"
    );
    std::fs::write(&file, "old\n").unwrap();
    let (id, _) = store.queue_tool_edit(dir.path(), &request).await.unwrap();
    let result = store
        .approve(dir.path(), &id)
        .await
        .unwrap()
        .result
        .unwrap();
    assert!(result["message"]
        .as_str()
        .unwrap()
        .contains("ignored trailing"));
    assert_eq!(std::fs::read_to_string(file).unwrap(), "new\n");
}
