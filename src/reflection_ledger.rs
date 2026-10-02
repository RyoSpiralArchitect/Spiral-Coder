use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

fn now_ms() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn normalize_key(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_ascii_lowercase()
}

fn compact_one_line(s: &str, max: usize) -> String {
    let compact = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = compact.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    let mut out = String::new();
    for ch in trimmed.chars().take(max.saturating_sub(1)) {
        out.push(ch);
    }
    out.push('…');
    out
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReflectionLedgerEntry {
    pub wrong_assumption: String,
    pub next_minimal_action: String,
    #[serde(default)]
    pub trigger: String,
    #[serde(default)]
    pub last_outcome: String,
    #[serde(default)]
    pub goal_delta: String,
    #[serde(default)]
    pub strategy_change: String,
    #[serde(default)]
    pub count: u32,
    #[serde(default)]
    pub first_seen_ms: u128,
    #[serde(default)]
    pub last_seen_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionLedger {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub updated_at_ms: u128,
    #[serde(default)]
    pub entries: Vec<ReflectionLedgerEntry>,
}

const MAX_ENTRIES: usize = 16;

fn default_version() -> u32 {
    1
}

impl Default for ReflectionLedger {
    fn default() -> Self {
        Self {
            version: Self::VERSION,
            updated_at_ms: 0,
            entries: Vec::new(),
        }
    }
}

impl ReflectionLedger {
    pub const VERSION: u32 = 1;

    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read reflection ledger: {}", path.display()))?;
        let mut ledger: Self = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse reflection ledger: {}", path.display()))?;
        if ledger.version != Self::VERSION {
            anyhow::bail!(
                "unsupported reflection ledger version {} (expected {})",
                ledger.version,
                Self::VERSION
            );
        }
        ledger
            .entries
            .sort_by_key(|entry| std::cmp::Reverse(entry.last_seen_ms));
        Ok(ledger)
    }

    pub fn save_atomic(&self, path: &Path) -> Result<()> {
        let json =
            serde_json::to_string_pretty(self).context("failed to serialize reflection ledger")?;
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create reflection ledger dir: {}",
                parent.display()
            )
        })?;
        let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
        std::fs::write(&tmp, json.as_bytes()).with_context(|| {
            format!("failed to write temp reflection ledger: {}", tmp.display())
        })?;
        std::fs::rename(&tmp, path).with_context(|| {
            format!(
                "failed to replace reflection ledger {} -> {}",
                tmp.display(),
                path.display()
            )
        })?;
        Ok(())
    }

    pub fn remember(
        &mut self,
        wrong_assumption: &str,
        next_minimal_action: &str,
        trigger: Option<&str>,
        last_outcome: &str,
        goal_delta: &str,
        strategy_change: &str,
    ) -> bool {
        let wrong_assumption = compact_one_line(wrong_assumption.trim(), 180);
        let next_minimal_action = compact_one_line(next_minimal_action.trim(), 180);
        if wrong_assumption.is_empty() || next_minimal_action.is_empty() {
            return false;
        }
        let now = now_ms();
        let trigger = compact_one_line(trigger.unwrap_or_default().trim(), 140);
        let last_outcome = compact_one_line(last_outcome.trim(), 60);
        let goal_delta = compact_one_line(goal_delta.trim(), 60);
        let strategy_change = compact_one_line(strategy_change.trim(), 60);
        let wrong_sig = normalize_key(wrong_assumption.as_str());
        let next_sig = normalize_key(next_minimal_action.as_str());
        if wrong_sig.is_empty() || next_sig.is_empty() {
            return false;
        }
        if let Some(entry) = self.entries.iter_mut().find(|entry| {
            normalize_key(entry.wrong_assumption.as_str()) == wrong_sig
                && normalize_key(entry.next_minimal_action.as_str()) == next_sig
        }) {
            entry.count = entry.count.saturating_add(1);
            entry.last_seen_ms = now;
            if !trigger.is_empty() {
                entry.trigger = trigger;
            }
            if !last_outcome.is_empty() {
                entry.last_outcome = last_outcome;
            }
            if !goal_delta.is_empty() {
                entry.goal_delta = goal_delta;
            }
            if !strategy_change.is_empty() {
                entry.strategy_change = strategy_change;
            }
        } else {
            self.entries.push(ReflectionLedgerEntry {
                wrong_assumption,
                next_minimal_action,
                trigger,
                last_outcome,
                goal_delta,
                strategy_change,
                count: 1,
                first_seen_ms: now,
                last_seen_ms: now,
            });
        }
        self.updated_at_ms = now;
        self.entries.sort_by_key(|entry| {
            (
                std::cmp::Reverse(entry.count),
                std::cmp::Reverse(entry.last_seen_ms),
            )
        });
        if self.entries.len() > MAX_ENTRIES {
            self.entries.truncate(MAX_ENTRIES);
        }
        true
    }

    pub fn find_entry(
        &self,
        wrong_assumption: &str,
        next_minimal_action: &str,
    ) -> Option<&ReflectionLedgerEntry> {
        let wrong_sig = normalize_key(wrong_assumption);
        let next_sig = normalize_key(next_minimal_action);
        if wrong_sig.is_empty() || next_sig.is_empty() {
            return None;
        }
        self.entries.iter().find(|entry| {
            normalize_key(entry.wrong_assumption.as_str()) == wrong_sig
                && normalize_key(entry.next_minimal_action.as_str()) == next_sig
        })
    }

    pub fn build_prompt(&self) -> Option<String> {
        if self.entries.is_empty() {
            return None;
        }
        let mut out = String::from("[Reflection Ledger]\n");
        out.push_str("Recent recurring reflections for this repo:\n");
        for entry in self.entries.iter().take(4) {
            out.push_str(&format!(
                "- wrong_assumption: {}\n  next_minimal_action: {}\n",
                entry.wrong_assumption, entry.next_minimal_action
            ));
            let mut meta = Vec::new();
            if !entry.trigger.is_empty() {
                meta.push(format!("trigger={}", entry.trigger));
            }
            if !entry.strategy_change.is_empty() {
                meta.push(format!("strategy={}", entry.strategy_change));
            }
            if !entry.goal_delta.is_empty() {
                meta.push(format!("goal_delta={}", entry.goal_delta));
            }
            if !entry.last_outcome.is_empty() {
                meta.push(format!("last_outcome={}", entry.last_outcome));
            }
            meta.push(format!("count={}", entry.count));
            out.push_str(&format!("  meta: {}\n", meta.join(" | ")));
        }
        out.push_str(
            "Use this as a bias, not as proof. If the same wrong assumption reappears, prefer the remembered next minimal action before broadening search. Ignore ledger entries when current tool output contradicts them.",
        );
        Some(out)
    }

    pub fn build_compact_prompt(&self) -> Option<String> {
        let full = self.build_prompt()?;
        let mut lines = vec![
            "[Reflection Ledger cache]".to_string(),
            format!("entries: {}", self.entries.len()),
            format!("updated_at_ms: {}", self.updated_at_ms),
        ];
        for entry in self.entries.iter().take(2) {
            lines.push(format!(
                "- {} => {} (count={})",
                compact_one_line(entry.wrong_assumption.as_str(), 70),
                compact_one_line(entry.next_minimal_action.as_str(), 70),
                entry.count
            ));
        }
        lines.push(format!("hash_seed: {}", compact_one_line(&full, 48)));
        Some(lines.join("\n"))
    }
}

