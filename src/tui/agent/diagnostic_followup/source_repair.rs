//! Source ranges retained during one baseline diagnosis.
use super::{read_range, relative_path, PrefixExcerpt, Read, SourcePrefix, ToolCallData};
use serde::Serialize;
use std::ops::RangeInclusive;

#[derive(Debug, Serialize)]
pub(super) struct SourceRepair {
    path: String,
    read_call_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) previous_read_call_id: Option<String>,
    test_read: String,
    implementation_read: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    preceding_lines: Option<PrefixExcerpt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_implementation_read: Option<String>,
    #[serde(skip)]
    total_lines: usize,
    #[serde(skip)]
    windows: Vec<SourceWindow<String>>,
}

#[derive(Debug)]
pub(super) struct SourceWindow<T> {
    pub(super) lines: RangeInclusive<usize>,
    pub(super) output: T,
}

pub(super) fn adds_source_lines<T>(
    mut range: RangeInclusive<usize>,
    seen: &[SourceWindow<T>],
) -> bool {
    range.any(|line| !seen.iter().any(|window| window.lines.contains(&line)))
}

pub(super) struct SourceRead<'a> {
    pub(super) path: String,
    pub(super) read: Read<'a>,
    pub(super) lines: RangeInclusive<usize>,
    pub(super) total_lines: usize,
    pub(super) total_bytes: usize,
    pub(super) previous_call_id: Option<String>,
}

impl<'a> SourceRead<'a> {
    pub(super) fn complete(path: String, call: &ToolCallData, output: &'a str) -> Option<Self> {
        let request: read_range::ReadRequest = serde_json::from_str(&call.arguments).ok()?;
        let (total, total_bytes, _) =
            read_range::header_counts(&request.path, output.lines().next()?)?;
        let (start, end) = if request.is_range() {
            let (start, end) = request.bounds().ok()?;
            (start, end.min(total))
        } else {
            (1, total)
        };
        // Require the last line in the native window so a clipped or pruned
        // result cannot supply new source for an exact edit.
        read_range::covers_line(&call.arguments, output, end).then(|| Self {
            path,
            read: Read {
                call_id: call.id.clone(),
                result: output,
            },
            lines: start..=end,
            total_lines: total,
            total_bytes,
            previous_call_id: None,
        })
    }
}

impl SourceRepair {
    pub(super) fn from_read(
        source: SourceRead<'_>,
        test_read: &str,
        windows: Vec<SourceWindow<&str>>,
        prefix: Option<SourcePrefix<'_>>,
    ) -> Self {
        let preceding_lines = prefix.and_then(|prefix| prefix.adjoining(&source));
        let previous_implementation_read = source
            .previous_call_id
            .as_ref()
            .and_then(|_| windows.first().map(|window| window.output.to_owned()));
        Self {
            path: source.path,
            read_call_id: source.read.call_id,
            previous_read_call_id: source.previous_call_id,
            test_read: test_read.to_owned(),
            implementation_read: source.read.result.to_owned(),
            preceding_lines,
            previous_implementation_read,
            total_lines: source.total_lines,
            windows: windows
                .into_iter()
                .map(|window| SourceWindow {
                    lines: window.lines,
                    output: window.output.to_owned(),
                })
                .collect(),
        }
    }

    pub(super) fn accepts_range(&self, read: &read_range::ReadRequest, root: &str) -> bool {
        if self.previous_read_call_id.is_some()
            || !read.is_range()
            || relative_path(&read.path, Some(root)).as_deref() != Some(self.path.as_str())
        {
            return false;
        }
        read.bounds().ok().is_some_and(|(start, end)| {
            start <= self.total_lines
                && (start..=end.min(self.total_lines)).any(|line| !self.covers_line(line))
        })
    }

    fn covers_line(&self, line: usize) -> bool {
        self.windows
            .iter()
            .any(|window| window.lines.contains(&line))
            || self
                .preceding_lines
                .as_ref()
                .is_some_and(|prefix| (prefix.start_line..=prefix.end_line).contains(&line))
    }

    pub(super) fn covered_read(
        &self,
        read: &read_range::ReadRequest,
        root: &str,
    ) -> Option<String> {
        let prefix = self.preceding_lines.as_ref()?;
        if !read.is_range()
            || relative_path(&read.path, Some(root)).as_deref() != Some(self.path.as_str())
        {
            return None;
        }
        let (start, end) = read.bounds().ok()?;
        let end = end.min(self.total_lines);
        let current = &self.windows.first()?.lines;
        if start > end
            || start > prefix.end_line
            || end < prefix.start_line
            || !(start..=end).all(|line| {
                (prefix.start_line..=prefix.end_line).contains(&line) || current.contains(&line)
            })
        {
            return None;
        }
        Some(format!("Requested lines {start}-{end} of {} are already quoted in source_repair.preceding_lines ({}-{}, read {}) and implementation_read (read {}). Use these source bytes to propose a repair, explain which decision is still open, or request a range containing missing lines.", self.path, prefix.start_line, prefix.end_line, prefix.read_call_id, self.read_call_id))
    }

    fn source_texts<'a>(&'a self, path: &'a str) -> impl Iterator<Item = &'a str> {
        self.windows
            .iter()
            .filter(move |_| self.path == path)
            .filter_map(|window| window.output.split_once('\n').map(|(_, body)| body))
            .chain(
                self.preceding_lines
                    .iter()
                    .filter(move |_| self.path == path)
                    .map(|prefix| prefix.text.as_str()),
            )
    }

    pub(super) fn contains_fragment(&self, path: &str, fragment: &str) -> bool {
        self.source_texts(path).any(|body| body.contains(fragment))
    }

    pub(super) fn contains_identifier(&self, path: &str, identifier: &str) -> bool {
        self.source_texts(path).any(|body| {
            body.split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
                .any(|word| word == identifier)
        })
    }
}
