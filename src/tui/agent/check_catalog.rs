//! Derived selections for declared commands. Selection is not execution approval
//! or proof: the resolved exec traverses the ordinary governor and approver.
use super::{prose_context, task_harness::benchmark_proof};
use crate::streaming::ToolCallData;
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Check {
    pub id: String,
    pub command: String,
}

#[derive(Default, Debug)]
pub(super) struct Catalog {
    pub checks: Vec<Check>,
    error: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Declaration {
    version: u32,
    checks: Vec<Check>,
}

impl Catalog {
    pub(super) fn from_root(root: &str) -> Self {
        match Self::parse(root) {
            Ok(checks) => Self {
                checks,
                error: None,
            },
            Err(error) => Self {
                checks: vec![],
                error: Some(error.to_string()),
            },
        }
    }

    fn parse(root: &str) -> Result<Vec<Check>> {
        let required = benchmark_proof::required_commands(root);
        let mut checks = required
            .iter()
            .enumerate()
            .map(|(index, command)| Check {
                id: format!("required-{}", index + 1),
                command: command.clone(),
            })
            .collect::<Vec<_>>();
        if let Some(body) = declaration_body(root)? {
            let declaration: Declaration =
                serde_json::from_str(&body).context("invalid task_checks JSON")?;
            ensure!(declaration.version == 1, "unsupported task_checks version");
            for check in declaration.checks {
                ensure!(
                    !check.id.is_empty()
                        && check.id.len() <= 64
                        && check
                            .id
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
                    "invalid check ID"
                );
                ensure!(
                    !check.command.trim().is_empty() && check.command.len() <= 8192,
                    "invalid check command length"
                );
                ensure!(
                    !checks.iter().any(|c| c.id == check.id),
                    "duplicate check ID: {}",
                    check.id
                );
                ensure!(
                    required.contains(&check.command)
                        || super::classify_verify_level(&check.command, None).is_some(),
                    "optional check must be a recognized verification command"
                );
                checks.push(check);
            }
        }
        ensure!(checks.len() <= 64, "at most 64 checks may be selected");
        Ok(checks)
    }

    pub(super) fn bind(&self, tools: &mut Value, base_url: &str) -> Option<String> {
        let enabled = !self.checks.is_empty()
            && !crate::streaming::provider_metadata::is_google_endpoint(base_url);
        for tool in tools.as_array_mut().into_iter().flatten() {
            if tool["function"]["name"] != "exec" {
                continue;
            }
            let parameters = &mut tool["function"]["parameters"];
            if enabled {
                parameters["properties"]["check_id"]["enum"] =
                    json!(self.checks.iter().map(|c| &c.id).collect::<Vec<_>>());
            } else {
                parameters["properties"]
                    .as_object_mut()
                    .unwrap()
                    .remove("check_id");
                parameters["required"] = json!(["command"]);
            }
        }
        if let Some(error) = &self.error {
            return Some(format!("[Check selections unavailable] {error}. Use the original exact commands; all task obligations remain."));
        }
        enabled.then(|| format!("[Declared check selections]\n{}\nSelect an exact command with exec arguments {{\"check_id\":\"<id>\"}} alone. Runtime executes the listed command at tool_root and records its command, directory and result. Keep tool=exec in think; next may name the check ID. Do not add command, cwd, pipelines or output filters: runtime bounds displayed output. Selection does not grant approval or satisfy a check. Required checks still need fresh successful native execution after final edits; optional reproducers do not create new completion obligations.", json!(self.checks)))
    }

    /// Resolve only a single valid selection, before any action/recovery gate.
    /// Signed calls must keep their original arguments and use explicit commands.
    pub(super) fn resolve(
        &self,
        call: &mut ToolCallData,
        root: Option<&str>,
    ) -> Result<Option<Value>> {
        if call.name != "exec" {
            return Ok(None);
        }
        let args: Value = serde_json::from_str(&call.arguments)?;
        let Some(id) = args.get("check_id") else {
            return Ok(None);
        };
        ensure!(
            call.can_rewrite(),
            "signed exec calls must use the full command, without check_id"
        );
        let id = id.as_str().context("check_id must be a string")?;
        ensure!(
            args.as_object().is_some_and(|o| o.len() == 1),
            "use check_id alone; command/cwd overrides are not allowed"
        );
        let check = self
            .checks
            .iter()
            .find(|c| c.id == id)
            .context("unknown check_id in the current task")?;
        let root = root.context("check selection requires a tool_root")?;
        let receipt =
            json!({"call_id":call.id, "check_id":id, "command":check.command, "cwd":root});
        call.arguments = json!({"check_id":id, "command":check.command, "cwd":root}).to_string();
        Ok(Some(receipt))
    }
}

fn declaration_body(root: &str) -> Result<Option<String>> {
    let (mut fence, mut started, mut closed) = (None, false, false);
    let mut body = String::new();
    for line in root.lines().map(str::trim) {
        if (!started || closed) && prose_context::is_fence_line(line) {
            let marker = prose_context::fence_marker(line);
            if fence == Some(marker) {
                fence = None;
            } else if fence.is_none() {
                fence = Some(marker);
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }
        if line == "<task_checks>" {
            ensure!(!started, "only one task_checks block is allowed");
            started = true;
        } else if line == "</task_checks>" {
            ensure!(started && !closed, "unexpected task_checks closing tag");
            closed = true;
        } else if started && !closed {
            body.push_str(line);
            body.push('\n');
        }
    }
    ensure!(!started || closed, "unclosed task_checks block");
    Ok(started.then_some(body))
}

pub(super) fn think_names_selection(think: &str, arguments: &str) -> bool {
    serde_json::from_str::<Value>(arguments)
        .ok()
        .is_some_and(|args| {
            args["command"].as_str().is_some()
                && args["check_id"].as_str().is_some_and(|id| {
                    think
                        .split_whitespace()
                        .any(|word| word.trim_matches(['`', '\'', '"', '.', ',']) == id)
                })
        })
}

/// A selected command must remain byte-identical through generic rescue paths.
pub(super) fn allows_coercion(call: &ToolCallData) -> bool {
    call.can_rewrite()
        && !serde_json::from_str::<Value>(&call.arguments)
            .ok()
            .is_some_and(|args| call.name == "exec" && args.get("check_id").is_some())
}
