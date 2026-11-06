use super::*;

use super::synthesize_list_member_insertion;

#[test]
fn a_lookup_cannot_become_a_patch_against_an_unrelated_string_list() {
    assert!(super::unique_requested_source_member(
        "Inspect `src/a.rs`, `src/b.rs` and `src/c.rs`.",
        "src/a.rs"
    )
    .is_none());
    let source="(\n    \"promote-harness\",\n    \"generate governor-contract promotion candidates from harness overlays\",\n),\n];\n";
    assert!(synthesize_list_member_insertion(source, "src/tui_replay/catalog.rs").is_none());
    let lists = "    \"src/a.rs\",\n];\nconst OTHER: &[&str] = &[\n    \"src/b.rs\",\n];\n";
    assert!(synthesize_list_member_insertion(lists, "src/new.rs").is_none());
}
