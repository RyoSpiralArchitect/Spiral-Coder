//! Retain a bounded excerpt immediately before the first complete source window.
use super::{read_range, SourceRead, ToolCallData};
use serde::Serialize;

pub(super) struct SourcePrefix<'a> {
    path: String,
    call_id: String,
    total_lines: usize,
    total_bytes: usize,
    lines: &'a str,
}

#[derive(Debug, Serialize)]
pub(super) struct PrefixExcerpt {
    pub(super) read_call_id: String,
    pub(super) start_line: usize,
    pub(super) end_line: usize,
    pub(super) text: String,
}

impl<'a> SourcePrefix<'a> {
    pub(super) fn observed(path: String, call: &ToolCallData, output: &'a str) -> Option<Self> {
        let request: read_range::ReadRequest = serde_json::from_str(&call.arguments).ok()?;
        let (header, body) = output.split_once('\n')?;
        let (total_lines, total_bytes, "") = read_range::header_counts(&request.path, header)?
        else {
            return None;
        };
        if request.is_range() {
            return None;
        }
        // Verify the native character boundary and its truncation note. Source
        // text resembling the note and pruned/cached output cannot supply lines.
        let end = body
            .char_indices()
            .nth(crate::file_tools::MAX_READ_CHARS)?
            .0;
        let (shown, note) = body.split_at(end);
        if total_bytes <= shown.len()
            || note != crate::file_tools::full_read_truncation_note(total_lines, shown)
        {
            return None;
        }
        let complete = shown.rfind('\n')? + 1;
        Some(Self {
            path,
            call_id: call.id.clone(),
            total_lines,
            total_bytes,
            lines: &shown[..complete],
        })
    }

    pub(super) fn adjoining(self, source: &SourceRead<'_>) -> Option<PrefixExcerpt> {
        let start = *source.lines.start();
        let (_, body) = source.read.result.split_once('\n')?;
        if source.path != self.path
            || source.total_lines != self.total_lines
            || source.total_bytes != self.total_bytes
            || source.previous_call_id.is_some()
            || start <= 1
        {
            return None;
        }
        let lines: Vec<_> = self.lines.split_inclusive('\n').collect();
        let overlap_end = lines.len().min(*source.lines.end());
        if start > overlap_end {
            return None;
        }
        let overlap = &lines[start - 1..overlap_end];
        if !body
            .split_inclusive('\n')
            .take(overlap.len())
            .eq(overlap.iter().copied())
        {
            return None;
        }
        let mut first = start - 1;
        let mut size = 0;
        for line in lines[..first].iter().rev().take(120) {
            if size + line.len() > 4096 {
                break;
            }
            size += line.len();
            first -= 1;
        }
        (size > 0).then(|| PrefixExcerpt {
            read_call_id: self.call_id,
            start_line: first + 1,
            end_line: start - 1,
            text: lines[first..start - 1].concat(),
        })
    }
}
