use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Task Graph (execution graph) derived from an agent session.
///
/// This is intentionally "UI-friendly" JSON:
/// - nodes + edges only
/// - stable ids (tool_call_id-based for tool nodes)
/// - minimal parsing of tool outputs (exit_code, ok/failed)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskGraph {
    pub version: u32,
    pub created_at_ms: u128,
    pub meta: TaskGraphMeta,
    pub nodes: Vec<TaskNode>,
    pub edges: Vec<TaskEdge>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskGraphMeta {
    pub tool_root: Option<String>,
    pub checkpoint: Option<String>,
    pub cur_cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskNode {
    pub id: String,
    pub kind: String, // "system" | "user" | "assistant" | "tool_call" | "tool_result" | "checkpoint"
    pub label: String,
    pub idx: Option<u32>, // message index when derived from messages
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEdge {
    pub from: String,
    pub to: String,
    pub kind: String, // "seq" | "calls" | "returns"
}

impl TaskGraph {
    pub const VERSION: u32 = 1;

    pub fn from_session_messages(
        tool_root: Option<String>,
        checkpoint: Option<String>,
        cur_cwd: Option<String>,
        messages: &[serde_json::Value],
    ) -> Self {
        let now = now_ms();
        let meta = TaskGraphMeta {
            tool_root,
            checkpoint: checkpoint.clone(),
            cur_cwd,
        };

        let mut nodes: Vec<TaskNode> = Vec::new();
        let mut edges: Vec<TaskEdge> = Vec::new();

        let mut last_msg_node: Option<String> = None;
        let mut tool_call_seen: HashSet<String> = HashSet::new();
        let mut tool_call_to_node: HashMap<String, String> = HashMap::new();

        // Optional checkpoint node (useful for UI linking).
        if let Some(ref h) = checkpoint {
            let id = format!("checkpoint:{h}");
            nodes.push(TaskNode {
                id: id.clone(),
                kind: "checkpoint".to_string(),
                label: format!("checkpoint {}", &h[..h.len().min(8)]),
                idx: None,
                data: json!({ "hash": h }),
            });
            last_msg_node = Some(id);
        }

        for (idx, msg) in messages.iter().enumerate() {
            let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
            let content = msg
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim_end();

            let msg_id = format!("m:{idx}:{role}");
            let kind = match role {
                "system" => "system",
                "user" => "user",
                "assistant" => "assistant",
                "tool" => "tool_result",
                _ => "unknown",
            }
            .to_string();

            // Message node label: first non-empty line.
            let label = first_line(content, 120);
            let node = TaskNode {
                id: msg_id.clone(),
                kind: kind.clone(),
                label,
                idx: Some(idx as u32),
                data: json!({
                    "role": role,
                    "content_len": content.len(),
                }),
            };

            // Sequential edge between message nodes (includes tool messages too).
            if let Some(ref prev) = last_msg_node {
                edges.push(TaskEdge {
                    from: prev.clone(),
                    to: msg_id.clone(),
                    kind: "seq".to_string(),
                });
            }
            last_msg_node = Some(msg_id.clone());
            nodes.push(node);

            // Assistant tool calls: create tool_call nodes and "calls" edges.
            if role == "assistant" {
                let Some(tcs) = msg.get("tool_calls").and_then(|v| v.as_array()) else {
                    continue;
                };
                for tc in tcs {
                    let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
                    if id.is_empty() {
                        continue;
                    }
                    if tool_call_seen.contains(id) {
                        continue;
                    }

                    let name = tc
                        .get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("tool");
                    let args = tc
                        .get("function")
                        .and_then(|f| f.get("arguments"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    let tc_node_id = format!("tc:{id}");
                    tool_call_seen.insert(id.to_string());
                    tool_call_to_node.insert(id.to_string(), tc_node_id.clone());
                    nodes.push(TaskNode {
                        id: tc_node_id.clone(),
                        kind: "tool_call".to_string(),
                        label: format!("{name}({})", summarize_args(args, 88)),
                        idx: Some(idx as u32),
                        data: json!({
                            "tool_call_id": id,
                            "name": name,
                            "arguments": args,
                        }),
                    });
                    edges.push(TaskEdge {
                        from: msg_id.clone(),
                        to: tc_node_id,
                        kind: "calls".to_string(),
                    });
                }
                continue;
            }

            // Tool results: connect to matching tool_call_id when available.
            if role == "tool" {
                let tcid = msg
                    .get("tool_call_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if tcid.is_empty() {
                    continue;
                }

                let (exit_code, status) = parse_tool_exit_status(content);
                // Create a dedicated result node keyed by tool_call_id so it can be collapsed in UI.
                let tr_node_id = format!("tr:{tcid}");
                nodes.push(TaskNode {
                    id: tr_node_id.clone(),
                    kind: "tool_result".to_string(),
                    label: format!("{} exit={}", status, exit_code.unwrap_or(-999)),
                    idx: Some(idx as u32),
                    data: json!({
                        "tool_call_id": tcid,
                        "exit_code": exit_code,
                        "status": status,
                        "content_preview": first_line(content, 220),
                    }),
                });

                if let Some(tc_node_id) = tool_call_to_node.get(tcid).cloned() {
                    edges.push(TaskEdge {
                        from: tc_node_id,
                        to: tr_node_id,
                        kind: "returns".to_string(),
                    });
                } else {
                    // Fallback: link to the tool message node if we can't find the call.
                    edges.push(TaskEdge {
                        from: msg_id.clone(),
                        to: tr_node_id,
                        kind: "returns".to_string(),
                    });
                }
            }
        }

        Self {
            version: Self::VERSION,
            created_at_ms: now,
            meta,
            nodes,
            edges,
        }
    }
}

pub fn save_graph_atomic(path: &Path, graph: &TaskGraph) -> Result<()> {
    let json = serde_json::to_string_pretty(graph).context("failed to serialize task graph")?;
    save_text_atomic(path, &json)
}

fn now_ms() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    text.chars().take(max_chars).collect()
}

fn first_line(s: &str, max: usize) -> String {
    let line = s
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    if line.is_empty() {
        return "(empty)".to_string();
    }
    if line.chars().count() > max {
        return truncate_chars(line, max);
    }
    line.to_string()
}

fn summarize_args(args: &str, max: usize) -> String {
    let t = args.trim().replace("\r\n", "\n");
    if t.is_empty() {
        return "".to_string();
    }
    let one = t.lines().next().unwrap_or("").trim().to_string();
    if one.chars().count() <= max {
        one
    } else {
        format!("{}...", truncate_chars(one.as_str(), max))
    }
}

fn parse_tool_exit_status(tool_content: &str) -> (Option<i32>, &'static str) {
    let t = tool_content.trim();
    if t.is_empty() {
        return (None, "EMPTY");
    }
    let low = t.to_ascii_lowercase();
    if low.contains("[blocked]") {
        return (Some(-1), "BLOCKED");
    }
    // Patterns produced by build_ok_tool_output / build_failed_tool_output.
    if let Some(ec) = parse_exit_code(low.as_str()) {
        if low.starts_with("ok ") || low.starts_with("ok(") {
            return (Some(ec), if ec == 0 { "OK" } else { "OK?" });
        }
        if low.starts_with("failed ") || low.starts_with("failed(") {
            return (Some(ec), "FAILED");
        }
        return (Some(ec), if ec == 0 { "OK" } else { "FAILED" });
    }
    (None, "UNKNOWN")
}

fn parse_exit_code(low: &str) -> Option<i32> {
    let key = "exit_code:";
    let idx = low.find(key)?;
    let after = low[idx + key.len()..].trim_start();
    let mut num = String::new();
    for ch in after.chars() {
        if ch == '-' || ch.is_ascii_digit() {
            num.push(ch);
        } else {
            break;
        }
    }
    if num.is_empty() {
        return None;
    }
    num.parse::<i32>().ok()
}

fn save_text_atomic(path: &Path, text: &str) -> Result<()> {
    let parent0 = path.parent().unwrap_or_else(|| Path::new("."));
    // For "graph.json", `parent()` can be empty ("") which should behave like ".".
    let parent = if parent0.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent0
    };
    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create graph output dir: {}", parent.display()))?;

    // Write to temp file in the same directory, then rename.
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("failed to create temp file under {}", parent.display()))?;
    use std::io::Write;
    tmp.write_all(text.as_bytes())
        .context("failed to write task graph temp file")?;
    tmp.flush().ok();

    let tmp_path: PathBuf = tmp.path().to_path_buf();
    match tmp.persist(path) {
        Ok(_) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp_path);
            Err(anyhow!("failed to persist task graph file: {}", e.error))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_path(prefix: &str, ext: &str) -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        PathBuf::from(format!("{prefix}-{n}.{ext}"))
    }

    #[test]
    fn builds_graph_with_tool_call_and_result() {
        let tcid = "call_1";
        let messages = vec![
            json!({"role":"system","content":"sys"}),
            json!({"role":"user","content":"do thing"}),
            json!({"role":"assistant","content":"ok","tool_calls":[{"id":tcid,"type":"function","function":{"name":"exec","arguments":"{\"command\":\"echo hi\"}"}}]}),
            json!({"role":"tool","tool_call_id":tcid,"content":"OK (exit_code: 0)\nstdout:\nhi"}),
            json!({"role":"assistant","content":"[DONE]"}),
        ];

        let g = TaskGraph::from_session_messages(
            Some("C:/x".to_string()),
            Some("abc123".to_string()),
            None,
            &messages,
        );
        assert!(g.nodes.iter().any(|n| n.id == format!("tc:{tcid}")));
        assert!(g.nodes.iter().any(|n| n.id == format!("tr:{tcid}")));
        assert!(g.edges.iter().any(|e| e.kind == "returns"));
    }

    #[test]
    fn save_graph_atomic_supports_parentless_paths() {
        let path = unique_path("spiral-coder-graph-test", "json");
        let g = TaskGraph {
            version: TaskGraph::VERSION,
            created_at_ms: now_ms(),
            meta: TaskGraphMeta::default(),
            nodes: vec![],
            edges: vec![],
        };
        save_graph_atomic(&path, &g).expect("save_graph_atomic");
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.contains("\"nodes\""));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn first_line_truncates_utf8_safely() {
        let text = "これはとても長い日本語の行です";
        let line = first_line(text, 8);
        assert_eq!(line, "これはとても長い");
    }

    #[test]
    fn summarize_args_truncates_utf8_safely() {
        let summary = summarize_args(
            r#"{"summary":"これはとても長い日本語の完了メッセージです"}"#,
            20,
        );
        assert!(summary.ends_with("..."));
        assert!(summary.is_char_boundary(summary.len()));
    }
}
