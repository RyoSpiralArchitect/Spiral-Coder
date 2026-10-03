use super::{tool_apply_diff, tool_patch_file};

fn fixture(content: &str) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("sample.txt"), content).unwrap();
    let base = dir.path().to_string_lossy().into_owned();
    (dir, base)
}

fn long_prefix() -> String {
    (1..=30)
        .map(|line| format!("unrelated prefix {line}\n"))
        .collect()
}

#[test]
fn missing_tail_anchor_shows_actual_tail_without_changing_file() {
    let content = format!("{}      ]\n    }}\n  ]\n}}\n", long_prefix());
    let (dir, base) = fixture(&content);
    let search = "      ]\n    ]\n  ]\n}";
    let (output, failed) = tool_patch_file("sample.txt", search, "replacement", Some(&base));
    assert!(failed, "{output}");
    assert!(output.contains("Current file tail"), "{output}");
    assert!(
        output.contains("do not copy them into search text"),
        "{output}"
    );
    assert!(output.contains("32 |     }"), "{output}");
    assert!(!output.contains("unrelated prefix 1\n"), "{output}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("sample.txt")).unwrap(),
        content
    );
}

#[test]
fn missing_diff_tail_shows_actual_hunk_input_without_changing_file() {
    let content = format!("{}      ]\n    }}\n  ]\n}}\n", long_prefix());
    let (dir, base) = fixture(&content);
    let diff = "@@ -31,4 +31,4 @@\n       ]\n-    ]\n+    },\n   ]\n }\n";
    let (output, failed) = tool_apply_diff("sample.txt", diff, Some(&base));
    assert!(failed, "{output}");
    assert!(output.contains("Hunk 1 input"), "{output}");
    assert!(output.contains("32 |     }"), "{output}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("sample.txt")).unwrap(),
        content
    );
}

#[test]
fn approximate_context_is_display_only_and_does_not_authorize_a_patch() {
    let content = format!(
        "{}fn process_final_record(value: usize) {{\n    save(value + 2);\n}}\n{}",
        long_prefix(),
        long_prefix()
    );
    let (dir, base) = fixture(&content);
    let search = "fn process_final_record(value: usize) {\n    save(value + 1);\n}";
    let (output, failed) = tool_patch_file("sample.txt", search, "replacement", Some(&base));
    assert!(failed, "{output}");
    assert!(output.contains("Closest anchor context"), "{output}");
    assert!(output.contains("32 |     save(value + 2);"), "{output}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("sample.txt")).unwrap(),
        content
    );
}

#[test]
fn ambiguous_patch_and_diff_show_distinct_occurrences_without_choosing_one() {
    let content = "first section\nrepeated anchor\nfirst end\nother context\nsecond section\nrepeated anchor\nsecond end\n";
    let (dir, base) = fixture(content);
    for (output, failed) in [
        tool_patch_file("sample.txt", "repeated anchor", "replacement", Some(&base)),
        tool_apply_diff(
            "sample.txt",
            "@@ -2,1 +2,1 @@\n-repeated anchor\n+replacement\n",
            Some(&base),
        ),
    ] {
        assert!(failed, "{output}");
        assert!(output.contains("Match 1 at line 2"), "{output}");
        assert!(output.contains("Match 2 at line 6"), "{output}");
        assert!(
            output.contains("do not copy them into search text"),
            "{output}"
        );
        assert!(output.contains("first section"), "{output}");
        assert!(output.contains("second section"), "{output}");
    }
    assert_eq!(
        std::fs::read_to_string(dir.path().join("sample.txt")).unwrap(),
        content
    );
}

#[test]
fn diagnostics_bound_long_unicode_lines_and_repeated_matches() {
    let content = format!("{}\n{}", "🌀".repeat(8_000), "anchor\n".repeat(100));
    let (dir, base) = fixture(&content);
    let (missing, failed) = tool_patch_file("sample.txt", "unknown", "replacement", Some(&base));
    assert!(failed);
    assert!(
        missing.chars().count() < 2_000,
        "{}",
        missing.chars().count()
    );
    let (ambiguous, failed) = tool_patch_file("sample.txt", "anchor", "replacement", Some(&base));
    assert!(failed);
    assert!(ambiguous.contains("Match 3"), "{ambiguous}");
    assert!(!ambiguous.contains("Match 4"), "{ambiguous}");
    assert!(
        ambiguous.chars().count() < 2_000,
        "{}",
        ambiguous.chars().count()
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("sample.txt")).unwrap(),
        content
    );
}

#[test]
fn partial_diff_keeps_only_accepted_hunks_and_labels_the_rejected_hunk_input() {
    let content = "before\nanchor\nseparator\nanchor\n";
    let (dir, base) = fixture(content);
    let diff = "@@ -1,1 +1,1 @@\n-before\n+after\n@@ -2,1 +2,1 @@\n-anchor\n+guessed replacement\n";
    let (output, failed) = tool_apply_diff("sample.txt", diff, Some(&base));
    assert!(!failed, "{output}");
    assert!(output.contains("OK: applied 1/2"), "{output}");
    assert!(
        output.contains("Hunk 2 input (before this hunk; diagnostic only)"),
        "{output}"
    );
    assert!(output.contains("1 | after"), "{output}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("sample.txt")).unwrap(),
        "after\nanchor\nseparator\nanchor\n"
    );
}
