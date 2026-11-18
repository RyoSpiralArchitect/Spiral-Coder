//! Pair a current native test diagnostic with its line in the available test revision.
use super::*;
use crate::tui::agent::requirement_proof::AuthoredTests;

pub(crate) const FAILURE_SITE_GUIDANCE: &str = "\nUse authored_test_failure to locate the operation that stopped. For a compiler diagnostic, quote the expression and compiler types when present. For a panic, trace the operation from fixture creation through invocation to assertion. State which observation actually ran, which behavior remains unmeasured, and the smallest control that would distinguish a fixture fault from a product fault. A passing fixture invocation alone does not establish that a later mutation has its required inputs. If source or fixture shape is missing, name the specific line or producer output to inspect. Follow the current required read or recheck first.\n";

#[derive(Debug, Serialize)]
pub(crate) struct AuthoredTestFailure {
    path: String,
    check: String,
    diagnostic: String,
    sites: Vec<Site>,
}

#[derive(Debug, Serialize)]
struct Site {
    kind: &'static str,
    line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_source_line: Option<String>,
}

impl AuthoredTestFailure {
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
        };
        failure.include_source(tests);
        Some(failure)
    }
}
