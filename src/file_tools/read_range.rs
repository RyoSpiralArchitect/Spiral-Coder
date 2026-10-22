//! Bounded, one-based inclusive line reads. A range is never a full-file snapshot.
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub(crate) struct ReadRequest {
    pub path: String,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
}

impl ReadRequest {
    pub(crate) fn is_range(&self) -> bool {
        self.start_line.is_some() || self.end_line.is_some()
    }

    pub(crate) fn execute(&self, base: Option<&str>) -> (String, bool) {
        if !self.is_range() {
            return super::tool_read_file(&self.path, base);
        }
        match self.read_range(base) {
            Ok(text) => (text, false),
            Err(error) => (format!("ERROR reading '{}': {error}", self.path), true),
        }
    }

    fn read_range(&self, base: Option<&str>) -> anyhow::Result<String> {
        let start = self.start_line.unwrap_or(1);
        let end = self.end_line.unwrap_or_else(|| start.saturating_add(199));
        anyhow::ensure!(
            start > 0 && end >= start,
            "use one-based lines with end_line >= start_line"
        );
        anyhow::ensure!(
            end - start < 200,
            "read at most 200 lines per call; narrow start_line/end_line"
        );
        let path = super::resolve_safe_path(&self.path, base)?;
        let content = std::fs::read_to_string(path)?;
        let lines: Vec<&str> = content.split_inclusive('\n').collect();
        anyhow::ensure!(
            start <= lines.len(),
            "start_line {start} is beyond EOF ({} lines)",
            lines.len()
        );
        let end = end.min(lines.len());
        let selected: String = lines[start - 1..end].concat();
        let truncated = selected.chars().count() > super::MAX_READ_CHARS;
        let shown: String = selected.chars().take(super::MAX_READ_CHARS).collect();
        let note = if truncated {
            "\n[…truncated — requested range exceeds 8000 characters; narrow the range. For a single long line use exec or an exact JSON patch.]"
        } else {
            ""
        };
        Ok(format!(
            "[{}] ({} lines, {} bytes) [range: lines {start}-{end}; partial file]\n{shown}{note}",
            self.path,
            lines.len(),
            content.len()
        ))
    }
}

pub(crate) fn execute_args(args: &Value, base: Option<&str>) -> (String, bool) {
    match serde_json::from_value::<ReadRequest>(args.clone()) {
        Ok(request) => request.execute(base),
        Err(error) => (format!("ERROR: invalid read_file arguments: {error}"), true),
    }
}

/// Both successful partial reads and truncated prefixes are useful observations,
/// but neither permits replacement of the whole existing file.
pub(crate) fn full_snapshot(text: &str) -> bool {
    text.starts_with('[')
        && !text.lines().next().unwrap_or("").contains("[range:")
        && !text.contains("\n[…truncated — ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_late_lines_without_repeating_the_truncated_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let text = (1..=1000)
            .map(|n| format!("line {n}: 日本語\r\n"))
            .collect::<String>();
        std::fs::write(dir.path().join("large.txt"), &text).unwrap();
        let base = dir.path().to_str();
        let (prefix, failed) = super::super::tool_read_file("large.txt", base);
        assert!(!failed);
        assert!(!full_snapshot(&prefix));
        let args = json!({"path":"large.txt","start_line":997,"end_line":1005});
        let (range, failed) = execute_args(&args, base);
        assert!(!failed, "{range}");
        assert!(range.contains("[range: lines 997-1000; partial file]"));
        assert_eq!(
            range.split_once('\n').unwrap().1,
            text.split_inclusive('\n').skip(996).collect::<String>()
        );
        assert!(!full_snapshot(&range));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("large.txt")).unwrap(),
            text
        );
    }

    #[test]
    fn validates_ranges_and_preserves_the_original_full_read_contract() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a"), "one\ntwo\nlast").unwrap();
        let base = dir.path().to_str();
        for args in [
            json!({"path":"a","start_line":0}),
            json!({"path":"a","start_line":2,"end_line":1}),
            json!({"path":"a","start_line":4}),
            json!({"path":"a","start_line":1,"end_line":201}),
            json!({"path":"a","start_line":"2"}),
            json!({"path":"a","start_line":-1}),
            json!({"path":"../a","start_line":1}),
        ] {
            assert!(execute_args(&args, base).1, "{args}");
        }
        let (full, failed) = execute_args(&json!({"path":"a"}), base);
        assert!(!failed);
        assert!(full_snapshot(&full));
        assert_eq!(full, super::super::tool_read_file("a", base).0);
        assert!(execute_args(&json!({"path":"a","start_line":3}), base)
            .0
            .ends_with("last"));
    }
}