pub fn path_for_root(root: &str) -> PathBuf {
    Path::new(root).join(".spiral-coder/reflection_ledger.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_path() -> PathBuf {
        let n = now_ms();
        std::env::temp_dir().join(format!("spiral-coder-reflection-ledger-{n}.json"))
    }

    #[test]
    fn remember_dedupes_and_counts() {
        let mut ledger = ReflectionLedger::default();
        assert!(ledger.remember(
            "cargo test was necessary first",
            "run targeted tests",
            Some("verify gate"),
            "partial",
            "closer",
            "adjust"
        ));
        assert!(ledger.remember(
            "cargo   test was necessary first",
            "run targeted tests",
            Some("verify gate"),
            "success",
            "closer",
            "adjust"
        ));
        assert_eq!(ledger.entries.len(), 1);
        assert_eq!(ledger.entries[0].count, 2);
        assert_eq!(ledger.entries[0].last_outcome, "success");
    }

    #[test]
    fn roundtrip_preserves_entries() {
        let path = unique_path();
        let mut ledger = ReflectionLedger::default();
        ledger.remember(
            "broad search was unnecessary",
            "read src/tui/prefs.rs",
            Some("missing_concrete_next_step"),
            "partial",
            "closer",
            "adjust",
        );
        ledger.save_atomic(&path).expect("save");
        let loaded = ReflectionLedger::load(&path).expect("load");
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(
            loaded.entries[0].next_minimal_action,
            "read src/tui/prefs.rs"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn build_prompt_mentions_bias_and_action() {
        let mut ledger = ReflectionLedger::default();
        ledger.remember(
            "broad search was unnecessary",
            "read src/tui/prefs.rs",
            Some("missing_concrete_next_step"),
            "partial",
            "closer",
            "adjust",
        );
        let prompt = ledger.build_prompt().expect("prompt");
        assert!(prompt.contains("[Reflection Ledger]"));
        assert!(prompt.contains("broad search was unnecessary"));
        assert!(prompt.contains("read src/tui/prefs.rs"));
    }
}
