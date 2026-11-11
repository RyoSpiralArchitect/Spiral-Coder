use super::*;

pub const DEFAULT_MAX_ITERS: usize = 12;

pub(crate) const MAX_STDOUT_CHARS: usize = 1500;

pub(crate) const MAX_STDERR_CHARS: usize = 600;

#[cfg(test)]
pub(crate) const KEEP_RECENT_TOOL_TURNS: usize = 4;

#[cfg(test)]
pub(crate) const KEEP_RECENT_ASSISTANT_TURNS: usize = 6;

pub(crate) const SUCCESS_TOOL_HISTORY_MAX_LINES: usize = 10;

pub(crate) const SUCCESS_TOOL_HISTORY_MAX_CHARS: usize = 1200;

pub(crate) const TOKEN_BUDGET_WARN_TOKENS: usize = 9000;

/// Marker appended to every exec call so we can persist working directory across tool runs.
///
/// IMPORTANT: Each `exec` runs in a fresh process. Without this, `cd` is lost between calls,
/// which causes nested-git disasters and "why did it run in the repo root?" failures.
pub(crate) const PWD_MARKER: &str = "__SPIRAL_CODER_PWD__=";

/// Injected for Windows PowerShell environments.
pub(crate) const WIN_SYSTEM_ADDON: &str = "\n\n[Windows execution rules]\n\
- You are on Windows. Use PowerShell syntax.\n\
- NEVER use here-strings (@' or @\"). Write files line-by-line with Set-Content / Add-Content, or use Out-File.\n\
- Use single-line commands or semicolons to chain statements.\n\
- Check $LASTEXITCODE after commands that may fail.\n\
- Prefer relative paths inside the project directory.";

/// Fixed base for the TUI Coder pane — always an agentic executor, not a chat bot.
pub(crate) const CODER_BASE_SYSTEM: &str = "\
You are an autonomous coding agent with 9 tools:\n\
  exec(command, cwd?)                       — run shell commands (build, test, git, installs)\n\
  read_file(path)                           — read a file's exact content\n\
  write_file(path, content)                 — create or overwrite a file reliably\n\
  patch_file(path, search, replace)         — replace an exact snippet in a file\n\
  apply_diff(path, diff)                    — apply a unified @@ diff (multiple hunks)\n\
  search_files(pattern, dir?, ci?)          — find text across files (like grep -rn)\n\
  list_dir(dir?, max_entries?, include_hidden?) — list a directory (non-recursive)\n\
  glob(pattern, dir?)                       — find files by name pattern (like find -name)\n\
  done(summary, completed_acceptance, remaining_acceptance, acceptance_evidence, next_steps?)\n\
                                           — finish the task and end the loop\n\
\n\
RULE: You MUST call a tool on every single turn. Never respond with text only.\n\
\n\
Fallback for non-tool models (implied actions):\n\
  - To create a NEW file, output this exact pattern:\n\
      File: path/to/file.ext\n\
      ```lang\n\
      file contents...\n\
      ```\n\
    (Spiral-Coder may auto-write the file with approval; it will NOT overwrite existing files.)\n\
  - To run commands, paste a PowerShell/bash code fence.\n\
\n\
PRIORITY (safety-first default when unsure):\n\
  1) read_file                  — before editing any existing file\n\
  2) list_dir/glob/search_files  — discover files (prefer list_dir for quick structure)\n\
  3) patch_file/apply_diff       — prefer for edits (less destructive)\n\
  4) write_file                  — ONLY for new files, or after read_file if overwriting\n\
  5) exec                        — build/test/git/install; avoid for file I/O\n\
\n\
Plan enforcement:\n\
  - Every tool call MUST map to a step in your <plan>. If not, update <plan> first.\n\
  - Your <plan> MUST include explicit `acceptance:` criteria. Verification must satisfy them.\n\
  - __INSTRUCTION_RESOLVER_SCRATCHPAD_RULE__\n\
  - For existing-file mutation, emit <evidence> first; if evidence is weak, inspect instead of mutating.\n\
\n\
Choose the right tool:\n\
  Quick directory listing  → list_dir      (structure discovery, low token)\n\
  List files by pattern    → glob          (NOT exec+ls/find; OS-agnostic, token-efficient)\n\
  Find text in files       → search_files  (NOT exec+grep; safer, token-efficient)\n\
  Create/overwrite file    → write_file    (handles encoding; more reliable than exec+echo)\n\
  Small targeted edit      → read_file → patch_file  (simple single-snippet replacement)\n\
  Complex multi-hunk edit  → read_file → apply_diff  (multiple changes, spans many lines)\n\
  Run programs/tests       → exec\n\
  Git / installs           → exec\n\
\n\
apply_diff format (include 2-3 context lines for reliable matching):\n\
  @@ -10,4 +10,5 @@\n\
   context line\n\
  -old line to remove\n\
  +new line to add\n\
   context line\n\
\n\
After every file edit: tests run automatically if configured — check the result.\n\
After every build/test: confirm exit_code == 0 before proceeding.\n\
\n\
When ALL steps from your <plan> are verified complete:\n\
  call exec one final time to run a smoke test or confirm the deliverable exists,\n\
  then call done with: (1) a brief summary, (2) which acceptance criteria are satisfied,\n\
  (3) which acceptance criteria remain, if any, and (4) the exact verification command used for each completed criterion.\n\
  Do NOT call done while any command is still failing.";

