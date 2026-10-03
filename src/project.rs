/// Lightweight project context scanner.
///
/// Scans a directory root in <200 ms and returns a structured summary that is
/// injected into the Coder's system message so it understands the project
/// without having to ask.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::time::Duration;
use tokio::time::timeout;

// ── Public types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectScanResult {
    pub root: String,
    pub stack_label: String,
    pub stack: Vec<String>,
    pub git_branch: String,
    pub git_modified: u32,
    pub git_untracked: u32,
    pub context_text: String,
}

impl Default for ProjectScanResult {
    fn default() -> Self {
        Self {
            root: String::new(),
            stack_label: String::new(),
            stack: Vec::new(),
            git_branch: String::new(),
            git_modified: 0,
            git_untracked: 0,
            context_text: String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProjectContext {
    pub root: String,
    pub stack: Vec<String>,
    pub git_branch: Option<String>,
    pub git_modified: u32,
    pub git_untracked: u32,
    pub git_recent: Vec<String>,
    pub tree: Vec<(String, usize)>, // (dir_name, file_count)
    pub key_files: Vec<String>,
    pub readme_excerpt: Option<String>,
    /// Content of .spiral-coder.md / AGENTS.md / CLAUDE.md — project-specific instructions.
    pub agents_md: Option<String>,
    /// Auto-detected or .spiral-coder.md-configured test command.
    pub test_cmd: Option<String>,
    repo_map: Option<RepoMapStatus>,
}

#[derive(Debug, Clone, Default)]
struct RepoMapStatus {
    available: bool,
    ready: bool,
    files: usize,
    symbols: usize,
}

// ── Main scan function ─────────────────────────────────────────────────────────

impl ProjectContext {
    /// Scan a root directory.  Returns None only if the path doesn't exist.
    /// All sub-errors (git unavailable, no README, etc.) are silently swallowed.
    pub async fn scan(root: &str) -> Option<Self> {
        let root = root.trim();
        if root.is_empty() {
            return None;
        }
        let p = Path::new(root);
        if !p.is_dir() {
            return None;
        }
        let root = p.to_string_lossy().to_string();

        // Stack detection is sync and fast.
        let stack = detect_stack(&root);

        // Tree is sync and fast (top-level only).
        let tree = scan_tree(&root);

        // Key files list (existence check only).
        let key_files = detect_key_files(&root);

        // README excerpt (sync read, first 15 lines).
        let readme_excerpt = read_readme_excerpt(&root);

        // Git info: run async with tight timeouts.
        let (git_branch, git_modified, git_untracked, git_recent) = scan_git(&root).await;

        // Project-specific instruction file (.spiral-coder.md > AGENTS.md > CLAUDE.md).
        let agents_md = try_read_agents_file(&root);

        // Test command: from .spiral-coder.md `test_cmd:` line, then auto-detect from stack.
        let test_cmd = detect_test_cmd(&root, agents_md.as_deref());

        // Repo-map status: cheap local signal for future lazy integration.
        let repo_map = detect_repo_map_status(&root);

        Some(ProjectContext {
            root,
            stack,
            git_branch,
            git_modified,
            git_untracked,
            git_recent,
            tree,
            key_files,
            readme_excerpt,
            agents_md,
            test_cmd,
            repo_map,
        })
    }

    /// One-line label shown in TUI header and web badge.
    /// E.g. "Rust · React · git:main"
    pub fn stack_label(&self) -> String {
        let mut parts: Vec<String> = self.stack.clone();
        if let Some(ref branch) = self.git_branch {
            parts.push(format!("git:{branch}"));
        }
        if parts.is_empty() {
            return String::new();
        }
        parts.join(" · ")
    }

    /// Build the context text block (<300 tokens) for injection into the system message.
    pub fn to_context_text(&self) -> String {
        let mut out = String::with_capacity(512);
        out.push_str("[Project Context — auto-detected]\n");

        // Stack
        if !self.stack.is_empty() {
            out.push_str(&format!("stack: {}\n", self.stack.join(", ")));
        }

        // Test command (auto-detected or configured in .spiral-coder.md)
        if let Some(ref cmd) = self.test_cmd {
            let cmd = cmd.trim();
            if !cmd.is_empty() {
                out.push_str(&format!("test_cmd: {cmd}\n"));
            }
        }

        let explore = build_explore_recipe(&self.root, &self.stack, self.test_cmd.as_deref());
        if !explore.is_empty() {
            out.push_str("explore:\n");
            for line in explore.iter().take(4) {
                out.push_str(&format!("  - {line}\n"));
            }
        }

        if let Some(ref repo_map) = self.repo_map {
            if repo_map.ready {
                out.push_str(&format!(
                    "repo_map: ready  files={}  symbols={}\n",
                    repo_map.files, repo_map.symbols
                ));
            } else if repo_map.available {
                out.push_str("repo_map: available  build with `python3 scripts/repo_map.py build --root .`\n");
            }
        }

        // Git
        let git_line = match &self.git_branch {
            Some(branch) => format!(
                "git:   branch={}  modified={}  untracked={}\n",
                branch, self.git_modified, self.git_untracked
            ),
            None => "git:   (not a git repository)\n".to_string(),
        };
        out.push_str(&git_line);

        // Recent commits
        if !self.git_recent.is_empty() {
            let commits: Vec<String> = self
                .git_recent
                .iter()
                .map(|c| format!("\"{}\"", c))
                .collect();
            out.push_str(&format!("recent: {}\n", commits.join(" · ")));
        }

        // Tree
        if !self.tree.is_empty() {
            out.push_str("tree:\n");
            for (dir, count) in &self.tree {
                out.push_str(&format!("  {:<14} {} files\n", format!("{}/", dir), count));
            }
        }

        // Key files
        if !self.key_files.is_empty() {
            out.push_str(&format!("key:  {}\n", self.key_files.join(" · ")));
        }

        // README excerpt
        if let Some(ref excerpt) = self.readme_excerpt {
            if !excerpt.trim().is_empty() {
                out.push_str("readme:\n");
                for line in excerpt.lines().take(5) {
                    out.push_str(&format!("  {line}\n"));
                }
            }
        }

        out.trim_end().to_string()
    }

    /// Convert to the serialisable result type for the REST API.
    pub fn to_scan_result(&self) -> ProjectScanResult {
        ProjectScanResult {
            root: self.root.clone(),
            stack_label: self.stack_label(),
            stack: self.stack.clone(),
            git_branch: self.git_branch.clone().unwrap_or_default(),
            git_modified: self.git_modified,
            git_untracked: self.git_untracked,
            context_text: self.to_context_text(),
        }
    }
}

pub fn detect_stack_labels(root: &Path) -> Vec<String> {
    detect_stack(root.to_string_lossy().as_ref())
}

pub fn detect_test_command(root: &Path, agents_md: Option<&str>) -> Option<String> {
    detect_test_cmd(root.to_string_lossy().as_ref(), agents_md)
}

// ── Stack detection ───────────────────────────────────────────────────────────

fn detect_stack(root: &str) -> Vec<String> {
    let mut stack: Vec<String> = Vec::new();
    let root_path = Path::new(root);
    let package_json = read_text_if_exists(&root_path.join("package.json"));

    let mut push_stack = |label: &str| {
        if !stack.iter().any(|item| item == label) {
            stack.push(label.to_string());
        }
    };

    // Rust
    if root_path.join("Cargo.toml").is_file() {
        push_stack("Rust");
    }

    // Node / frontend runtimes
    if package_json.is_some() {
        push_stack("Node");
        if let Some(src) = package_json.as_deref() {
            if src.contains("\"next\"") {
                push_stack("Next.js");
            }
            if src.contains("\"react\"") || src.contains("react-dom") {
                push_stack("React");
            }
            if src.contains("\"vue\"") {
                push_stack("Vue");
            }
            if src.contains("\"svelte\"") {
                push_stack("Svelte");
            }
            if src.contains("\"typescript\"") {
                push_stack("TypeScript");
            }
        }
        if root_path.join("bun.lockb").is_file() || root_path.join("bun.lock").is_file() {
            push_stack("Bun");
        }
    }

    if root_path.join("deno.json").is_file() || root_path.join("deno.jsonc").is_file() {
        push_stack("Deno");
        push_stack("TypeScript");
    }

    // Python
    if root_path.join("pyproject.toml").is_file()
        || root_path.join("requirements.txt").is_file()
        || root_path.join("setup.py").is_file()
        || root_path.join("setup.cfg").is_file()
        || root_path.join("Pipfile").is_file()
    {
        push_stack("Python");
    }

    // Go
    if root_path.join("go.mod").is_file() {
        push_stack("Go");
    }

    // Java / JVM
    if root_path.join("pom.xml").is_file() {
        push_stack("Java");
        push_stack("Maven");
    }
    if root_path.join("build.gradle").is_file() || root_path.join("build.gradle.kts").is_file() {
        push_stack("Gradle");
        push_stack("JVM");
    }

    // Ruby
    if root_path.join("Gemfile").is_file() {
        push_stack("Ruby");
    }

    // PHP
    if root_path.join("composer.json").is_file() {
        push_stack("PHP");
    }

    // Elixir
    if root_path.join("mix.exs").is_file() {
        push_stack("Elixir");
    }

    // .NET
    if has_top_level_suffix(root_path, &[".sln", ".csproj", ".fsproj", ".vbproj"]) {
        push_stack(".NET");
    }

    // Swift
    if root_path.join("Package.swift").is_file() {
        push_stack("Swift");
    }

    // Zig
    if root_path.join("build.zig").is_file() {
        push_stack("Zig");
    }

    // Terraform
    if has_top_level_suffix(root_path, &[".tf"]) || root_path.join("terraform.tfvars").is_file() {
        push_stack("Terraform");
    }

    // C / C++
    if root_path.join("CMakeLists.txt").is_file()
        || root_path.join("meson.build").is_file()
        || root_path.join("compile_commands.json").is_file()
    {
        push_stack("C/C++");
    }

    // tsconfig without package.json
    if root_path.join("tsconfig.json").is_file() {
        push_stack("TypeScript");
    }

    stack
}

fn detect_repo_map_status(root: &str) -> Option<RepoMapStatus> {
    let script_path = Path::new(root).join("scripts").join("repo_map.py");
    let index_path = Path::new(root).join(".spiral").join("repo_map.json");
    if !script_path.is_file() && !index_path.is_file() {
        return None;
    }

    let mut status = RepoMapStatus {
        available: script_path.is_file(),
        ready: false,
        files: 0,
        symbols: 0,
    };

    if let Ok(src) = std::fs::read_to_string(&index_path) {
        if let Ok(v) = serde_json::from_str::<Value>(&src) {
            status.ready = true;
            status.files = v.get("files_indexed").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
            status.symbols = v
                .get("symbols_indexed")
                .and_then(|x| x.as_u64())
                .unwrap_or(0) as usize;
        }
    }

    Some(status)
}

// ── Git scanning ──────────────────────────────────────────────────────────────

async fn scan_git(root: &str) -> (Option<String>, u32, u32, Vec<String>) {
    let branch = run_git(root, &["rev-parse", "--abbrev-ref", "HEAD"]).await;
    if branch.is_none() {
        return (None, 0, 0, vec![]);
    }

    let status_out = run_git(root, &["status", "--short"])
        .await
        .unwrap_or_default();
    let mut modified = 0u32;
    let mut untracked = 0u32;
    for line in status_out.lines() {
        if line.starts_with("??") {
            untracked += 1;
        } else if !line.trim().is_empty() {
            modified += 1;
        }
    }

    let log_out = run_git(root, &["log", "--oneline", "-3", "--no-decorate"])
        .await
        .unwrap_or_default();
    let recent: Vec<String> = log_out
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            // Strip the leading hash (first word).
            if let Some(idx) = l.find(' ') {
                l[idx + 1..].trim().chars().take(60).collect()
            } else {
                l.to_string()
            }
        })
        .collect();

    (branch, modified, untracked, recent)
}

async fn run_git(root: &str, args: &[&str]) -> Option<String> {
    let fut = tokio::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output();

    let out = timeout(Duration::from_secs(2), fut).await.ok()?.ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

// ── Directory tree ────────────────────────────────────────────────────────────

const SKIP_DIRS: &[&str] = &[
    "target",
    "node_modules",
    "__pycache__",
    ".git",
    "dist",
    "build",
    ".next",
    ".nuxt",
    "vendor",
];

fn scan_tree(root: &str) -> Vec<(String, usize)> {
    let dir = match std::fs::read_dir(root) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    let mut entries: Vec<(String, usize)> = Vec::new();

    for entry in dir.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if SKIP_DIRS.contains(&name.as_str()) {
            continue;
        }
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if ft.is_dir() {
            let count = count_direct_files(&entry.path());
            entries.push((name, count));
        }
    }

    // Sort by name for deterministic output.
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries.truncate(8);
    entries
}

fn count_direct_files(dir: &std::path::PathBuf) -> usize {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                .count()
        })
        .unwrap_or(0)
}

