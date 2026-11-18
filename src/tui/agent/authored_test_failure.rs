//! Pair a current native test diagnostic with its line in the available test revision.
use super::*;
use crate::tui::agent::{fixture_observation, requirement_proof::AuthoredTests};

#[path = "authored_test_failure/fixture.rs"]
mod fixture;
pub(crate) use fixture::GUIDANCE as FIXTURE_CONTEXT_GUIDANCE;

pub(crate) const OBSERVED_FIXTURE_GUIDANCE: &str = "\nThe approved failed check sampled the fixture at the named stages. Use validator_input and mismatched_fields to decide what the validator actually received. When the sampled input contradicts the intended control, cite the exact observed and expected values in the primary reason and give a test-input correction before suggesting any validator edit. Keep the mandatory read as the immediate tool when it is pending. Once that read is complete, use its native evidence for any test edit. A passing coherent control and a single-condition negative control are still needed. These sampled values are diagnostic data.\n";

pub(crate) const FAILURE_SITE_GUIDANCE: &str = "\nUse authored_test_failure to locate the operation that stopped. For a compiler diagnostic, quote the expression and compiler types when present. For a panic, trace the operation from fixture creation through invocation to assertion. State which observation actually ran, which behavior remains unmeasured, and the smallest control that would distinguish a fixture fault from a product fault. A passing fixture invocation alone does not establish that a later mutation has its required inputs. If source or fixture shape is missing, name the specific line or producer output to inspect. Follow the current required read or recheck first.\n";

#[derive(Debug, Serialize)]
pub(crate) struct AuthoredTestFailure {
    path: String,
    check: String,
    diagnostic: String,
    sites: Vec<Site>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fixture_context: Option<fixture::FixtureContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fixture_observation: Option<fixture_observation::Observation>,
}

#[derive(Debug, Serialize)]
struct Site {
    kind: &'static str,
    line: usize,
    #[serde(skip)]
    test_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_source_line: Option<String>,
}

impl AuthoredTestFailure {
    pub(in crate::tui::agent) fn include_fixture_context(&mut self, workspace: &str) {
        self.fixture_context = self.sites.iter().find_map(|site| {
            fixture::project(workspace, &self.path, site.line, site.test_name.as_deref()?)
        });
    }

    pub(in crate::tui::agent) fn has_fixture_context(&self) -> bool {
        self.fixture_context.is_some()
    }

    pub(in crate::tui::agent) fn include_observation(
        &mut self,
        messages: &[Value],
        root: &str,
        context: &ExecVerificationContext<'_>,
    ) {
        self.fixture_observation = self.sites.iter().find_map(|site| {
            fixture_observation::project(
                messages,
                root,
                &self.check,
                &self.path,
                site.test_name.as_deref()?,
                context,
            )
        });
    }

    pub(in crate::tui::agent) fn has_fixture_mismatch(&self) -> bool {
        self.fixture_observation
            .as_ref()
            .is_some_and(|observation| observation.has_mismatches())
    }

