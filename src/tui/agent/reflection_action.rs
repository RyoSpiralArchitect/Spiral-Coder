//! Exact operation identity for the reflection gate, separate from diagnostic signatures.
use super::{compact_one_line, ToolCallData};
use serde_json::{json, Value};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ActionIdentity {
    Exec {
        command: String,
        cwd: Option<String>,
    },
    Read {
        path: String,
        range: Option<(usize, usize)>,
    },
    Write {
        path: String,
        content: String,
    },
    Patch {
        path: String,
        search: String,
        replace: String,
    },
    Diff {
        path: String,
        diff: String,
    },
}

// Only execution-relevant fields participate. IDs, provider metadata, JSON key
// ordering and ignored fields cannot turn an identical retry into an adjustment.
// Never normalize/truncate payload text: case, digits and whitespace are data.
pub(super) fn tool_call_action_sig(call: &ToolCallData) -> Option<ActionIdentity> {
    let args: Value = serde_json::from_str(&call.arguments).unwrap_or_else(|_| {
        if call.name == "exec" {
            json!({"command":call.arguments})
        } else {
            json!({})
        }
    });
    let text = |key: &str| args[key].as_str().unwrap_or("").to_owned();
    if call.name == "exec" {
        let command = text("command");
        if command.trim().is_empty() {
            return None;
        }
        let cwd = args["cwd"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        return Some(ActionIdentity::Exec { command, cwd });
    }
    // File tools trim the path at their execution boundary.
    let path = args["path"].as_str()?.trim().to_owned();
    if path.is_empty() {
        return None;
    }
    Some(match call.name.as_str() {
        "read_file" => {
            let request: crate::file_tools::read_range::ReadRequest =
                serde_json::from_value(args).ok()?;
            let range = request.is_range().then(|| {
                let start = request.start_line.unwrap_or(1);
                (
                    start,
                    request
                        .end_line
                        .unwrap_or_else(|| start.saturating_add(199)),
                )
            });
            ActionIdentity::Read { path, range }
        }
        "write_file" => ActionIdentity::Write {
            path,
            content: text("content"),
        },
        "patch_file" => ActionIdentity::Patch {
            path,
            search: if args.get("json_pointer").is_some() {
                format!(
                    "{} expected={}",
                    text("json_pointer"),
                    text("json_expected")
                )
            } else {
                text("search")
            },
            replace: if args.get("json_pointer").is_some() {
                text("json_value")
            } else {
                text("replace")
            },
        },
        "apply_diff" => ActionIdentity::Diff {
            path,
            diff: text("diff"),
        },
        _ => return None,
    })
}

impl fmt::Display for ActionIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Feedback may abbreviate labels; equality above always uses full data.
        let (name, target) = match self {
            Self::Exec { command, .. } => ("exec", command),
            Self::Read { path, .. } => ("read_file", path),
            Self::Write { path, .. } => ("write_file", path),
            Self::Patch { path, .. } => ("patch_file", path),
            Self::Diff { path, .. } => ("apply_diff", path),
        };
        write!(f, "{name}:{}", compact_one_line(target, 160))?;
        if let Self::Read {
            range: Some((start, end)),
            ..
        } = self
        {
            write!(f, " [lines {start}-{end}]")?;
        }
        write!(f, " (unchanged execution arguments)")
    }
}
