use super::tool_search_files;

fn search(source: &str, name: &str, query: &str, scope: &str) -> String {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join(name), source).unwrap();
    let (text, error) = tool_search_files(query, scope, false, root.path().to_str());
    assert!(!error, "{text}");
    text
}

#[test]
fn a_scoped_command_search_separates_identifier_locations_from_literal_hits() {
    let source = "enum Command {\n    ListRuns(ListRunsArgs),\n}\nconst NAME: &str = \"list-runs\";\nmatch cmd {\n    Command::ListRuns(args) => run(args),\n}\n";
    let out = search(source, "entry.rs", "list-runs", "entry.rs");
    let mut lines = out.lines();
    assert_eq!(
        lines.next(),
        Some("[search_files: 'list-runs' — 1 match(es)]")
    );
    let navigation = lines.next().unwrap();
    assert!(
        navigation.starts_with("[navigation]")
            && navigation.contains("ListRuns")
            && navigation.contains("2, 6"),
        "{out}"
    );
    assert_eq!(
        lines
            .filter(|line| !line.starts_with('['))
            .collect::<Vec<_>>(),
        ["entry.rs:4: const NAME: &str = \"list-runs\";"]
    );
}

#[test]
fn identifier_only_keeps_zero_literal_matches_and_excludes_longer_names() {
    let source =
        "PreListRuns ListRunsArgs ListRuns_2 éListRuns ListRunsβ\n// ListRuns\nListRuns()\n";
    let out = search(source, "entry.rs", "list-runs", "entry.rs");
    assert!(out.starts_with("[search_files] No matches for 'list-runs'"));
    let note = out
        .lines()
        .find(|s| s.starts_with("[navigation]"))
        .expect("location note");
    assert!(note.contains("2, 3") && !note.contains("1,"), "{note}");
    assert!(!out.lines().any(|line| line.starts_with("entry.rs:")));
}

#[test]
fn ordinary_searches_get_no_identifier_note() {
    for (name, pattern, scope, src) in [
        ("entry.rs", "list-runs", ".", "ListRuns"),
        ("entry.md", "list-runs", "entry.md", "ListRuns"),
        ("entry.rs", "--list-runs", "entry.rs", "ListRuns"),
        ("entry.rs", "list--runs", "entry.rs", "ListRuns"),
        ("entry.rs", "unknown-cmd", "entry.rs", "ListRuns"),
        (
            "entry.rs",
            "list-runs",
            "entry.rs",
            "ListRunsArgs PreListRuns ListRuns_2",
        ),
    ] {
        assert!(
            !search(src, name, pattern, scope).contains("[navigation]"),
            "{name} {pattern}"
        );
    }
}

#[test]
fn an_absent_command_can_point_to_one_existing_declaration_and_match_arm() {
    let source = "enum Task {\n    ExportCsv(Args),\n}\nmatch input {\n    Some(Task::ExportCsv(args)) => save(args),\n}\n";
    let out = search(source, "entry.rs", "export-json", "entry.rs");
    let mut lines = out.lines();
    assert_eq!(
        lines.next(),
        Some("[search_files] No matches for 'export-json' in file 'entry.rs'")
    );
    let note = lines.next().unwrap();
    assert!(
        note.starts_with("[navigation]")
            && note.contains("`ExportCsv`")
            && note.contains("line 2")
            && note.contains("line 5"),
        "{out}"
    );
    assert!(!out.lines().any(|line| line.starts_with("entry.rs:")));
    for extra in [
        "ExportXml(Other),\nTask::ExportXml(_) => xml(),\n",
        "Task::ExportCsv(other) => other(),\n",
        "const REQUEST: &str = \"export-json\";\n",
    ] {
        assert!(
            !search(
                &format!("{source}{extra}"),
                "entry.rs",
                "export-json",
                "entry.rs"
            )
            .contains("[navigation]"),
            "{extra}"
        );
    }
    for (src, query) in [
        ("enum Task {\n    ExportCsv(Args),\n}\n", "export-json"),
        ("Task::ExportCsv(_) => save(),\n", "export-json"),
        (
            "enum Task {\n    Exporter(Args),\n}\nTask::Exporter(_) => save(),\n",
            "export-json",
        ),
        (
            "enum Task {\n    // ExportCsv(Args),\n}\nTask::ExportCsv(_) => save(),\n",
            "export-json",
        ),
        (
            "enum Task {\n    ExportCsv(Args),\n}\n// Task::ExportCsv(_) => save(),\n",
            "export-json",
        ),
        (
            "enum Task {\n    ExportCsv(Args),\n}\nTask::ExportCsvExtra(_) => save(),\n",
            "export-json",
        ),
        (
            "enum Job {\n    ExportCsv(Args),\n}\nTask::ExportCsv(_) => save(),\n",
            "export-json",
        ),
    ] {
        assert!(
            !search(src, "entry.rs", query, "entry.rs").contains("[navigation]"),
            "{src}"
        );
    }
    assert!(!search(source, "entry.rs", "export-json", ".").contains("[navigation]"));
    assert!(!search(source, "entry.md", "export-json", "entry.md").contains("[navigation]"));
}