pub(crate) fn read_only_task_addon() -> &'static str {
    "[Read-Only Task Contract]\n\
This task is read-only inspection only.\n\
- Do NOT edit files.\n\
- Do NOT run exec/build/test/manual-behavioral checks just to finish.\n\
- Use observation tools only: list_dir, glob, search_files, read_file.\n\
- Once the file path and handling context are confirmed by successful observation commands, call done directly.\n\
- In read-only plans, acceptance should focus on locating and confirming the code path/context; meta constraints like `no files modified` are constraints, not acceptance targets."
}

pub fn exec_tool_def() -> serde_json::Value {
    json!({
        "type": "function",
        "function": {
            "name": "exec",
            "description": "Execute a shell command on the local machine and return stdout/stderr/exit_code.\n\
                            On Windows: PowerShell.  On Linux/macOS: sh.\n\
                            ALWAYS check exit_code — 0 = success, non-zero = failure.\n\
                            If exit_code != 0: STOP, diagnose the error, then fix it.\n\
                            Do NOT proceed to the next step while any command is failing.\n\
                            Supply either command (optional cwd), or a declared check_id alone.",
            "parameters": {
                "type": "object",
                "properties": {
                    "check_id": {
                        "type": "string",
                        "description": "Select a declared exact check at tool_root. Use alone, without command or cwd."
                    },
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute. Single-line preferred."
                    },
                    "cwd": {
                        "type": "string",
                        "description": "Optional working directory (absolute or relative path)."
                    }
                },
                "required": []
            }
        }
    })
}

pub fn read_file_tool_def() -> serde_json::Value {
    json!({
        "type": "function",
        "function": {
            "name": "read_file",
            "description": "Read the full content of a file. Use before editing to see the exact current text. \
                            Path is relative to tool_root (or absolute within tool_root). \
                            Large files are truncated. Set start_line/end_line (one-based inclusive, at most 200 lines) to read later sections. A partial read does not authorize write_file; use patch_file for targeted edits.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to tool_root."
                    },
                    "start_line": {"type":"integer", "minimum":1, "description":"First line, one-based. Defaults to 1."},
                    "end_line": {"type":"integer", "minimum":1, "description":"Last line, inclusive. Defaults to start_line + 199; at most 200 lines."}
                },
                "required": ["path"]
            }
        }
    })
}

pub fn write_file_tool_def() -> serde_json::Value {
    json!({
        "type": "function",
        "function": {
            "name": "write_file",
            "description": "Atomically create or overwrite a file with the given content. \
                            Creates parent directories automatically. \
                            More reliable than exec+echo for file creation (handles encoding, special chars, newlines). \
                            Path is relative to tool_root.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to tool_root."
                    },
                    "content": {
                        "type": "string",
                        "description": "Complete file content to write."
                    }
                },
                "required": ["path", "content"]
            }
        }
    })
}

pub fn patch_file_tool_def() -> serde_json::Value {
    json!({
        "type": "function",
        "function": {
            "name": "patch_file",
            "description": "Edit a file by replacing an exact text snippet. \
                            The search string MUST appear exactly once in the file. \
                            Call read_file first to see the exact current text if unsure. \
                            For JSON use json_pointer and JSON-encoded json_value instead of search/replace: \
                            append at /cases/- or replace an existing member with json_expected. \
                            Never reprint a large JSON document. Path is relative to tool_root.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to tool_root."
                    },
                    "search": {
                        "type": "string",
                        "description": "Exact text to find (must match character-for-character, including whitespace and newlines)."
                    },
                    "replace": {
                        "type": "string",
                        "description": "Text to substitute in place of the search string."
                    },
                    "json_pointer": {"type":"string", "description":"JSON pointer. Append at an existing array followed by /-, e.g. /cases/-. Omit search/replace."},
                    "json_value": {"type":"string", "description":"Only the new JSON value, encoded as a string. Existing document is preserved by the runtime."},
                    "json_expected": {"type":"string", "description":"Required for member replacement: the current JSON value encoded as a string. Omit for append."}
                },
                "required": ["path"]
            }
        }
    })
}

