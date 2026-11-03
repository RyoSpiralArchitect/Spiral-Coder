use anyhow::{Context, Result};
use serde::Serialize;
use std::path::PathBuf;

use super::{load_spec, resolve_path, select_cases};

#[derive(Debug, Serialize)]
struct CatalogOutput {
    version: u32,
    total: usize,
    cases: Vec<CatalogCase>,
}

#[derive(Debug, Serialize)]
struct CatalogCase {
    id: String,
    tags: Vec<String>,
}

pub(crate) fn run(args: crate::TuiReplayListArgs) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to get current directory")?;
    let base_root = args
        .tool_root
        .clone()
        .or_else(|| cwd.to_str().map(str::to_owned))
        .unwrap_or_else(|| ".".to_owned());
    let base_root_path = PathBuf::from(base_root);
    let spec_path = resolve_path(&base_root_path, args.spec);
    let spec = load_spec(&spec_path)?;
    let selected = select_cases(&spec, args.filter.as_deref(), args.max_cases);
    if selected.is_empty() {
        return Err(super::diagnostics::empty_selection(
            &spec_path,
            &base_root_path,
            args.filter.as_deref(),
            &spec.cases,
        ));
    }
    let output = CatalogOutput {
        version: 1,
        total: selected.len(),
        cases: selected
            .into_iter()
            .map(|case| CatalogCase {
                id: case.id,
                tags: case.tags,
            })
            .collect(),
    };
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