#[test]
fn navigation_is_bounded_even_when_the_literal_search_finds_more() {
    let entry = "ListRuns(); ListRuns(); // list-runs\n";
    let out = search(&entry.repeat(7), "entry.rs", "list-runs", "entry.rs");
    let note = out.lines().find(|s| s.starts_with("[navigation]")).unwrap();
    assert!(note.contains("lines 1, 2, 3, 4.") && !note.contains("4, 5"));
    assert!(out.starts_with("[search_files: 'list-runs' — 7 match(es)]"));
    let large = format!("{entry}{}", " ".repeat(512 * 1024));
    let out = search(&large, "entry.rs", "list-runs", "entry.rs");
    assert!(out.contains("entry.rs:1:") && !out.contains("[navigation]"));
}

#[test]
fn helper_search_distinguishes_lexical_evidence_from_a_planned_import() {
    let source = "fn use_it() { select_many(); select(); }\nfn select<T>() {}\nselect(); select();\n// préselect() selecté()\n";
    let out = search(source, "host.rs", "select", "host.rs");
    assert!(
        out.contains("fn-name lines [2]; call-like lines [1, 3]"),
        "{out}"
    );
    assert!(out.starts_with("[search_files: 'select' — 4 match(es)]"));
    let absent = search(source, "host.rs", "select_cases", "host.rs");
    assert!(absent.starts_with("[search_files] No matches"));
    assert!(
        absent.contains("No exact identifier spelling `select_cases` was observed in this file")
    );
    assert!(!search(source, "host.rs", "select", ".").contains("[navigation]"));
}

#[test]
fn directory_command_search_locates_one_definition_and_dispatch_across_literal_callers() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("src");
    std::fs::create_dir(&dir).unwrap();
    let source="enum Command {\n    InspectRun(Args),\n}\nfn dispatch(cmd:Command) { match cmd {\n    Command::InspectRun(args) => inspect(args),\n} }\n";
    std::fs::write(dir.join("entry.rs"), source).unwrap();
    std::fs::write(
        dir.join("caller.md"),
        "Use inspect-run for the current run.\n",
    )
    .unwrap();
    let search_dir = || tool_search_files("inspect-run", "src", false, root.path().to_str()).0;
    let with_hits = search_dir();
    let note="[navigation] In `src/entry.rs`, enum `Command` declares `InspectRun` at line 2 and its qualified match arm is at line 5.";
    assert_eq!(
        with_hits.lines().collect::<Vec<_>>(),
        [
            "[search_files: 'inspect-run' — 1 match(es)]",
            note,
            "src/caller.md:1: Use inspect-run for the current run."
        ]
    );
    std::fs::write(dir.join("caller.md"), "operator guide").unwrap();
    let no_hits = search_dir();
    assert!(no_hits.starts_with("[search_files] No matches"));
    assert_eq!(no_hits.lines().nth(1), Some(note));
    std::fs::write(dir.join("other.rs"), source).unwrap();
    assert!(!search_dir().contains("[navigation]"));
    std::fs::write(
        dir.join("other.rs"),
        "// Command::InspectRun(args) => inspect(args)\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("caller.md"),
        "inspect-run\n".repeat(super::MAX_SEARCH_RESULTS + 1),
    )
    .unwrap();
    let truncated = search_dir();
    assert!(truncated.contains("more may exist") && !truncated.contains("[navigation]"));
}
