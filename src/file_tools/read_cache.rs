//! Rebind cache display paths only after resolving both names to the same file.

pub(crate) fn for_request(cached: &str, requested: &str, base: Option<&str>) -> Option<String> {
    let (header, body) = cached.split_once('\n')?;
    // A filename can itself contain `] (`; the last delimiter owns metadata.
    let (path, metadata) = header.rsplit_once("] (")?;
    let path = path.strip_prefix('[')?;
    let (lines, rest) = metadata.split_once(" lines, ")?;
    let (bytes, suffix) = rest.split_once(" bytes)")?;
    let lines: usize = lines.parse().ok()?;
    bytes.parse::<usize>().ok()?;
    if !suffix.is_empty() {
        let range = suffix
            .strip_prefix(" [range: lines ")?
            .strip_suffix("; partial file]")?;
        let (start, end) = range.split_once('-')?;
        let (start, end): (usize, usize) = (start.parse().ok()?, end.parse().ok()?);
        if start == 0 || end < start || end > lines || end - start >= 200 {
            return None;
        }
    }
    if super::resolve_safe_path(path, base).ok()?
        != super::resolve_safe_path(requested, base).ok()?
    {
        return None;
    }
    let rebound = format!("[{requested}] ({metadata}");
    Some(format!(
        "{rebound} [⚡ cached — unchanged since last read]\n{rebound}\n{body}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_header_rebinds_only_the_headers_and_preserves_literal_body() {
        let root = tempfile::tempdir().unwrap();
        let relative = "notes] (draft).md";
        let absolute = root.path().join(relative);
        let body = format!(
            "Literal path: {}\n[other.md] (1 lines, 1 bytes)\n",
            absolute.display()
        );
        std::fs::write(&absolute, &body).unwrap();
        let (native, failed) =
            super::super::tool_read_file(absolute.to_str().unwrap(), root.path().to_str());
        assert!(!failed);
        let rebound = for_request(&native, relative, root.path().to_str()).unwrap();
        let mut parts = rebound.splitn(3, '\n');
        assert!(parts
            .next()
            .unwrap()
            .starts_with(&format!("[{relative}] (")));
        assert!(parts
            .next()
            .unwrap()
            .starts_with(&format!("[{relative}] (")));
        assert_eq!(parts.next(), Some(body.as_str()));
    }

    #[test]
    fn mismatched_outside_and_malformed_cache_headers_require_a_fresh_read() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("one.md"), "one\n").unwrap();
        let (native, failed) = super::super::tool_read_file("one.md", root.path().to_str());
        assert!(!failed);
        assert!(for_request(&native, "two.md", root.path().to_str()).is_none());
        for invalid in [
            "not a native read\none\n",
            "[one.md] (bad lines, 4 bytes)\none\n",
            "[/outside/one.md] (1 lines, 4 bytes)\none\n",
            "[one.md] (1 lines, 4 bytes) unknown\none\n",
        ] {
            assert!(for_request(invalid, "one.md", root.path().to_str()).is_none());
        }
    }

    #[test]
    fn rebinding_preserves_native_range_metadata_and_truncation_body() {
        let root = tempfile::tempdir().unwrap();
        let absolute = root.path().join("long.md");
        std::fs::write(&absolute, "line\n".repeat(3000)).unwrap();
        let (range, failed) = super::super::read_range::execute_args(
            &serde_json::json!({"path":absolute.to_str().unwrap(),"start_line":2,"end_line":3}),
            root.path().to_str(),
        );
        assert!(!failed);
        let rebound = for_request(&range, "long.md", root.path().to_str()).unwrap();
        assert!(rebound
            .lines()
            .next()
            .unwrap()
            .contains("[range: lines 2-3; partial file]"));
        assert_eq!(
            rebound.splitn(3, '\n').nth(2),
            range.split_once('\n').map(|(_, body)| body)
        );
        let (truncated, failed) =
            super::super::tool_read_file(absolute.to_str().unwrap(), root.path().to_str());
        assert!(!failed);
        let rebound = for_request(&truncated, "long.md", root.path().to_str()).unwrap();
        assert_eq!(
            rebound.splitn(3, '\n').nth(2),
            truncated.split_once('\n').map(|(_, body)| body)
        );
    }
}