// ── Key files ─────────────────────────────────────────────────────────────────

const KEY_CANDIDATES: &[&str] = &[
    "Cargo.toml",
    "package.json",
    "pyproject.toml",
    "go.mod",
    "pom.xml",
    "README.md",
    "readme.md",
    "Makefile",
    "docker-compose.yml",
    "Dockerfile",
    ".env.example",
];

fn detect_key_files(root: &str) -> Vec<String> {
    KEY_CANDIDATES
        .iter()
        .filter(|&&f| Path::new(root).join(f).is_file())
        .map(|&f| f.to_string())
        .collect()
}

// ── README excerpt ─────────────────────────────────────────────────────────────

fn read_readme_excerpt(root: &str) -> Option<String> {
    let path = ["README.md", "readme.md", "README.txt", "README"]
        .iter()
        .map(|n| Path::new(root).join(n))
        .find(|p| p.is_file())?;

    let content = std::fs::read_to_string(path).ok()?;
    let useful: Vec<&str> = content
        .lines()
        .filter(|l| !l.trim_start().starts_with("[!["))
        .take(15)
        .collect();

    let non_empty = useful.iter().filter(|l| !l.trim().is_empty()).count();
    if non_empty < 2 {
        return None;
    }

    Some(useful.join("\n"))
}

