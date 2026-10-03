(() => {
  "use strict";

  const g = (typeof window !== "undefined") ? window : globalThis;
  const SpiralCoder = g.SpiralCoder || (g.SpiralCoder = {});
  if (SpiralCoder.exec) return;

  function isWindowsHost() {
    try {
      const ho = String((window && window.__SPIRAL_CODER_HOST_OS) || "").trim().toLowerCase();
      if (ho) return ho === "windows";
      return typeof navigator !== "undefined" && /Windows/i.test(String(navigator.userAgent || ""));
    } catch (_) {
      return false;
    }
  }

  function stripShellTranscript(text) {
    const raw = String(text || "").replace(/\r\n/g, "\n");
    const lines = raw.split("\n");
    const promptRe = /^\s*(\$\s+|PS(?: [^>]+)?>\s+|>\s+)/;
    const hasPrompt = lines.some((l) => promptRe.test(l));

    const cutToolNoise = (s) => {
      let x = String(s || "");
      const xl = x.toLowerCase();
      const toks = [
        "assistant to=",
        "to=multi_tool_use.",
        "to=functions.",
        "to=web.run",
        "recipient_name",
        "parameters:",
      ];
      let cut = x.length;
      for (const t of toks) {
        const idx = xl.indexOf(t);
        if (idx !== -1) cut = Math.min(cut, idx);
      }
      if (cut !== x.length) x = x.slice(0, cut).trimEnd();
      // If a tool-call trace leaked into the line, it often leaves stray brackets/braces at the end.
      if (x.indexOf("{") === -1) x = x.replace(/[}\]]+$/g, "").trimEnd();
      return x.trim();
    };

    const out = [];
    for (const l0 of lines) {
      const l = String(l0 || "");
      if (hasPrompt) {
        if (!promptRe.test(l)) continue; // drop output lines
        let x = cutToolNoise(l.replace(promptRe, "").trim());
        if (!x || x === "$") continue;
        out.push(x);
      } else {
        // No explicit prompts found: some models paste command+output without `$`/`PS>`.
        // Trim leading whitespace to make output-line filters robust.
        let x = cutToolNoise(l.replace(/^\s*\$\s+/, "").trim());
        if (!x || x === "$") continue;

        // Heuristic: drop obvious command output lines when the model accidentally pastes them
        // into a code fence that should contain commands only.
        if (/^(stdout:|stderr:)\b/i.test(x)) continue;
        if (/^exit\s*:?\s*-?\d+\b/i.test(x)) continue;
        if (/^(fatal:|error:|warning:|hint:)\b/i.test(x)) continue;
        if (/^(initialized empty git repository|on branch|your branch|changes to be committed:|untracked files:|nothing to commit)\b/i.test(x)) continue;
        if (/^(directory:)\b/i.test(x)) continue;
        // ^ディレクトリ:
        if (/^\u30c7\u30a3\u30ec\u30af\u30c8\u30ea\s*:/i.test(x)) continue;
        if (/^(mode\s+lastwritetime|----\s+-------------)\b/i.test(x)) continue;
        if (/^(modified:|new file:|deleted:)\b/i.test(x)) continue;

        out.push(x);
      }
    }
    return out.join("\n").trim();
  }

  function dangerousCommandReason(cmd) {
    const s0 = String(cmd || "");
    const s = s0.toLowerCase().replace(/\s+/g, " ").trim();
    if (!s) return "";
    if (s.indexOf("git reset --hard") !== -1) return "git reset --hard";
    if (/\bgit\s+clean\b/.test(s) && /\b-[a-z]*f[a-z]*\b/.test(s) && /\b-[a-z]*d[a-z]*\b/.test(s)) {
      if (/\b-[a-z]*x[a-z]*\b/.test(s)) return "git clean -fdx";
      return "git clean -fd";
    }
    if (/\bgit\s+rm\b/.test(s)
      && /\b--cached\b/.test(s)
      && (/(^|\s)-r(\s|$)/.test(s) || /\b--recursive\b/.test(s))
      && /(^|\s)\.(?:[\\/])?(\s|$)/.test(s)
    ) return "git rm --cached -r .";
    if (/\brm\s+-rf\b/.test(s)) return "rm -rf";
    if (s.indexOf("remove-item") !== -1 && s.indexOf("-recurse") !== -1 && s.indexOf("-force") !== -1) return "Remove-Item -Recurse -Force";
    return "";
  }

  function gitRepoHint(stderr) {
    const s = String(stderr || "");
    if (!s) return "";
    if (/adding embedded git repository/i.test(s) || /embedded git repository/i.test(s)) {
      return [
        "HINT: You tried to add a nested git repo (embedded repository).",
        "- Do NOT run `git add .` from the parent repo. Work inside the project directory only.",
        "- Fix: move the project under tool_root (fresh directory) OR remove the nested `.git` folder.",
        "- If you truly need nesting, use a submodule (skip for now in hackathon mode).",
      ].join("\n");
    }
    if (/does not have a commit checked out/i.test(s)) {
      return [
        "HINT: You tried to add a git repo that has no commits (unborn HEAD) as a nested repo/submodule.",
        "- Preferred: avoid nesting. Create the project under tool_root and DO NOT add it to the Spiral-Coder repo.",
        "- Otherwise: commit inside the nested repo first, then add as submodule.",
      ].join("\n");
    }
    return "";
  }

  function splitArgsSimple(s) {
    const src = String(s || "");
    const out = [];
    let cur = "";
    let q = null;
    for (let i = 0; i < src.length; i++) {
      const ch = src[i];
      if (q) {
        if (ch === q) { q = null; continue; }
        if (ch === "\\" && q === "\"" && i + 1 < src.length) { cur += src[i + 1]; i++; continue; }
        cur += ch;
        continue;
      }
      if (ch === "'" || ch === "\"") { q = ch; continue; }
      if (/\s/.test(ch)) {
        if (cur) { out.push(cur); cur = ""; }
        continue;
      }
      cur += ch;
    }
    if (cur) out.push(cur);
    return out;
  }

  function psSingleQuote(s) {
    return "'" + String(s || "").replace(/'/g, "''") + "'";
  }

  function bashToPowerShell(script) {
    const raw = stripShellTranscript(script);
    if (!raw) return "";
    const lines = raw.split("\n");
    const out = [];
    out.push("$ErrorActionPreference = 'Stop'");

    for (const line0 of lines) {
      let line = String(line0 || "").trim();
      if (!line) continue;
      if (line.startsWith("#")) continue;

      // Common model glitch: trailing brace.
      if (line.endsWith("}") && line.indexOf("{") === -1) line = line.slice(0, -1).trim();

      line = line.replace(/&&/g, ";");
      const segs = line.split(";"); // naive but good enough for typical command snippets
      for (const seg0 of segs) {
        const seg = String(seg0 || "").trim();
        if (!seg) continue;

        const toks = splitArgsSimple(seg);
        if (!toks.length) continue;

        const head = toks[0];
        if (head === "mkdir") {
          let parents = false;
          const paths = [];
          for (let i = 1; i < toks.length; i++) {
            const t = toks[i];
            if (t === "-p" || t === "--parents") { parents = true; continue; }
            if (t && t.startsWith("-")) continue;
            paths.push(t);
          }
          if (parents) {
            for (const p of paths) out.push("[System.IO.Directory]::CreateDirectory(" + psSingleQuote(p) + ") | Out-Null");
          } else {
            out.push(seg); // mkdir is an alias in PowerShell
          }
          continue;
        }

        if (head === "touch") {
          const paths = [];
          for (let i = 1; i < toks.length; i++) {
            const t = toks[i];
            if (t && t.startsWith("-")) continue;
            paths.push(t);
          }
          for (const p of paths) {
            const q = psSingleQuote(p);
            out.push("if (-not (Test-Path -LiteralPath " + q + ")) { New-Item -ItemType File -Force -Path " + q + " | Out-Null } else { (Get-Item -LiteralPath " + q + ").LastWriteTime = Get-Date }");
          }
          continue;
        }

        if (head === "rm") {
          let recurse = false;
          const paths = [];
          for (let i = 1; i < toks.length; i++) {
            const t = toks[i];
            if (t && t.startsWith("-")) { if (t.indexOf("r") !== -1 || t.indexOf("R") !== -1) recurse = true; continue; }
            paths.push(t);
          }
          for (const p of paths) {
            const q = psSingleQuote(p);
            out.push("Remove-Item -Force " + (recurse ? "-Recurse " : "") + "-ErrorAction SilentlyContinue -LiteralPath " + q);
          }
          continue;
        }

        if (head === "cp" && toks.length >= 3) {
          const src = psSingleQuote(toks[1]);
          const dst = psSingleQuote(toks[2]);
          out.push("Copy-Item -Force -LiteralPath " + src + " -Destination " + dst);
          continue;
        }

        if (head === "mv" && toks.length >= 3) {
          const src = psSingleQuote(toks[1]);
          const dst = psSingleQuote(toks[2]);
          out.push("Move-Item -Force -LiteralPath " + src + " -Destination " + dst);
          continue;
        }

        if (head === "export" && toks.length >= 2 && toks[1].indexOf("=") !== -1) {
          const idx = toks[1].indexOf("=");
          const k = toks[1].slice(0, idx);
          const v = toks[1].slice(idx + 1);
          if (k) out.push("$env:" + k + " = " + psSingleQuote(v));
          continue;
        }

        out.push(seg);
      }
    }

    return out.join("\n").trim();
  }

  function normalizeExecScript(lang, codeText) {
    const isWin = isWindowsHost();
    const l = String(lang || "").trim().toLowerCase();
    const cleaned = stripShellTranscript(codeText);
    if (!isWin) return cleaned;

    const isBash = /^(bash|sh|shell|zsh|console)$/.test(l);
    const isPwsh = /^(powershell|pwsh|ps1|ps)$/.test(l);
    if (isPwsh) {
      // Windows PowerShell 5.x doesn't support `&&` — treat as bash-ish and split.
      if (cleaned.indexOf("&&") !== -1) return bashToPowerShell(cleaned);
      return cleaned;
    }
    if (isBash) return bashToPowerShell(cleaned);

    // Tool logs often use ```bash``` even on Windows.
    if (/(^|\s)(mkdir\s+-p\b|touch\b|rm\s+-rf\b)/.test(cleaned) || cleaned.indexOf("&&") !== -1) {
      return bashToPowerShell(cleaned);
    }
    return cleaned;
  }

  function scaffoldCommand({ name, files, windows = isWindowsHost() }) {
    if (!name || name === "." || name === ".." || /[\\/\0\r\n]/.test(name)) {
      throw new Error("Scaffold name must be a single directory name");
    }
    const entries = Object.entries(files);
    if (!entries.length || entries.some(([path]) => !/^[A-Za-z0-9_.-]+$/.test(path) || path === "." || path === "..")) {
      throw new Error("Scaffold files must have simple file names");
    }
    if (windows) {
      const commands = [
        "$ErrorActionPreference = 'Stop'",
        `if (Test-Path -LiteralPath ${psSingleQuote(name)}) { throw 'Scaffold directory already exists' }`,
        `New-Item -ItemType Directory -Path ${psSingleQuote(name)} | Out-Null`,
        `Set-Location -LiteralPath ${psSingleQuote(name)}`,
        "New-Item -ItemType Directory -Path 'src','docs' | Out-Null",
        ...entries.map(([path, content]) => `${psSingleQuote(content)} | Set-Content -LiteralPath ${psSingleQuote(path)} -Encoding UTF8 -NoNewline`),
        "git init; if ($LASTEXITCODE -ne 0) { throw 'git init failed' }",
        "git branch -M main; if ($LASTEXITCODE -ne 0) { throw 'git branch failed' }",
        "$n = (git config user.name); if (-not $n) { git config user.name 'Spiral-Coder' }",
        "$e = (git config user.email); if (-not $e) { git config user.email 'spiral-coder@local' }",
        `git add -- ${entries.map(([path]) => psSingleQuote(path)).join(" ")}; if ($LASTEXITCODE -ne 0) { throw 'git add failed' }`,
        "git commit -m 'Initial commit'; if ($LASTEXITCODE -ne 0) { throw 'git commit failed' }",
      ];
      return commands.join("\n");
    }
    const quote = (value) => "'" + String(value).replace(/'/g, "'\"'\"'") + "'";
    return [
      "set -eu",
      `mkdir -- ${quote(name)}`,
      `cd -- ${quote(`./${name}`)}`,
      "mkdir src docs",
      ...entries.map(([path, content]) => `printf '%s' ${quote(content)} > ${quote(path)}`),
      "git init",
      "git branch -M main",
      "git config user.name >/dev/null || git config user.name 'Spiral-Coder'",
      "git config user.email >/dev/null || git config user.email 'spiral-coder@local'",
      `git add -- ${entries.map(([path]) => quote(path)).join(" ")}`,
      "git commit -m 'Initial commit'",
    ].join("\n");
  }

  SpiralCoder.exec = {
    isWindowsHost,
    stripShellTranscript,
    dangerousCommandReason,
    gitRepoHint,
    bashToPowerShell,
    normalizeExecScript,
    scaffoldCommand,
  };
})();
