//! Advisory source membership for an observed Cargo target. Never feeds proof or tool routing.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

const WATCH: &str = "new_rust_sources";
const RECEIPT: &str = "cargo_inputs";
const MAX_FILES: usize = 8;
const MAX_DEPFILE: u64 = 2 * 1024 * 1024;

fn source_name(name: &str) -> bool {
    let p = Path::new(name);
    name.len() <= 240
        && !name.chars().any(char::is_control)
        && p.extension().is_some_and(|ext| ext == "rs")
        && p.components().all(|c| matches!(c, Component::Normal(_)))
}

/// Observe absence immediately before the approved native write, never from its proposed arguments alone.
pub(crate) fn write_file(
    path: &str,
    content: &str,
    root: Option<&str>,
    messages: &mut [Value],
) -> (String, bool) {
    let absent_root = root.filter(|root| {
        Path::new(path).extension().is_some_and(|e| e == "rs")
            && crate::file_tools::resolve_safe_path(path, Some(root)).is_ok_and(|p| {
                fs::symlink_metadata(p).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
            })
    });
    let result = crate::file_tools::tool_write_file(path, content, root);
    if let Some(root) = absent_root.filter(|_| {
        !result.1 && crate::execution_evidence::file_edit_succeeded("write_file", &result.0)
    }) {
        created(messages, root, path);
    }
    result
}

/// The original human message survives window pruning. Runtime continuations share it.
fn created(messages: &mut [Value], root: &str, path: &str) {
    let Ok(root) = fs::canonicalize(root) else {
        return;
    };
    let Ok(file) = fs::canonicalize(root.join(path)) else {
        return;
    };
    let Some(name) = file
        .strip_prefix(root)
        .ok()
        .and_then(Path::to_str)
        .filter(|p| source_name(p))
    else {
        return;
    };
    let Some(task) = messages
        .iter_mut()
        .rev()
        .find(|m| crate::task_origin::is_human_request(m))
    else {
        return;
    };
    let name = name.replace(std::path::MAIN_SEPARATOR, "/");
    let mut paths = watched(task);
    paths.retain(|p| p != &name);
    paths.push(name);
    if paths.len() > MAX_FILES {
        paths.remove(0);
    }
    task[WATCH] = serde_json::json!(paths);
}