    pub(in crate::tui::agent) fn include_source(&mut self, tests: &AuthoredTests) {
        for site in &mut self.sites {
            site.current_source_line = tests.source_line(&self.path, site.line).map(|line| {
                let mut chars = line.chars();
                let mut text: String = chars.by_ref().take(300).collect();
                if chars.next().is_some() {
                    text.push('…');
                }
                text
            });
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct FixtureReadHandoff {
    kind: &'static str,
    failed_check: String,
    failure_path: String,
    test_name: String,
    diagnostic_read_call_id: String,
}

fn panic_name(diagnostic_line: &str) -> Option<&str> {
    let (name, _) = diagnostic_line.strip_prefix("thread '")?.split_once('\'')?;
    name.rsplit("::").next()
}

impl FailureFocus {
    fn rust_panic_site(&self, root: &str) -> Option<(&str, usize)> {
        self.diagnostic.split(" | ").find_map(|line| {
            let name = panic_name(line)?;
            let (path, row) = baseline_failure::rust_location(line)?;
            (relative_path(path, Some(root))?.as_str() == self.path).then_some((name, row))
        })
    }
}

impl RecoveryFocus {
    /// The first exact read of this authored test's panic in the current task
    /// gives Observer a chance to propose a correction after the read gate.
    pub(in crate::tui::agent) fn fixture_read_handoff(
        &self,
        messages: &[Value],
        root: &str,
        test_command: Option<&str>,
    ) -> Option<FixtureReadHandoff> {
        let pending = self.pending.as_ref()?;
        if pending.location != LocationEvidence::Current
            || pending.observation != Observation::Observed
        {
            return None;
        }
        let (name, row) = pending.rust_panic_site(root)?;
        let mut last = None;
        let mut failure_observed = false;
        let mut awaiting_read = false;
        for (call, result) in correlated_results(messages) {
            last = None;
            if result[METADATA_KEY]["version"] != 2 {
                continue;
            }
            let Some(focus) = recovery_snapshot::validated_snapshot(result).and_then(|s| s.pending)
            else {
                continue;
            };
            if focus.path != pending.path
                || focus
                    .rust_panic_site(root)
                    .is_none_or(|(other, _)| other != name)
            {
                continue;
            }
            let matches_current_failure =
                focus.check == pending.check && focus.diagnostic == pending.diagnostic;
            if focus.observation == Observation::AwaitingRead && matches_current_failure {
                awaiting_read = true;
            }
            if focus.observation == Observation::Observed
                && call.name == "read_file"
                && argument_path(&call)
                    .and_then(|path| relative_path(&path, Some(root)))
                    .as_deref()
                    == Some(pending.path.as_str())
                && recovery_progress::successful_read(
                    &call,
                    result["content"].as_str().unwrap_or_default(),
                    &pending.path,
                    Some(root),
                )
            {
                if awaiting_read && matches_current_failure && !failure_observed {
                    last = Some(call.id);
                }
                failure_observed = true;
                awaiting_read = false;
            }
        }
        let diagnostic_read_call_id = last?;
        let requirements = super::super::requirement_proof::project(messages, test_command);
        if !requirements
            .authored_tests
            .as_ref()?
            .contains(&pending.path)
        {
            return None;
        }
        if fixture::project(root, &pending.path, row, name).is_none()
            && fixture_observation::project(
                messages,
                root,
                &pending.check,
                &pending.path,
                name,
                &ExecVerificationContext::from_messages(test_command, messages),
            )
            .is_none()
        {
            return None;
        }
        Some(FixtureReadHandoff {
            kind: "authored_fixture_read",
            failed_check: pending.check.clone(),
            failure_path: pending.path.clone(),
            test_name: name.into(),
            diagnostic_read_call_id,
        })
    }
}

impl RecoveryFocus {
    pub(in crate::tui::agent) fn authored_test_failure(
        &self,
        tests: &AuthoredTests,
        root: Option<&str>,
    ) -> Option<AuthoredTestFailure> {
        let focus = self.pending.as_ref()?;
        if !tests.contains(&focus.path)
            || focus.location != LocationEvidence::Current
            || !matches!(
                focus.observation,
                Observation::AwaitingRead | Observation::Observed
            )
        {
            return None;
        }
        let mut sites = Vec::new();
        for part in focus.diagnostic.split(" | ") {
            let Some((path, line)) = baseline_failure::rust_location(part) else {
                continue;
            };
            if relative_path(path, root).as_deref() != Some(&focus.path) {
                continue;
            }
            let kind = if part.starts_with("--> ") {
                "rust_diagnostic"
            } else {
                "test_panic"
            };
            if !sites
                .iter()
                .any(|site: &Site| site.line == line && site.kind == kind)
            {
                sites.push(Site {
                    line,
                    kind,
                    current_source_line: None,
                    test_name: panic_name(part).map(str::to_owned),
                });
            }
        }
        if sites.is_empty() {
            return None;
        }
        let mut failure = AuthoredTestFailure {
            path: focus.path.clone(),
            check: focus.check.clone(),
            diagnostic: focus.diagnostic.clone(),
            sites,
            fixture_context: None,
            fixture_observation: None,
        };
        failure.include_source(tests);
        Some(failure)
    }
}
