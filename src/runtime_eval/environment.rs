//! Copied fixtures must not reuse another workspace's compiled test executable.
use super::{RuntimeEvalCase, RuntimeEvalDefaults};
use anyhow::Result;
use std::ffi::OsStr;
use std::path::{Component, Path};

pub fn validate_build_isolation(
    cases: &[RuntimeEvalCase],
    defaults: &RuntimeEvalDefaults,
    cargo_target_dir: Option<&OsStr>,
) -> Result<()> {
    if !cases
        .iter()
        .any(|case| case.copy_tool_root.unwrap_or(defaults.copy_tool_root))
    {
        return Ok(());
    }
    let Some(target) = cargo_target_dir.filter(|target| !target.is_empty()) else {
        return Ok(());
    };
    let target = Path::new(target);
    if target.components().any(|component| {
        matches!(
            component,
            Component::Prefix(_) | Component::RootDir | Component::ParentDir
        )
    }) {
        anyhow::bail!(
            "copied runtime-eval fixtures require isolated Cargo build output: \
             unset CARGO_TARGET_DIR or use a relative child directory inside each fixture; \
             shared targets can run stale tests from a different copied workspace"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn case(copy: Option<bool>) -> RuntimeEvalCase {
        serde_json::from_value(json!({
            "id": "copied-rust", "prompt": "Fix the test", "copy_tool_root": copy
        }))
        .unwrap()
    }

    #[test]
    fn copied_fixtures_reject_shared_targets_but_allow_local_builds() {
        let defaults = RuntimeEvalDefaults::default();
        let cases = [case(Some(true))];
        for target in [
            "/tmp/shared-target",
            "../shared-target",
            "target/../../shared",
        ] {
            assert!(validate_build_isolation(&cases, &defaults, Some(OsStr::new(target))).is_err());
        }
        for target in [None, Some(""), Some("target"), Some(".tmp/cargo-target")] {
            assert!(validate_build_isolation(&cases, &defaults, target.map(OsStr::new)).is_ok());
        }
    }

    #[test]
    fn copy_defaults_and_case_overrides_control_the_isolation_requirement() {
        let defaults = RuntimeEvalDefaults {
            copy_tool_root: true,
            ..RuntimeEvalDefaults::default()
        };
        let shared = Some(OsStr::new("../shared"));
        assert!(validate_build_isolation(&[case(None)], &defaults, shared).is_err());
        assert!(validate_build_isolation(&[case(Some(false))], &defaults, shared).is_ok());
        assert!(validate_build_isolation(&[], &defaults, shared).is_ok());
    }
}