fn watched(task: &Value) -> Vec<String> {
    task[WATCH]
        .as_array()
        .into_iter()
        .flatten()
        .take(MAX_FILES)
        .filter_map(Value::as_str)
        .filter(|p| source_name(p))
        .map(String::from)
        .collect()
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Status {
    Listed,
    NotListed,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Receipt {
    target: Option<String>,
    files: BTreeMap<String, Status>,
}

impl Receipt {
    pub(crate) fn from_message(message: &Value) -> Option<Self> {
        if message["role"] != "tool"
            && !(message["role"] == "user" && message["origin"] == "runtime")
        {
            return None;
        }
        let row = message.get(RECEIPT)?;
        let files = row.get("files")?.as_object()?;
        if files.is_empty() || files.len() > MAX_FILES || !files.keys().all(|p| source_name(p)) {
            return None;
        }
        if row["target"]
            .as_str()
            .is_some_and(|s| s.len() > 128 || s.chars().any(char::is_control))
        {
            return None;
        }
        serde_json::from_value(row.clone()).ok()
    }

    pub(crate) fn attach(receipt: Option<Self>, mut message: Value) -> Value {
        if let Some(r) = receipt {
            message[RECEIPT] = serde_json::json!(r);
        }
        message
    }
}

pub(crate) fn render_for_coder(message: &mut Value) {
    let receipt = Receipt::from_message(message);
    let Some(object) = message.as_object_mut() else {
        return;
    };
    object.remove(WATCH);
    object.remove(RECEIPT);
    if let Some(row) = receipt {
        if let Some(Value::String(text)) = object.get_mut("content") {
            text.push_str("\n[Cargo compiler inputs for this target, not runtime coverage] ");
            text.push_str(&serde_json::json!(row).to_string());
        }
    }
}

enum Target {
    UnitTest {
        deps: PathBuf,
        bin: Option<String>,
        merged: bool,
        started: SystemTime,
        finished_banner: &'static str,
    },
    Binary {
        profile: PathBuf,
        bin: Option<String>,
        before: BTreeMap<PathBuf, (SystemTime, u64)>,
    },
}

pub(crate) struct Probe {
    cwd: PathBuf,
    target: Option<Target>,
    paths: Vec<String>,
}

impl Probe {
    pub(crate) fn start(messages: &[Value], command: &str, cwd: &str, root: &str) -> Option<Self> {
        let paths = watched(crate::task_origin::current_task_messages(messages).first()?);
        let mut words = command.split_ascii_whitespace();
        if paths.is_empty() || words.next() != Some("cargo") {
            return None;
        }
        let verb = words.next()?;
        if !matches!(verb, "test" | "build" | "check") {
            return None;
        }
        let cwd = fs::canonicalize(cwd).ok()?;
        let target = (fs::canonicalize(root).ok().as_ref() == Some(&cwd))
            .then(|| target(verb, words.collect(), &cwd))
            .flatten();
        Some(Self { cwd, target, paths })
    }

    pub(crate) fn finish(self, success: bool, stdout: &str, stderr: &str) -> Receipt {
        let evidence = if success {
            self.target
                .and_then(|t| t.inputs(&self.cwd, stdout, stderr))
        } else {
            None
        };
        let files = self
            .paths
            .into_iter()
            .map(|name| {
                let status = match (&evidence, self.cwd.join(&name).canonicalize()) {
                    (Some((_, inputs)), Ok(path)) if inputs.contains(&path) => Status::Listed,
                    (Some(_), Ok(path)) if path.starts_with(&self.cwd) => Status::NotListed,
                    _ => Status::Unknown,
                };
                (name, status)
            })
            .collect();
        Receipt {
            target: evidence.map(|(label, _)| label),
            files,
        }
    }
}

fn target(verb: &str, mut args: Vec<&str>, cwd: &Path) -> Option<Target> {
    let merged = args.last() == Some(&"2>&1");
    if merged {
        args.pop();
    }
    let mut profile = "debug";
    let mut bin = None;
    let mut target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| "target".into());
    let mut args = args.into_iter();
    while let Some(word) = args.next() {
        match word {
            "--offline" | "--locked" | "--frozen" | "--quiet" | "-q" => (),
            "--release" => profile = "release",
            "--bin" => {
                let name = args.next()?;
                if name.is_empty()
                    || !name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                {
                    return None;
                }
                if bin.replace(name.to_owned()).is_some() {
                    return None;
                }
            }
            "--target-dir" => {
                let dir = args.next()?;
                if !dir
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_./".contains(&b))
                {
                    return None;
                }
                target_dir = dir.into();
            }
            _ => return None,
        }
    }
    let profile_dir = cwd.join(target_dir).join(profile);
    match verb {
        "test" => Some(Target::UnitTest {
            deps: profile_dir.join("deps"),
            bin,
            merged,
            started: SystemTime::now(),
            finished_banner: if profile == "release" {
                "Finished `release` profile "
            } else {
                "Finished `test` profile "
            },
        }),
        "build" => {
            let before = product_depfiles(&profile_dir, bin.as_deref())?;
            Some(Target::Binary {
                profile: profile_dir,
                bin,
                before,
            })
        }
        _ => None,
    }
}

fn stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let meta = fs::symlink_metadata(path).ok()?;
    meta.is_file()
        .then_some((meta.modified().ok()?, meta.len()))
}

fn product_depfiles(
    profile: &Path,
    bin: Option<&str>,
) -> Option<BTreeMap<PathBuf, (SystemTime, u64)>> {
    let mut stamps = BTreeMap::new();
    if let Some(name) = bin {
        let p = profile.join(format!("{name}.d"));
        if let Some(stamp) = stamp(&p) {
            stamps.insert(p, stamp);
        }
    } else {
        let entries = match profile.read_dir() {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Some(stamps),
            Err(_) => return None,
        };
        for (i, entry) in entries.enumerate() {
            if i >= 128 {
                return None;
            }
            let p = entry.ok()?.path();
            if p.extension().is_some_and(|ext| ext == "d") {
                if let Some(stamp) = stamp(&p) {
                    stamps.insert(p, stamp);
                }
            }
        }
    }
    Some(stamps)
}

