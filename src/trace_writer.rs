use anyhow::{Context, Result};
use serde_json::json;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

/// Create parent directories for an output file path.
///
/// Important edge case (Windows + relative paths):
/// - For `trace.jsonl`, `path.parent()` can be empty ("") which should behave like ".".
pub fn safe_mkdir(path: &Path) -> Result<()> {
    let parent0 = path.parent().unwrap_or_else(|| Path::new("."));
    let parent = if parent0.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent0
    };
    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create trace output dir: {}", parent.display()))?;
    Ok(())
}

/// Minimal JSONL trace writer (append-only).
///
/// This is intentionally simple:
/// - each call writes one JSON object per line
/// - best used for tool calls, checkpoints, governor events, and errors
pub struct TraceWriter {
    started_at_ms: u128,
    w: Mutex<BufWriter<std::fs::File>>,
}

impl TraceWriter {
    pub fn new(path: PathBuf) -> Result<Self> {
        safe_mkdir(&path)?;
        let f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("failed to open trace file: {}", path.display()))?;
        Ok(Self {
            started_at_ms: now_ms(),
            w: Mutex::new(BufWriter::new(f)),
        })
    }

    pub fn event(&self, event: &str, payload: serde_json::Value) -> Result<()> {
        let obj = json!({
            "ts_ms": now_ms(),
            "t_ms": (now_ms().saturating_sub(self.started_at_ms)) as u64,
            "event": event,
            "data": payload,
        });
        let mut w = self.w.lock().unwrap();
        writeln!(w, "{}", obj.to_string()).context("failed to write trace line")?;
        w.flush().ok();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_path(prefix: &str, ext: &str) -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        PathBuf::from(format!("{prefix}-{n}.{ext}"))
    }

    #[test]
    fn trace_writer_supports_parentless_paths() {
        // This must work for paths like "trace.jsonl" where Path::parent() is empty ("").
        let path = unique_path("spiral-coder-trace-test", "jsonl");
        let tw = TraceWriter::new(path.clone()).expect("TraceWriter::new");
        tw.event("test", json!({"ok": true})).expect("event");
        drop(tw);
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.contains("\"event\":\"test\""));
        let _ = std::fs::remove_file(&path);
    }
}
