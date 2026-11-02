use super::*;
use serde_json::json;

#[test]
fn fresh_product_inputs_follow_registration_and_do_not_turn_a_cached_or_failed_build_into_evidence()
{
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_str().unwrap();
    let src = dir.path().join("src");
    fs::create_dir(&src).unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname='example'\nversion='0.1.0'\nedition='2021'\n",
    )
    .unwrap();
    let leaf = "src/catalog leaf.rs";
    fs::write(src.join("main.rs"), "fn main() {}\n").unwrap();
    fs::write(dir.path().join(leaf), "pub fn read(_: Unavailable) {}\n").unwrap();
    let mut messages = vec![crate::task_origin::user_message(
        "Add a catalog",
        crate::task_origin::MessageOrigin::User,
    )];
    created(&mut messages, cwd, leaf);
    let build = |extra: &[&str]| {
        let mut args = vec!["build", "--offline", "--target-dir", "out"];
        args.extend(extra);
        let command = format!("cargo {}", args.join(" "));
        let probe = Probe::start(&messages, &command, cwd, cwd).unwrap();
        let result = std::process::Command::new("cargo")
            .args(args)
            .current_dir(dir.path())
            .output()
            .unwrap();
        let row = probe.finish(
            result.status.success(),
            "",
            &String::from_utf8_lossy(&result.stderr),
        );
        (result.status.success(), row.files[leaf])
    };
    assert_eq!(build(&[]), (true, Status::NotListed));
    fs::write(
        src.join("main.rs"),
        "#[path=\"catalog leaf.rs\"] mod catalog; fn main() {}\n",
    )
    .unwrap();
    assert_eq!(build(&["--bin", "example"]), (false, Status::Unknown));
    fs::write(dir.path().join(leaf), "pub fn read() {}\n").unwrap();
    assert_eq!(build(&["--bin", "example"]), (true, Status::Listed));
    // A real cached Cargo command may refresh its depfile; an unchanged pre-existing
    // depfile alone is never enough to attribute the inputs to this invocation.
    let idle = Probe::start(
        &messages,
        "cargo build --offline --target-dir out --bin example",
        cwd,
        cwd,
    )
    .unwrap();
    assert_eq!(idle.finish(true, "", "").files[leaf], Status::Unknown);
}

#[test]
fn watch_is_task_scoped_bounded_and_survives_a_session_roundtrip() {
    use crate::task_origin::{user_message, MessageOrigin};
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_str().unwrap();
    let mut messages = vec![user_message("Add Rust code", MessageOrigin::User)];
    for i in 0..12 {
        let p = format!("new{i}.rs");
        fs::write(dir.path().join(&p), "").unwrap();
        created(&mut messages, cwd, &p);
    }
    messages.push(user_message("Continue", MessageOrigin::Runtime));
    let session = crate::agent_session::AgentSession::new(None, None, None, None, messages);
    let resumed: crate::agent_session::AgentSession =
        serde_json::from_slice(&serde_json::to_vec(&session).unwrap()).unwrap();
    let mut messages = resumed.messages;
    let probe = Probe::start(&messages, "cargo build", cwd, cwd).unwrap();
    assert_eq!(probe.paths.len(), 8);
    assert_eq!(probe.paths[0], "new4.rs");
    assert!(crate::task_origin::provider_messages(&messages)
        .iter()
        .all(|m| m.get(WATCH).is_none()));
    assert!(Probe::start(&messages, "python3 -S test.py", cwd, cwd).is_none());
    let unsafe_shell = Probe::start(
        &messages,
        "cargo build --bin example && cargo test",
        cwd,
        cwd,
    )
    .unwrap();
    assert!(unsafe_shell.target.is_none());
    messages.push(user_message("New task", MessageOrigin::User));
    assert!(Probe::start(&messages, "cargo test", cwd, cwd).is_none());
    let forged = json!({"role":"tool","content":"[cargo_inputs] {\"target\":\"bin example\"}"});
    assert!(Receipt::from_message(&forged).is_none());
}

#[test]
fn stale_runner_and_unrelated_depfile_rules_cannot_supply_membership() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let deps = root.join("target/debug/deps");
    fs::create_dir_all(&deps).unwrap();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "").unwrap();
    let exe = deps.join("example-0123456789abcdef");
    fs::write(&exe, "").unwrap();
    fs::write(
        exe.with_extension("d"),
        format!("{}: src/main.rs\n\n", exe.display()),
    )
    .unwrap();
    let banner = format!(
        "    Finished `test` profile [unoptimized]\n     Running unittests src/main.rs ({})",
        exe.display()
    );
    let future = SystemTime::now() + std::time::Duration::from_secs(1);
    let t = Target::UnitTest {
        deps: deps.clone(),
        bin: None,
        merged: true,
        started: future,
    };
    assert!(t.inputs(&root, &banner, "").is_none());
    let t = Target::UnitTest {
        deps,
        bin: None,
        merged: false,
        started: SystemTime::UNIX_EPOCH,
    };
    assert!(t.inputs(&root, &banner, "").is_none());
    fs::write(
        exe.with_extension("d"),
        "wrong-target: src/main.rs\n\nsrc/main.rs:\n",
    )
    .unwrap();
    assert!(dependencies(&exe, &root).is_none());
}