/// Detect the test command for this project.
/// Priority: `test_cmd:` line in .spiral-coder.md > auto-detect from stack markers.
fn detect_test_cmd(root: &str, agents_md: Option<&str>) -> Option<String> {
    // 1. Explicit override in .spiral-coder.md: `test_cmd: cargo test --workspace`
    if let Some(content) = agents_md {
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("test_cmd:") {
                let cmd = rest.trim().to_string();
                if !cmd.is_empty() {
                    return Some(cmd);
                }
            }
        }
    }

    // 2. Auto-detect from stack.
    let p = Path::new(root);
    let package_json = read_text_if_exists(&p.join("package.json"));
    if p.join("Cargo.toml").is_file() {
        return Some("cargo test 2>&1".to_string());
    }
    if p.join("package.json").is_file() {
        // Check for a "test" script in package.json
        if let Some(src) = package_json.as_deref() {
            if src.contains("\"test\"") {
                let mgr = if p.join("bun.lockb").is_file() || p.join("bun.lock").is_file() {
                    "bun"
                } else if p.join("pnpm-lock.yaml").is_file() {
                    "pnpm"
                } else if p.join("yarn.lock").is_file() {
                    "yarn"
                } else {
                    "npm"
                };
                return Some(if mgr == "bun" {
                    "bun test 2>&1".to_string()
                } else {
                    format!("{mgr} test --passWithNoTests 2>&1")
                });
            }
        }
    }
    if p.join("deno.json").is_file() || p.join("deno.jsonc").is_file() {
        return Some("deno test 2>&1".to_string());
    }
    if p.join("pyproject.toml").is_file()
        || p.join("pytest.ini").is_file()
        || p.join("setup.cfg").is_file()
        || p.join("requirements.txt").is_file()
    {
        return Some("pytest -q 2>&1".to_string());
    }
    if p.join("go.mod").is_file() {
        return Some("go test ./... 2>&1".to_string());
    }
    if p.join("pom.xml").is_file() {
        return Some("mvn test 2>&1".to_string());
    }
    if p.join("gradlew").is_file() {
        return Some("./gradlew test 2>&1".to_string());
    }
    if p.join("build.gradle").is_file() || p.join("build.gradle.kts").is_file() {
        return Some("gradle test 2>&1".to_string());
    }
    if p.join("mix.exs").is_file() {
        return Some("mix test 2>&1".to_string());
    }
    if has_top_level_suffix(p, &[".sln", ".csproj", ".fsproj", ".vbproj"]) {
        return Some("dotnet test 2>&1".to_string());
    }
    if p.join("Package.swift").is_file() {
        return Some("swift test 2>&1".to_string());
    }
    if p.join("Gemfile").is_file() && (p.join("spec").is_dir() || p.join(".rspec").is_file()) {
        return Some("bundle exec rspec 2>&1".to_string());
    }
    if p.join("composer.json").is_file()
        && (p.join("phpunit.xml").is_file()
            || p.join("phpunit.xml.dist").is_file()
            || p.join("tests").is_dir())
    {
        return Some("vendor/bin/phpunit 2>&1".to_string());
    }
    if p.join("build.zig").is_file() {
        return Some("zig build test 2>&1".to_string());
    }
    if has_top_level_suffix(p, &[".tf"]) || p.join("terraform.tfvars").is_file() {
        return Some("terraform validate 2>&1".to_string());
    }

    None
}