pub fn search_files_tool_def() -> serde_json::Value {
    json!({
        "type": "function",
        "function": {
            "name": "search_files",
            "description": "Search file contents for a literal text pattern (like grep -rn). \
                            Returns matching lines as 'file:line: content'. \
                            PREFER this over exec+grep — it is faster, safer, and token-efficient. \
                            Use to find function definitions, TODO items, error strings, or any \
                            pattern across the codebase. Dir is relative to tool_root.",
            "parameters": {
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Literal text to search for (not a regex)."
                    },
                    "dir": {
                        "type": "string",
                        "description": "File or directory relative to tool_root. To trace a CLI command, try a working sibling's name in the source file that parses it. In a Rust file, search the exact helper name to see declaration and call-like lines; inspect them before importing. A scoped command search can also suggest the Rust spelling. Leave empty to search the whole tree."
                    },
                    "case_insensitive": {
                        "type": "boolean",
                        "description": "If true, search ignores case. Default: false."
                    }
                },
                "required": ["pattern"]
            }
        }
    })
}

pub fn apply_diff_tool_def() -> serde_json::Value {
    json!({
        "type": "function",
        "function": {
            "name": "apply_diff",
            "description": "Apply a unified diff to a file. More reliable than patch_file for \
                            complex edits that span many lines or have multiple hunks. \
                            Use standard @@ unified diff format. Each hunk is matched by \
                            content (context + remove lines), so exact line numbers are not required. \
                            Multiple hunks per call are supported. \
                            ALWAYS include 2-3 context lines around changes for reliable matching.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to tool_root."
                    },
                    "diff": {
                        "type": "string",
                        "description": "Unified diff string with @@ hunks. Example:\n@@ -10,5 +10,6 @@\n context\n-old line\n+new line\n context"
                    }
                },
                "required": ["path", "diff"]
            }
        }
    })
}

pub fn list_dir_tool_def() -> serde_json::Value {
    json!({
        "type": "function",
        "function": {
            "name": "list_dir",
            "description": "List a directory (non-recursive). Use this to quickly discover the project structure.\n\
                            Prefer this over glob when you don't know the exact pattern yet.\n\
                            Dir is relative to tool_root. If dir is empty, list tool_root/current directory.",
            "parameters": {
                "type": "object",
                "properties": {
                    "dir": {
                        "type": "string",
                        "description": "Directory to list (relative to tool_root). Empty = tool_root/current directory."
                    },
                    "max_entries": {
                        "type": "integer",
                        "description": "Max entries to return. Default: 200. Max: 500."
                    },
                    "include_hidden": {
                        "type": "boolean",
                        "description": "Include dotfiles/directories. Default: false."
                    }
                }
            }
        }
    })
}

pub fn glob_tool_def() -> serde_json::Value {
    json!({
        "type": "function",
        "function": {
            "name": "glob",
            "description": "Find files by name/path pattern (like find -name). \
                            Supports * (single component), ** (any depth), ? (single char). \
                            Examples: '**/*.rs', 'src/*.ts', 'test_*'. \
                            Returns relative paths sorted alphabetically. \
                            PREFER this over exec+find/ls — OS-agnostic and token-efficient.",
            "parameters": {
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern, e.g. '**/*.rs' or 'src/*.ts'"
                    },
                    "dir": {
                        "type": "string",
                        "description": "Subdirectory to search in (relative to tool_root). \
                                        Omit to search all of tool_root."
                    }
                },
                "required": ["pattern"]
            }
        }
    })
}