impl Target {
    fn inputs(self, cwd: &Path, stdout: &str, stderr: &str) -> Option<(String, BTreeSet<PathBuf>)> {
        let (executable, entry, label) = match self {
            Self::Binary {
                profile,
                bin,
                before,
            } => {
                let after = product_depfiles(&profile, bin.as_deref())?;
                let mut changed = after
                    .iter()
                    .filter(|(p, stamp)| before.get(*p) != Some(*stamp))
                    .map(|(p, _)| p.with_extension(std::env::consts::EXE_EXTENSION))
                    .filter(|p| p.is_file());
                let executable = changed.next()?;
                if changed.next().is_some() {
                    return None;
                }
                let name = executable.file_stem()?.to_str()?;
                let label = format!("bin {name}");
                (executable, "src/main.rs", label)
            }
            Self::UnitTest {
                deps,
                bin,
                merged,
                started,
                finished_banner,
            } => {
                let output = if merged { stdout } else { stderr };
                let mut lines = output.lines().map(str::trim);
                lines.find(|l| l.starts_with(finished_banner))?;
                // Select only Cargo's first runner announcement, before that runner's output.
                let line = lines
                    .find(|l| !l.is_empty())?
                    .strip_prefix("Running unittests ")?;
                let (entry, exe) = line.strip_suffix(')')?.split_once(" (")?;
                let entry = match entry.replace('\\', "/").as_str() {
                    "src/main.rs" => "src/main.rs",
                    "src/lib.rs" if bin.is_none() => "src/lib.rs",
                    _ => return None,
                };
                let candidate = cwd.join(exe);
                if !fs::symlink_metadata(&candidate).ok()?.is_file() {
                    return None;
                }
                let executable = fs::canonicalize(candidate).ok()?;
                if executable.parent()? != fs::canonicalize(deps).ok()? {
                    return None;
                }
                // A cached sidecar and a line printed by a test cannot establish a fresh observation.
                if stamp(&executable.with_extension("d"))?.0 <= started {
                    return None;
                }
                let (name, hash) = executable.file_stem()?.to_str()?.rsplit_once('-')?;
                if hash.len() != 16
                    || !hash.bytes().all(|c| c.is_ascii_hexdigit())
                    || bin.is_some_and(|b| b.replace('-', "_") != name)
                {
                    return None;
                }
                (executable, entry, format!("unit tests {entry}"))
            }
        };
        if !fs::symlink_metadata(&executable).ok()?.is_file() {
            return None;
        }
        let executable = fs::canonicalize(executable).ok()?;
        let inputs = dependencies(&executable, cwd)?;
        inputs
            .contains(&cwd.join(entry).canonicalize().ok()?)
            .then_some((label, inputs))
    }
}

/// Use just the executable rule. Rustc also writes phony rules for individual inputs.
fn dependencies(exe: &Path, cwd: &Path) -> Option<BTreeSet<PathBuf>> {
    let file = exe.with_extension("d");
    let meta = fs::symlink_metadata(&file).ok()?;
    if !meta.is_file() || meta.len() > MAX_DEPFILE {
        return None;
    }
    let mut text = String::new();
    fs::File::open(file)
        .ok()?
        .take(MAX_DEPFILE + 1)
        .read_to_string(&mut text)
        .ok()?;
    if text.len() as u64 > MAX_DEPFILE {
        return None;
    }
    let mut rules = text
        .lines()
        .filter_map(|line| line.split_once(": "))
        .filter(|(lhs, _)| {
            // canonicalize() on Windows returns a verbatim path, while Cargo may print a DOS path.
            let display = exe.to_str().unwrap_or("");
            let display = if cfg!(windows) {
                display.strip_prefix(r"\\?\").unwrap_or(display)
            } else {
                display
            };
            Path::new(lhs) == exe || Path::new(&lhs.replace("\\ ", " ")) == Path::new(display)
        });
    let (_, body) = rules.next()?;
    if rules.next().is_some() || body.ends_with('\\') {
        return None;
    }
    let mut paths = BTreeSet::new();
    let mut word = String::new();
    let mut chars = body.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' if chars.peek() == Some(&' ') => {
                chars.next();
                word.push(' ');
            }
            ' ' if !word.is_empty() => {
                if paths.len() >= 8192 {
                    return None;
                }
                paths.insert(cwd.join(&word).canonicalize().ok()?);
                word.clear();
            }
            ' ' => (),
            c if c.is_control() => return None,
            c => word.push(c),
        }
    }
    if !word.is_empty() {
        paths.insert(cwd.join(word).canonicalize().ok()?);
    }
    Some(paths)
}

#[cfg(test)]
mod tests;
