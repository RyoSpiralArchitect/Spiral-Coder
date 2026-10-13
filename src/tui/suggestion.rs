use serde::{Deserialize, Serialize};
use serde_json::Value;

mod action;
pub(super) use action::tool_schemas;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObserverSuggestionKind {
    Search,
    Read,
    Edit,
    Verify,
    Done,
    Clarify,
    AbandonPath,
}

impl ObserverSuggestionKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::Read => "read",
            Self::Edit => "edit",
            Self::Verify => "verify",
            Self::Done => "done",
            Self::Clarify => "clarify",
            Self::AbandonPath => "abandon_path",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ObserverScoreVector {
    #[serde(default)]
    pub correctness: f32,
    #[serde(default)]
    pub security: f32,
    #[serde(default)]
    pub efficiency: f32,
    #[serde(default)]
    pub readability: f32,
}

impl ObserverScoreVector {
    pub fn normalize(&mut self) {
        self.correctness = self.correctness.clamp(0.0, 1.0);
        self.security = self.security.clamp(0.0, 1.0);
        self.efficiency = self.efficiency.clamp(0.0, 1.0);
        self.readability = self.readability.clamp(0.0, 1.0);
    }

    pub fn focus_axes(&self) -> Vec<&'static str> {
        let mut axes = Vec::new();
        if self.correctness <= 0.75 {
            axes.push("correctness");
        }
        if self.security <= 0.75 {
            axes.push("security");
        }
        if self.efficiency <= 0.75 {
            axes.push("efficiency");
        }
        if self.readability <= 0.75 {
            axes.push("readability");
        }
        axes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObserverResponseMode {
    Accept,
    Override,
    Defer,
}

impl ObserverResponseMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Override => "override",
            Self::Defer => "defer",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ObserverResponseContract {
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub focus_axes: Vec<String>,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObserverSuggestion {
    pub kind: ObserverSuggestionKind,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub suggested_tool: Option<String>,
    #[serde(default)]
    pub suggested_args: Value,
    #[serde(default)]
    pub based_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObserverSuggestionEnvelope {
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub primary_blocker: String,
    #[serde(default)]
    pub scores: ObserverScoreVector,
    #[serde(default)]
    pub response_contract: ObserverResponseContract,
    #[serde(default)]
    pub suggestions: Vec<ObserverSuggestion>,
    #[serde(default, deserialize_with = "display_text")]
    pub quickest_check: String,
    #[serde(default)]
    pub why_this_first: String,
    #[serde(default)]
    pub fallback: String,
}

fn display_text<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    // Optional presentation must not discard a valid typed action. Never
    // coerce an object into an executable suggestion or alter tool arguments.
    Ok(Value::deserialize(deserializer)?
        .as_str()
        .unwrap_or_default()
        .to_string())
}

pub fn parse_observer_suggestion_envelope(raw: &str) -> Option<ObserverSuggestionEnvelope> {
    parse_envelope_checked(raw).ok()
}

fn parse_envelope_checked(raw: &str) -> Result<ObserverSuggestionEnvelope, String> {
    let json_str = extract_first_json_object(raw).unwrap_or_else(|| raw.trim().to_string());
    let mut env = serde_json::from_str::<ObserverSuggestionEnvelope>(&json_str)
        .map_err(|error| error.to_string())?;
    normalize_envelope(&mut env);
    if env.summary.is_empty() && env.primary_blocker.is_empty() && env.suggestions.is_empty() {
        return Err("empty envelope: supply summary, primary_blocker, or suggestions".into());
    }
    Ok(env)
}

/// Replay diagnostics use the same parser and primary-action validator as the
/// TUI consumer. Serde supplies accepted kinds/types instead of a second catalog.
pub(crate) fn validation_issue_for_raw(raw: &str) -> Option<String> {
    match parse_envelope_checked(raw) {
        Ok(env) => action_validation_issue(Some(&env)),
        Err(issue) => Some(issue),
    }
}

pub fn format_observer_suggestion_envelope(env: &ObserverSuggestionEnvelope) -> String {
    let mut out = String::new();
    let blocker = if env.summary.is_empty() {
        env.primary_blocker.as_str()
    } else {
        env.summary.as_str()
    };
    out.push_str("--- blocker ---\n");
    if blocker.is_empty() {
        out.push_str("Observer did not provide a structured blocker summary.\n");
    } else {
        out.push_str(blocker);
        out.push('\n');
    }
    if !env.primary_blocker.is_empty() && env.primary_blocker != blocker {
        out.push_str(&format!("primary_blocker: {}\n", env.primary_blocker));
    }

    out.push_str("--- scores ---\n");
    out.push_str(&format!(
        "correctness={:.2} security={:.2} efficiency={:.2} readability={:.2}\n",
        env.scores.correctness, env.scores.security, env.scores.efficiency, env.scores.readability
    ));
    if env.response_contract.required {
        let focus = if env.response_contract.focus_axes.is_empty() {
            "current blocker".to_string()
        } else {
            env.response_contract.focus_axes.join(", ")
        };
        out.push_str(&format!(
            "response_required: yes ({focus}; choose accept|override|defer on the next coder turn)\n"
        ));
    }

    out.push_str("--- next_actions ---\n");
    if env.suggestions.is_empty() {
        out.push_str("1. No structured suggestions were returned.\n");
    } else {
        for (idx, suggestion) in env.suggestions.iter().take(3).enumerate() {
            out.push_str(&format!(
                "{}. [{} {:.2}] {}\n",
                idx + 1,
                suggestion.kind.label(),
                suggestion.confidence,
                render_suggestion_line(suggestion)
            ));
            if !suggestion.reason.is_empty() {
                out.push_str(&format!("   reason: {}\n", suggestion.reason));
            }
            if !suggestion.based_on.is_empty() {
                out.push_str(&format!(
                    "   based_on: {}\n",
                    suggestion.based_on.join(", ")
                ));
            }
        }
    }

    out.push_str("--- quickest_check ---\n");
    if env.quickest_check.is_empty() {
        out.push_str("Use the highest-confidence suggestion above.\n");
    } else {
        out.push_str(&env.quickest_check);
        out.push('\n');
    }

    out.push_str("--- why_this_first ---\n");
    if env.why_this_first.is_empty() {
        out.push_str("It is the smallest next step that stays within the current scope.\n");
    } else {
        out.push_str(&env.why_this_first);
        out.push('\n');
    }

    out.push_str("--- fallback ---\n");
    if env.fallback.is_empty() {
        out.push_str("If the first step fails, try the next structured suggestion.\n");
    } else {
        out.push_str(&env.fallback);
        out.push('\n');
    }

    out
}

fn normalize_envelope(env: &mut ObserverSuggestionEnvelope) {
    env.summary = env.summary.trim().to_string();
    env.primary_blocker = env.primary_blocker.trim().to_string();
    env.scores.normalize();
    env.response_contract.note = env.response_contract.note.trim().to_string();
    env.response_contract.focus_axes = env
        .response_contract
        .focus_axes
        .iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect();
    if env.response_contract.required && env.response_contract.focus_axes.is_empty() {
        env.response_contract.focus_axes = env
            .scores
            .focus_axes()
            .into_iter()
            .map(str::to_string)
            .collect();
    }
    env.quickest_check = env.quickest_check.trim().to_string();
    env.why_this_first = env.why_this_first.trim().to_string();
    env.fallback = env.fallback.trim().to_string();

    for suggestion in &mut env.suggestions {
        suggestion.reason = suggestion.reason.trim().to_string();
        suggestion.confidence = suggestion.confidence.clamp(0.0, 1.0);
        suggestion.suggested_tool = suggestion
            .suggested_tool
            .as_ref()
            .map(|tool| tool.trim().to_string())
            .filter(|tool| !tool.is_empty());
        suggestion.based_on = suggestion
            .based_on
            .iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect();
    }

    env.suggestions.retain(|suggestion| {
        !suggestion.reason.is_empty()
            || suggestion.suggested_tool.is_some()
            || !suggestion.based_on.is_empty()
    });
    if env.suggestions.len() > 3 {
        env.suggestions.truncate(3);
    }
}

/// Structural repair eligibility is independent of confidence. A valid but
/// uncertain answer must not be retried merely to cross the forwarding threshold.
pub(super) fn action_validation_issue(env: Option<&ObserverSuggestionEnvelope>) -> Option<String> {
    let Some(env) = env else {
        return Some("structured JSON envelope could not be parsed".into());
    };
    let Some(primary) = env.suggestions.first() else {
        return Some("suggestions must contain a primary next action".into());
    };
    action::validation_issue(primary)
}

pub fn primary_suggestion(env: &ObserverSuggestionEnvelope) -> Option<&ObserverSuggestion> {
    env.suggestions
        .first()
        .filter(|suggestion| action::usable(suggestion))
}

fn render_suggestion_line(suggestion: &ObserverSuggestion) -> String {
    if let Some(tool) = suggestion.suggested_tool.as_deref() {
        let rendered_args = render_tool_args(&suggestion.suggested_args);
        if rendered_args.is_empty() {
            return tool.to_string();
        }
        return format!("{tool}({rendered_args})");
    }
    if !suggestion.reason.is_empty() {
        return suggestion.reason.clone();
    }
    suggestion.kind.label().to_string()
}

fn render_tool_args(args: &Value) -> String {
    let Some(obj) = args.as_object() else {
        return String::new();
    };
    let mut keys: Vec<&String> = obj.keys().collect();
    keys.sort();
    keys.into_iter()
        .filter_map(|key| {
            let value = obj.get(key)?;
            Some(format!("{key}={}", render_tool_arg_value(value)))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_tool_arg_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .map(render_tool_arg_value)
            .collect::<Vec<_>>()
            .join("|"),
        _ => value.to_string(),
    }
}

fn extract_first_json_object(s: &str) -> Option<String> {
    let start = s.find('{')?;
    let end = s.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(s[start..=end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_observer_suggestion_envelope_extracts_embedded_json() {
        let raw = r#"
observer note
{
  "summary": "Coder is stuck in diagnose without taking the next concrete read step.",
  "primary_blocker": "missing_concrete_next_step",
  "scores": {
    "correctness": 0.82,
    "security": 0.61,
    "efficiency": 0.77,
    "readability": 0.80
  },
  "response_contract": {
    "required": true,
    "focus_axes": ["security"],
    "note": "If you do not follow the next step, explicitly choose override or defer."
  },
  "suggestions": [
    {
      "kind": "read",
      "reason": "The previous search already isolated the right file.",
      "confidence": 0.91,
      "suggested_tool": "read_file",
      "suggested_args": { "path": "src/tui/prefs.rs" },
      "based_on": ["intent_anchor", "recent_tool_results"]
    }
  ],
  "quickest_check": "read_file(path=src/tui/prefs.rs)",
  "why_this_first": "It confirms storage without widening scope.",
  "fallback": "If prefs.rs is not the storage point, inspect events.rs next."
}
"#;
        let parsed = parse_observer_suggestion_envelope(raw).expect("structured suggestion");
        assert_eq!(parsed.primary_blocker, "missing_concrete_next_step");
        assert!(parsed.response_contract.required);
        assert_eq!(parsed.response_contract.focus_axes, vec!["security"]);
        assert_eq!(parsed.suggestions.len(), 1);
        assert_eq!(
            parsed.suggestions[0].suggested_tool.as_deref(),
            Some("read_file")
        );
    }

    #[test]
    fn format_observer_suggestion_envelope_renders_tool_preview() {
        let env = ObserverSuggestionEnvelope {
            summary: "Need one concrete read step.".to_string(),
            primary_blocker: "missing_concrete_next_step".to_string(),
            scores: ObserverScoreVector {
                correctness: 0.82,
                security: 0.61,
                efficiency: 0.77,
                readability: 0.80,
            },
            response_contract: ObserverResponseContract {
                required: true,
                focus_axes: vec!["security".to_string()],
                note: "Explicitly respond on the next coder turn.".to_string(),
            },
            suggestions: vec![ObserverSuggestion {
                kind: ObserverSuggestionKind::Read,
                reason: "Read the prefs implementation directly.".to_string(),
                confidence: 0.88,
                suggested_tool: Some("read_file".to_string()),
                suggested_args: serde_json::json!({"path": "src/tui/prefs.rs"}),
                based_on: vec!["recent_tool_results".to_string()],
            }],
            quickest_check: "read_file(path=src/tui/prefs.rs)".to_string(),
            why_this_first: "It confirms the storage path directly.".to_string(),
            fallback: "Then inspect events.rs.".to_string(),
        };

        let rendered = format_observer_suggestion_envelope(&env);
        assert!(rendered.contains("--- blocker ---"));
        assert!(rendered.contains("--- scores ---"));
        assert!(rendered.contains("response_required: yes"));
        assert!(rendered.contains("[read 0.88] read_file(path=src/tui/prefs.rs)"));
        assert!(rendered.contains("based_on: recent_tool_results"));
    }
}