fn build_explore_recipe(root: &str, stack: &[String], test_cmd: Option<&str>) -> Vec<String> {
    let root_path = Path::new(root);
    let mut lines: Vec<String> = Vec::new();
    let mut seen: Vec<&'static str> = Vec::new();
    let mut push_once = |key: &'static str, text: String| {
        if !seen.contains(&key) {
            seen.push(key);
            lines.push(text);
        }
    };

    let rust_entry = first_existing_path(root_path, &["src/lib.rs", "src/main.rs", "src/bin"]);
    let rust_tests = first_existing_path(root_path, &["tests", "examples"]);
    let web_entry = first_existing_path(
        root_path,
        &[
            "app",
            "pages",
            "src/main.tsx",
            "src/main.ts",
            "src/index.tsx",
            "src/index.ts",
            "src/index.js",
            "src",
        ],
    );
    let web_tests = first_existing_path(
        root_path,
        &[
            "tests",
            "__tests__",
            "cypress",
            "playwright.config.ts",
            "vitest.config.ts",
        ],
    );
    let python_entry = first_existing_path(
        root_path,
        &["pyproject.toml", "src", "app", "main.py", "manage.py"],
    );
    let python_tests = first_existing_path(root_path, &["tests", "test", "pytest.ini"]);
    let go_entry = first_existing_path(root_path, &["go.mod", "cmd", "main.go", "internal", "pkg"]);
    let go_tests = first_existing_path(root_path, &["*_test.go", "test", "tests"]);
    let jvm_entry = first_existing_path(
        root_path,
        &[
            "pom.xml",
            "build.gradle.kts",
            "build.gradle",
            "src/main",
            "src",
        ],
    );
    let jvm_tests = first_existing_path(root_path, &["src/test", "test", "tests"]);
    let ruby_entry = first_existing_path(root_path, &["Gemfile", "lib", "app"]);
    let ruby_tests = first_existing_path(root_path, &["spec", "test", ".rspec"]);
    let php_entry = first_existing_path(root_path, &["composer.json", "src", "app"]);
    let php_tests = first_existing_path(root_path, &["tests", "phpunit.xml", "phpunit.xml.dist"]);
    let elixir_entry = first_existing_path(root_path, &["mix.exs", "lib"]);
    let elixir_tests = first_existing_path(root_path, &["test"]);
    let dotnet_entry = first_existing_path(
        root_path,
        &["*.sln", "*.csproj", "*.fsproj", "*.vbproj", "src"],
    );
    let dotnet_tests = first_existing_path(root_path, &["tests", "test"]);
    let swift_entry = first_existing_path(root_path, &["Package.swift", "Sources"]);
    let swift_tests = first_existing_path(root_path, &["Tests"]);
    let zig_entry = first_existing_path(root_path, &["build.zig", "src"]);
    let terraform_entry = first_existing_path(
        root_path,
        &["main.tf", "versions.tf", "providers.tf", "modules"],
    );
    let cpp_entry = first_existing_path(
        root_path,
        &["CMakeLists.txt", "meson.build", "include", "src"],
    );
    let cpp_tests = first_existing_path(root_path, &["tests", "test"]);

    for item in stack {
        match item.as_str() {
            "Rust" => push_once(
                "rust",
                format!(
                    "Rust: read `Cargo.toml` first, then `{}`, then `{}` before editing.",
                    rust_entry.as_deref().unwrap_or("src/lib.rs or src/main.rs"),
                    rust_tests.as_deref().unwrap_or("tests/ or examples/"),
                ),
            ),
            "Node" | "React" | "Next.js" | "Vue" | "Svelte" | "TypeScript" | "Bun" | "Deno" => push_once(
                "web",
                format!(
                    "JS/TS: read `{}` first, then `{}`, then `{}` before editing.",
                    if root_path.join("deno.json").is_file() || root_path.join("deno.jsonc").is_file() {
                        "deno.json"
                    } else {
                        "package.json"
                    },
                    web_entry.as_deref().unwrap_or("src/ entrypoints"),
                    web_tests.as_deref().unwrap_or("tests/"),
                ),
            ),
            "Python" => push_once(
                "python",
                format!(
                    "Python: read `{}` first, then `{}`, then `{}` before editing.",
                    first_existing_path(
                        root_path,
                        &["pyproject.toml", "requirements.txt", "setup.cfg", "setup.py", "Pipfile"],
                    )
                    .unwrap_or_else(|| "pyproject.toml or requirements.txt".to_string()),
                    python_entry.as_deref().unwrap_or("package entrypoints"),
                    python_tests.as_deref().unwrap_or("tests/"),
                ),
            ),
            "Go" => push_once(
                "go",
                format!(
                    "Go: read `go.mod` first, then `{}`, then `{}` before editing.",
                    go_entry.as_deref().unwrap_or("cmd/ or package entrypoints"),
                    go_tests.as_deref().unwrap_or("*_test.go files"),
                ),
            ),
            "Java" | "Maven" | "Gradle" | "JVM" => push_once(
                "jvm",
                format!(
                    "JVM: read `{}` first, then `{}`, then `{}` before editing.",
                    first_existing_path(root_path, &["pom.xml", "build.gradle.kts", "build.gradle"])
                        .unwrap_or_else(|| "pom.xml or build.gradle".to_string()),
                    jvm_entry.as_deref().unwrap_or("src/main"),
                    jvm_tests.as_deref().unwrap_or("src/test"),
                ),
            ),
            "Ruby" => push_once(
                "ruby",
                format!(
                    "Ruby: read `Gemfile` first, then `{}`, then `{}` before editing.",
                    ruby_entry.as_deref().unwrap_or("lib/"),
                    ruby_tests.as_deref().unwrap_or("spec/ or test/"),
                ),
            ),
            "PHP" => push_once(
                "php",
                format!(
                    "PHP: read `composer.json` first, then `{}`, then `{}` before editing.",
                    php_entry.as_deref().unwrap_or("src/"),
                    php_tests.as_deref().unwrap_or("tests/"),
                ),
            ),
            "Elixir" => push_once(
                "elixir",
                format!(
                    "Elixir: read `mix.exs` first, then `{}`, then `{}` before editing.",
                    elixir_entry.as_deref().unwrap_or("lib/"),
                    elixir_tests.as_deref().unwrap_or("test/"),
                ),
            ),
            ".NET" => push_once(
                "dotnet",
                format!(
                    ".NET: read `{}` first, then `{}`, then `{}` before editing.",
                    first_existing_path(root_path, &["*.sln", "*.csproj", "*.fsproj", "*.vbproj"])
                        .unwrap_or_else(|| "*.sln or *.csproj".to_string()),
                    dotnet_entry.as_deref().unwrap_or("Program.cs or project entrypoints"),
                    dotnet_tests.as_deref().unwrap_or("test projects"),
                ),
            ),
            "Swift" => push_once(
                "swift",
                format!(
                    "Swift: read `Package.swift` first, then `{}`, then `{}` before editing.",
                    swift_entry.as_deref().unwrap_or("Sources/"),
                    swift_tests.as_deref().unwrap_or("Tests/"),
                ),
            ),
            "Zig" => push_once(
                "zig",
                format!(
                    "Zig: read `build.zig` first, then `{}`, then zig test targets before editing.",
                    zig_entry.as_deref().unwrap_or("src/"),
                ),
            ),
            "Terraform" => push_once(
                "terraform",
                format!(
                    "Terraform: read `{}` first, then variables/outputs/modules before editing; inspect provider and module wiring first.",
                    terraform_entry
                        .as_deref()
                        .unwrap_or("main.tf or provider/root *.tf files"),
                ),
            ),
            "C/C++" => push_once(
                "cpp",
                format!(
                    "C/C++: read `{}` first, then `{}`, then `{}` before editing.",
                    first_existing_path(root_path, &["CMakeLists.txt", "meson.build"])
                        .unwrap_or_else(|| "CMakeLists.txt or meson.build".to_string()),
                    cpp_entry.as_deref().unwrap_or("include/ and src/"),
                    cpp_tests.as_deref().unwrap_or("tests/"),
                ),
            ),
            _ => {}
        }
    }

    if lines.is_empty() {
        let hint = if root_path.join("README.md").is_file() || root_path.join("readme.md").is_file()
        {
            "Start with README + key manifests, then read the smallest likely entrypoint before editing."
        } else {
            "Start with key manifests and likely entrypoints before editing any existing file."
        };
        lines.push(hint.to_string());
    }

    if let Some(cmd) = test_cmd.map(str::trim).filter(|cmd| !cmd.is_empty()) {
        lines.push(format!(
            "Verify after edits with `{cmd}` once the target files are understood."
        ));
    }

    lines
}