pub fn done_tool_def(base_url: &str) -> serde_json::Value {
    let acceptance_evidence_required = governor_contract::done_acceptance_evidence_fields();
    let required_args = governor_contract::done_required_args();
    // Gemini's OpenAI-compatible endpoint frequently drops nested function-call
    // arguments as MALFORMED_FUNCTION_CALL, producing an empty turn. Offer a
    // flattened string-row schema there; the parser accepts both forms.
    let evidence_items = if base_url.contains("generativelanguage.googleapis.com") {
        json!({
            "type": "string",
            "description": "One row per completed criterion, formatted as '<criterion reference> | <verification command>'."
        })
    } else {
        json!({
            "type": "object",
            "properties": {
                "criterion": {
                    "type": "string",
                    "description": "Acceptance criterion reference by number and/or text."
                },
                "command": {
                    "type": "string",
                    "description": "Verification command that already succeeded for this criterion."
                }
            },
            "required": acceptance_evidence_required
        })
    };
    json!({
        "type": "function",
        "function": {
            "name": "done",
            "description": "Signal that the task is complete and end the agent loop. \
                            Use only after verifying with commands/tests.",
            "parameters": {
                "type": "object",
                "properties": {
                    "summary": {
                        "type": "string",
                        "description": "Brief [DONE] summary: what was built/changed and where it lives."
                    },
                    "completed_acceptance": {
                        "type": "array",
                        "description": "Current plan acceptance criteria that are already satisfied. Cite by number and/or criterion text.",
                        "items": {
                            "type": "string"
                        }
                    },
                    "remaining_acceptance": {
                        "type": "array",
                        "description": "Current plan acceptance criteria that are still not satisfied. Must cover the rest of the current plan criteria.",
                        "items": {
                            "type": "string"
                        }
                    },
                    "acceptance_evidence": {
                        "type": "array",
                        "description": "For each completed acceptance criterion, cite the real verification command that proved it.",
                        "items": evidence_items
                    },
                    "next_steps": {
                        "type": "string",
                        "description": "How to run/verify, or any follow-up work."
                    }
                },
                "required": required_args
            }
        }
    })
}

pub(crate) fn has_in_path(cmd: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    let exts: Vec<String> = if cfg!(target_os = "windows") {
        let pathext =
            std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".to_string());
        pathext
            .split(';')
            .map(|e| e.trim())
            .filter(|e| !e.is_empty())
            .map(|e| e.to_ascii_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&path) {
        if cfg!(target_os = "windows") {
            for ext in &exts {
                let name = if ext.is_empty() {
                    cmd.to_string()
                } else if cmd.to_ascii_lowercase().ends_with(ext) {
                    cmd.to_string()
                } else {
                    format!("{cmd}{ext}")
                };
                if dir.join(&name).is_file() {
                    return true;
                }
            }
        } else if dir.join(cmd).is_file() {
            return true;
        }
    }
    false
}

pub(crate) fn host_capability_addon() -> String {
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    };

    let bash = has_in_path("bash");
    let sh = has_in_path("sh");
    let shell = if cfg!(target_os = "windows") {
        "powershell"
    } else if bash {
        "bash"
    } else if sh {
        "sh"
    } else {
        "unknown"
    };

    let mut has: Vec<&'static str> = Vec::new();
    if has_in_path("git") {
        has.push("git");
    }
    if has_in_path("rg") {
        has.push("rg");
    }
    if has_in_path("node") {
        has.push("node");
    }
    if has_in_path("python3") {
        has.push("python3");
    } else if has_in_path("python") {
        has.push("python");
    }
    if has_in_path("cargo") {
        has.push("cargo");
    }
    if has_in_path("npm") {
        has.push("npm");
    }

    let has_list = if has.is_empty() {
        "(none detected)".to_string()
    } else {
        has.join(", ")
    };

    format!(
        "[Host Capability Probe]\n\
os: {os}\n\
exec_shell: {shell}\n\
tools_in_path: {has_list}\n\
Rules:\n\
- If os != windows: do NOT use PowerShell syntax.\n\
- Prefer built-in tools (list_dir/search_files/glob) over exec-based discovery.\n\
- Use bash-compatible syntax only when exec_shell=bash."
    )
}

/// Build the full Coder system prompt: base + scratchpad + OS rules + persona + language.
pub fn coder_system(
    persona_prompt: &str,
    lang_instruction: &str,
    realize_preset: Option<RealizePreset>,
) -> String {
    let mut s = CODER_BASE_SYSTEM.to_string();
    s = s.replace(
        "__INSTRUCTION_RESOLVER_SCRATCHPAD_RULE__",
        governor_contract::instruction_resolver_scratchpad_rule_message().as_str(),
    );
    s.push_str(&governor_contract::scratchpad_addon());
    s.push_str("\n\n");
    s.push_str(&host_capability_addon());
    if let Some(addon) = realize_on_demand_addon(realize_preset) {
        s.push_str("\n\n");
        s.push_str(&addon);
    }
    if cfg!(target_os = "windows") {
        s.push_str(WIN_SYSTEM_ADDON);
    }
    if !persona_prompt.is_empty() {
        s.push_str("\n\n");
        s.push_str(persona_prompt);
    }
    if !lang_instruction.is_empty() {
        s.push_str("\n\n");
        s.push_str(lang_instruction);
    }
    s
}