fn read_text_if_exists(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn has_top_level_suffix(root: &Path, suffixes: &[&str]) -> bool {
    std::fs::read_dir(root)
        .ok()
        .into_iter()
        .flat_map(|rd| rd.flatten())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .any(|name| suffixes.iter().any(|suffix| name.ends_with(suffix)))
}

fn first_existing_path(root: &Path, candidates: &[&str]) -> Option<String> {
    for candidate in candidates {
        if candidate.contains('*') {
            if let Some(found) = first_top_level_match(root, candidate) {
                return Some(found);
            }
            continue;
        }
        let path = root.join(candidate);
        if path.exists() {
            return Some(candidate.to_string());
        }
    }
    None
}

fn first_top_level_match(root: &Path, pattern: &str) -> Option<String> {
    let (prefix, suffix) = pattern.split_once('*').unwrap_or(("", pattern));
    std::fs::read_dir(root)
        .ok()?
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .find(|name| name.starts_with(prefix) && name.ends_with(suffix))
}

/// Read the first project instruction file found in `root`.
/// Priority: .spiral-coder.md > AGENTS.md > CLAUDE.md
/// Truncated to 200 lines to keep token cost bounded.
fn try_read_agents_file(root: &str) -> Option<String> {
    for name in &[".spiral-coder.md", "AGENTS.md", "CLAUDE.md"] {
        let path = Path::new(root).join(name);
        if let Ok(content) = std::fs::read_to_string(&path) {
            let trimmed = content.trim();
            if trimmed.is_empty() {
                continue;
            }
            // Cap at 200 lines to avoid blowing the context window.
            let capped: String = trimmed.lines().take(200).collect::<Vec<_>>().join("\n");
            return Some(capped);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn detect_stack_labels_covers_more_ecosystems() {
        let td = tempfile::tempdir().expect("tempdir");
        fs::write(
            td.path().join("mix.exs"),
            "defmodule Demo.MixProject do end",
        )
        .expect("mix");
        fs::write(td.path().join("composer.json"), "{}").expect("composer");
        fs::write(td.path().join("Package.swift"), "// swift package").expect("swift");
        fs::write(td.path().join("main.tf"), "terraform {}").expect("tf");
        fs::write(td.path().join("build.zig"), "const std = @import(\"std\");").expect("zig");
        fs::write(
            td.path().join("CMakeLists.txt"),
            "cmake_minimum_required(VERSION 3.24)",
        )
        .expect("cmake");
        fs::write(td.path().join("app.sln"), "").expect("sln");

        let stack = detect_stack_labels(td.path());
        assert!(stack.iter().any(|item| item == "Elixir"));
        assert!(stack.iter().any(|item| item == "PHP"));
        assert!(stack.iter().any(|item| item == "Swift"));
        assert!(stack.iter().any(|item| item == "Terraform"));
        assert!(stack.iter().any(|item| item == "Zig"));
        assert!(stack.iter().any(|item| item == "C/C++"));
        assert!(stack.iter().any(|item| item == ".NET"));
    }

    #[test]
    fn context_text_includes_explore_recipe_and_detected_test_command() {
        let td = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(td.path().join("src")).expect("src dir");
        fs::write(
            td.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("cargo");
        fs::write(td.path().join("src/main.rs"), "fn main() {}").expect("main");

        let ctx = ProjectContext {
            root: td.path().to_string_lossy().into_owned(),
            stack: detect_stack_labels(td.path()),
            git_branch: None,
            git_modified: 0,
            git_untracked: 0,
            git_recent: Vec::new(),
            tree: Vec::new(),
            key_files: vec!["Cargo.toml".to_string()],
            readme_excerpt: None,
            agents_md: None,
            test_cmd: detect_test_command(td.path(), None),
            repo_map: None,
        };

        let text = ctx.to_context_text();
        assert!(text.contains("explore:"));
        assert!(text.contains("Rust: read `Cargo.toml` first"));
        assert!(text.contains("test_cmd: cargo test 2>&1"));
        assert!(text.contains("Verify after edits with `cargo test 2>&1`"));
    }
}
