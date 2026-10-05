#!/usr/bin/env python3
"""Minimal Spiral-Coder-compatible server that avoids running a custom EXE."""

from __future__ import annotations

import argparse
import base64
import difflib
import json
import os
import re
import shlex
import subprocess
import sys
import threading
import time
import traceback
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from urllib import error as urlerror
from urllib import request as urlrequest


def _env(name: str, default: str | None = None) -> str | None:
    """Read canonical configuration first, with legacy OBS_ migration support."""
    if name in os.environ:
        return os.environ[name]
    if name.startswith("SPIRAL_CODER_"):
        return os.environ.get("OBS_" + name[len("SPIRAL_CODER_"):], default)
    return os.environ.get(name, default)


REPO_ROOT = Path(__file__).resolve().parents[1]
WEB_ROOT = REPO_ROOT / "web"
DEFAULT_WORKSPACE_ROOT = (Path.home() / "spiral-coder-work").resolve()
# Default to a safe workspace outside the repo, so local tools never "accidentally"
# create nested repos or delete tracked files under the source tree.
WORKSPACE_ROOT = DEFAULT_WORKSPACE_ROOT
MAX_BODY_BYTES = 2 * 1024 * 1024
ANTHROPIC_VERSION = "2023-06-01"
DIRECT_OPENER = urlrequest.build_opener(urlrequest.ProxyHandler({}))
LOCAL_TOOL_MAX_STEPS = 10
PENDING_LOCK = threading.Lock()
PENDING_EDITS: dict[str, dict[str, Any]] = {}
CHANGE_LOCK = threading.Lock()
CHANGE_LOG: list[dict[str, Any]] = []

# Self-evolving meta layer:
# Prompt overrides live under the workspace root, not the repo, so they can be
# safely modified via approvals without touching Spiral-Coder's source tree.
META_PROMPTS_REL = ".spiral-coder/meta_prompts.json"

LOCAL_TOOL_PROMPT = (
    "You are a coding agent with local workspace tools.\n"
    "When the user asks to create/edit/read files, use tool calls."
    " Do not claim lack of permission. Use tools first, then summarize results.\n"
    "If the user asks to create a repository/project/app/game, you must actually create "
    "folders/files (mkdir/write_file) and then report what was created.\n\n"
    "Git safety: NEVER run `git init` at the workspace root. Always create a project directory "
    "and run git inside it (or use `git -C <dir> ...`). NEVER run `git add .` outside the target repo.\n\n"
    "Use run_command to act locally (git, tests, package managers, winget, etc.).\n"
    "Examples:\n"
    "```spiral-coder-tool\n"
    "{\"name\":\"run_command\",\"arguments\":{\"command\":\"git status --porcelain\"}}\n"
    "{\"name\":\"run_command\",\"arguments\":{\"command\":\"powershell -NoProfile -Command \\\"winget --version\\\"\"}}\n"
    "{\"name\":\"run_command\",\"arguments\":{\"command\":\"bash -lc \\\"ls -la\\\"\"}}\n"
    "```\n\n"
    "If your provider/model does not support tool calling, you MUST emit tool intents "
    "in this exact format so Spiral-Coder can execute them:\n"
    "```spiral-coder-tool\n"
    "{\"name\":\"mkdir\",\"arguments\":{\"path\":\"myproj\"}}\n"
    "{\"name\":\"write_file\",\"arguments\":{\"path\":\"myproj/README.md\",\"content\":\"...\"}}\n"
    "```\n"
    "Only JSON is allowed inside the spiral-coder-tool block.\n\n"
    "Fallback (no tool calling AND you cannot emit spiral-coder-tool JSON):\n"
    "- For files: print a file path on its own line, then a fenced code block. Spiral-Coder will write it.\n"
    "  Example:\n"
    "  src/main.py\n"
    "  ```python\n"
    "  print('hi')\n"
    "  ```\n"
    "- For commands: use a fenced block with language bash/powershell/cmd. Spiral-Coder will convert it to run_command.\n"
    "  Example:\n"
    "  ```powershell\n"
    "  git status --porcelain\n"
    "  ```"
)

LONGRUN_AUTONOMY_PROMPT = (
    "Execution policy: long-run autonomous coding.\n"
    "- Decompose work into modules and execute step-by-step.\n"
    "- Continue working without unnecessary confirmation until done.\n"
    "- Reuse tools repeatedly for inspect/edit/verify loops.\n"
    "- For implementation/scaffolding: batch related mkdir/write_file calls (create multiple files) before summarizing.\n"
    "- After edits, prefer a quick local check via run_command (git status / build / tests) when useful.\n"
    "- If blocked, clearly state blocker and best next action.\n"
)

CODER_IDENTITY_PROMPT = (
    "Identity: You are the Coder.\n"
    "- You are responsible for producing working code changes, not just advice.\n"
    "- Prefer concrete edits, commands, and verification over abstract discussion.\n"
    "- Keep explanations concise and implementation-focused.\n"
    "- For implementation requests, execute filesystem changes instead of only giving steps.\n"
)

OBSERVER_NOVELIST_PROMPT = (
    "Observer persona (novelist): cynical modern novelist (original voice).\n"
    "- Do NOT imitate any specific living author.\n"
    "- Stay in-character. No generic encouragement. No 'as an AI' disclaimers.\n"
    "- Evidence first: you MUST quote or paraphrase >=1 concrete fact from coder_context "
    "(error code, file path, model/provider, tool_root, pending approval id, etc.).\n"
    "- If coder_context contains concrete failures (e.g. 401/403/429/10061), you MUST name them.\n"
    "- Loop breaker: if the coder keeps rewriting README/plan/polish without any concrete action, "
    "call out the loop and force a pivot to implementation (tools, files, commands).\n"
    "- Anti-loop: do NOT repeat your last critique. Only mention NEW findings or still-UNRESOLVED items.\n"
    "  If there is no new signal, output exactly: \"[Observer] No new critique. Loop detected.\" and stop.\n"
    "- Output format:\n"
    "  1) First line: one biting sentence (<= 25 words) about what just happened.\n"
    "  2) Body: 2-6 short paragraphs (1-3 sentences each): narrate + critique with metaphor/irony.\n"
    "  3) Optional: if actionable, append a proposals block exactly as specified.\n"
    "- Length: <= ~1400 characters.\n"
)

def _observer_lang_line(ui_lang: str) -> str:
    l = str(ui_lang or "").strip().lower()
    if l == "fr":
        return "- Language: French.\n- Langue: français.\n"
    if l == "en":
        return "- Language: English.\n"
    return "- Language: Japanese.\n- 言語: 日本語。\n"


def _language_instruction(ui_lang: str) -> str:
    """Hard language constraint for both coder and observer outputs.

    Keep tool JSON blocks and code fences intact; only natural language should follow the UI language.
    """
    l = str(ui_lang or "").strip().lower()
    if l == "fr":
        return (
            "Language: French. Write assistant messages in French.\n"
            "Langue: français. Écris les messages de l'assistant en français.\n"
            "Ne traduis pas les blocs JSON d'outils ni les fences de code.\n"
        )
    if l == "en":
        return (
            "Language: English. Write assistant messages in English.\n"
            "Do NOT translate tool JSON blocks or code fences.\n"
        )
    # Default: Japanese.
    return (
        "Language: Japanese. Write assistant messages in Japanese.\n"
        "言語: 日本語。アシスタントのメッセージは日本語で書いてください。\n"
        "ツールJSONブロックとコードフェンスは翻訳しないでください。\n"
    )


def _observer_persona_prompt(persona: str, *, ui_lang: str = "") -> str:
    p = str(persona or "").strip().lower()
    if p == "novelist":
        return OBSERVER_NOVELIST_PROMPT + _observer_lang_line(ui_lang)
    if p == "cynical":
        return (
            "Observer persona (cynical): direct, critical, unsentimental.\n"
            + _observer_lang_line(ui_lang) +
            "- No pep talk. No basics.\n"
            "- You MUST cite >=1 concrete evidence from coder_context.\n"
            "- Prioritize risks, contradictions, and what will break next.\n"
            "- Anti-loop: only mention NEW findings or still-UNRESOLVED items. Do not repeat yourself.\n"
        )
    if p == "cheerful":
        return (
            "Observer persona (cheerful): energetic but still evidence-driven.\n"
            + _observer_lang_line(ui_lang) +
            "- You MUST cite >=1 concrete evidence from coder_context.\n"
            "- Do not hide risks; make them actionable.\n"
            "- Anti-loop: only mention NEW findings or still-UNRESOLVED items. Do not repeat yourself.\n"
        )
    if p == "thoughtful":
        return (
            "Observer persona (thoughtful): calm, analytical, reflective.\n"
            + _observer_lang_line(ui_lang) +
            "- You MUST cite >=1 concrete evidence from coder_context.\n"
            "- Emphasize trade-offs, unknowns, and verification.\n"
            "- Anti-loop: only mention NEW findings or still-UNRESOLVED items. Do not repeat yourself.\n"
        )
    return ""

LOCAL_TOOLS = [
    {
        "type": "function",
        "function": {
            "name": "list_files",
            "description": "List files/directories under the workspace root using a glob pattern.",
            "parameters": {
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "Glob pattern (default: **/*)"},
                    "max_results": {"type": "integer", "minimum": 1, "maximum": 1000},
                },
                "additionalProperties": False,
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "read_file",
            "description": "Read UTF-8 text from a file path under workspace root.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "max_bytes": {"type": "integer", "minimum": 1, "maximum": 300000},
                },
                "required": ["path"],
                "additionalProperties": False,
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "write_file",
            "description": "Write UTF-8 text to a file path under workspace root. Creates parent dirs by default.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "content": {"type": "string"},
                    "overwrite": {"type": "boolean"},
                    "ensure_parent": {"type": "boolean"},
                },
                "required": ["path", "content"],
                "additionalProperties": False,
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "mkdir",
            "description": "Create a directory path under workspace root.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "parents": {"type": "boolean"},
                },
                "required": ["path"],
                "additionalProperties": False,
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "run_command",
            "description": "Run a terminal command in workspace root and return stdout/stderr.",
            "parameters": {
                "type": "object",
                "properties": {
                    "command": {"type": "string"},
                    "timeout_seconds": {"type": "integer", "minimum": 1, "maximum": 600},
                },
                "required": ["command"],
                "additionalProperties": False,
            },
        },
    },
]

DEFAULT_BASE_URL = {
    "mistral": "https://api.mistral.ai/v1",
    "codestral": "https://codestral.mistral.ai/v1",
    "openai-compatible": "https://api.openai.com/v1",
    # Gemini OpenAI-compat endpoint (Google AI Studio).
    "gemini": "https://generativelanguage.googleapis.com/v1beta/openai",
    "anthropic": "https://api.anthropic.com/v1",
    "mistral-cli": "",
}

DEFAULT_MODEL = {
    "mistral": "mistral-small-latest",
    "codestral": "codestral-latest",
    "openai-compatible": "gpt-4o-mini",
    "gemini": "gemini-2.0-flash",
    "anthropic": "claude-3-5-sonnet-latest",
    "mistral-cli": "mistral-medium-latest",
}


def _env_present(name: str) -> bool:
    v = _env(name, "").strip()
    return bool(v)


def _set_workspace_root(raw_root: str) -> None:
    global WORKSPACE_ROOT
    s = str(raw_root or "").strip()
    if not s:
        return

    p = Path(s).expanduser()
    if not p.is_absolute():
        p = REPO_ROOT / p
    p = p.resolve()
    if not p.exists():
        p.mkdir(parents=True, exist_ok=True)
    if not p.is_dir():
        raise RuntimeError(f"workspace root is not a directory: {p}")
    WORKSPACE_ROOT = p


_VIBE_DOTENV_CACHE: dict[str, str] | None = None


def _load_vibe_dotenv() -> dict[str, str]:
    """Load VIBE CLI dotenv (~/.vibe/.env) if present.

    This lets Spiral-Coder reuse the same keys without requiring global env var exports.
    """
    global _VIBE_DOTENV_CACHE
    if _VIBE_DOTENV_CACHE is not None:
        return _VIBE_DOTENV_CACHE

    out: dict[str, str] = {}
    try:
        p = Path.home() / ".vibe" / ".env"
        if p.exists() and p.is_file():
            raw = p.read_text(encoding="utf-8", errors="replace")
            for line in raw.splitlines():
                s = str(line or "").strip()
                if not s or s.startswith("#"):
                    continue
                if s.lower().startswith("export "):
                    s = s[7:].strip()
                if "=" not in s:
                    continue
                k, v = s.split("=", 1)
                key = k.strip()
                val = v.strip().strip(";").strip()
                if (val.startswith("'") and val.endswith("'")) or (
                    val.startswith('"') and val.endswith('"')
                ):
                    val = val[1:-1]
                if key:
                    out[key] = val
    except Exception:
        out = {}

    _VIBE_DOTENV_CACHE = out
    return out


def _provider_key_from_env(provider: str) -> str:
    vibe_env = _load_vibe_dotenv()
    if provider == "codestral":
        return (
            _env("CODESTRAL_API_KEY", "").strip()
            or _env("MISTRAL_API_KEY", "").strip()
            or str(vibe_env.get("CODESTRAL_API_KEY") or "").strip()
            or str(vibe_env.get("MISTRAL_API_KEY") or "").strip()
            or _env("SPIRAL_CODER_API_KEY", "").strip()
        )
    if provider == "mistral":
        return (
            _env("MISTRAL_API_KEY", "").strip()
            or str(vibe_env.get("MISTRAL_API_KEY") or "").strip()
            or _env("SPIRAL_CODER_API_KEY", "").strip()
        )
    if provider == "openai-compatible":
        return (
            _env("SPIRAL_CODER_API_KEY", "").strip()
            or _env("OPENAI_API_KEY", "").strip()
            or str(vibe_env.get("SPIRAL_CODER_API_KEY", vibe_env.get("OBS_API_KEY")) or "").strip()
            or str(vibe_env.get("OPENAI_API_KEY") or "").strip()
        )
    if provider == "gemini":
        return (
            _env("GEMINI_API_KEY", "").strip()
            or _env("GOOGLE_API_KEY", "").strip()
            or str(vibe_env.get("GEMINI_API_KEY") or "").strip()
            or str(vibe_env.get("GOOGLE_API_KEY") or "").strip()
        )
    if provider == "anthropic":
        return _env("ANTHROPIC_API_KEY", "").strip() or str(
            vibe_env.get("ANTHROPIC_API_KEY") or ""
        ).strip()
    return ""


def _provider_from_req(req: dict[str, Any]) -> str:
    provider = str(req.get("provider") or "mistral").strip()
    if provider in (
        "mistral",
        "codestral",
        "openai-compatible",
        "gemini",
        "anthropic",
        "hf",
        "mistral-cli",
    ):
        return provider
    return "mistral"


def _cot_level(req: dict[str, Any]) -> str:
    v = req.get("cot")
    if v is None:
        return "brief"
    if isinstance(v, bool):
        return "brief" if v else "off"
    s = str(v).strip().lower()
    if s in ("off", "false", "0", "none", "disable", "disabled"):
        return "off"
    if s in ("deep", "pro", "full"):
        return "deep"
    if s in ("structured", "plan", "detailed"):
        return "structured"
    if s in ("brief", "short", "on", "true", "1", "simple"):
        return "brief"
    return "brief"


def _autonomy_level(req: dict[str, Any]) -> str:
    v = req.get("autonomy")
    if v is None:
        return "longrun"
    s = str(v).strip().lower()
    if s in ("off", "none", "false", "0"):
        return "off"
    return "longrun"


def _as_bool(v: Any, default: bool) -> bool:
    if v is None:
        return default
    if isinstance(v, bool):
        return v
    if isinstance(v, (int, float)):
        return bool(v)
    s = str(v).strip().lower()
    if s in ("1", "true", "yes", "on"):
        return True
    if s in ("0", "false", "no", "off"):
        return False
    return default


def _force_tool_use(req: dict[str, Any]) -> bool:
    mode = str(req.get("mode") or "").strip().lower()
    default = mode != "observer"
    return _as_bool(req.get("force_tools"), default)


_MATERIAL_INTENT_RE = re.compile(
    r"(repo|repository|scaffold|bootstrap|init|setup|create|generate|implement|"
    r"install|build|test|run|git|winget|bash|powershell|cmd|command|"
    r"readme|license|requirements|"
    r"depot|dépôt|referentiel|référentiel|projet|dossier|répertoire|repertoire|"
    r"fichier|commande|créer|cree|crée|générer|generer|implément|implementer|exécuter|execute|installer|"
    r"リポ|リポジトリ|雛形|ひな形|フォルダ|ふぉるだ|ディレクトリ|でぃれくとり|プロジェクト|"
    r"実装|ファイル|作成|作ろ|作って|生成|追加|組み込|編集|更新|書いて|書き直して|"
    r"コマンド|実行|インストール|セットアップ|ビルド|テスト|"
    r"自分で|じぶんで|やって|やってみて)",
    re.IGNORECASE,
)
def _wants_material_change(req: dict[str, Any]) -> bool:
    """Heuristic: user intent implies we should actually touch files/commands."""
    s = str(req.get("input") or "").strip()
    if not s:
        return False
    return bool(_MATERIAL_INTENT_RE.search(s))


_COMMAND_INTENT_RE = re.compile(
    r"(\bgit\b|\bwinget\b|\bpip\b|\buv\b|\bnpm\b|\bpnpm\b|\byarn\b|\bcargo\b|"
    r"\bpytest\b|\bmake\b|\bcmake\b|\bbash\b|\bpowershell\b|\bcmd\b|"
    r"command|commande|execute|exécuter|コマンド|実行)",
    re.IGNORECASE,
)
def _wants_command_action(req: dict[str, Any]) -> bool:
    """Heuristic: user intent implies we should run terminal commands (git/winget/bash/etc.)."""
    s = str(req.get("input") or "").strip()
    if not s:
        return False
    return bool(_COMMAND_INTENT_RE.search(s))


def _requires_edit_approval(req: dict[str, Any] | None) -> bool:
    if req is None:
        return False
    env_default = _as_bool(_env("SPIRAL_CODER_REQUIRE_EDIT_APPROVAL"), True)
    return _as_bool(req.get("require_edit_approval"), env_default)


def _requires_command_approval(req: dict[str, Any] | None) -> bool:
    if req is None:
        return False
    env_default = _as_bool(_env("SPIRAL_CODER_REQUIRE_COMMAND_APPROVAL"), True)
    if "require_command_approval" in req:
        return _as_bool(req.get("require_command_approval"), env_default)
    return _requires_edit_approval(req)


def _pending_summary(item: dict[str, Any]) -> dict[str, Any]:
    args = item.get("args") if isinstance(item.get("args"), dict) else {}
    path = str(args.get("path") or "")
    if not path and str(item.get("action") or "") == "run_command":
        path = "<command>"
    preview = str(args.get("content_preview") or args.get("command_preview") or "")
    diff = str(args.get("diff_preview") or "")
    out = {
        "id": item.get("id"),
        "action": item.get("action"),
        "status": item.get("status"),
        "path": path,
        "created_at": item.get("created_at"),
        "updated_at": item.get("updated_at"),
        "preview": preview,
        "diff": diff,
        "error": item.get("error"),
    }
    if item.get("result") is not None:
        out["result"] = item.get("result")
    return out


def _queue_pending_edit(action: str, args: dict[str, Any]) -> dict[str, Any]:
    now = int(time.time())
    eid = "edit_" + uuid.uuid4().hex[:10]
    item = {
        "id": eid,
        "action": action,
        "status": "pending",
        "created_at": now,
        "updated_at": now,
        "args": args,
        "result": None,
        "error": None,
    }
    with PENDING_LOCK:
        PENDING_EDITS[eid] = item
    return item


def _meta_prompts_path() -> Path:
    return (WORKSPACE_ROOT / META_PROMPTS_REL).resolve()


def _default_meta_prompts() -> dict[str, str]:
    return {"coder_system_append": "", "observer_system_append": ""}


def _load_meta_prompts() -> dict[str, str]:
    p = _meta_prompts_path()
    try:
        if not p.exists() or not p.is_file():
            return _default_meta_prompts()
        raw = p.read_text(encoding="utf-8", errors="replace")
        data = json.loads(raw) if raw.strip() else {}
        if not isinstance(data, dict):
            return _default_meta_prompts()
        out = _default_meta_prompts()
        for k in ("coder_system_append", "observer_system_append"):
            v = data.get(k)
            if isinstance(v, str):
                out[k] = v
        return out
    except Exception:
        return _default_meta_prompts()


def _queue_meta_prompts_write(new_prompts: dict[str, str]) -> dict[str, Any]:
    p = _meta_prompts_path()
    # Ensure the target path stays inside the workspace.
    rel = _rel_path(p)
    content = json.dumps(new_prompts, ensure_ascii=False, indent=2) + "\n"
    diff_preview = _diff_preview(p, content)
    pending = _queue_pending_edit(
        "write_file",
        {
            "path": rel,
            "content": content,
            "content_preview": content[:2000],
            "diff_preview": diff_preview,
            "overwrite": True,
            "ensure_parent": True,
        },
    )
    return {
        "needs_approval": True,
        "approval_id": pending["id"],
        "action": "write_file",
        "path": rel,
        "message": "Awaiting approval via /api/approve_edit",
        "prompts": new_prompts,
    }


def _log_change(action: str, payload: dict[str, Any]) -> None:
    now = int(time.time())
    item = {"id": "chg_" + uuid.uuid4().hex[:10], "ts": now, "action": action, **payload}
    with CHANGE_LOCK:
        CHANGE_LOG.append(item)
        if len(CHANGE_LOG) > 200:
            del CHANGE_LOG[:50]


def _list_changes() -> list[dict[str, Any]]:
    with CHANGE_LOCK:
        items = list(CHANGE_LOG)
    items.sort(key=lambda x: int(x.get("ts") or 0), reverse=True)
    return items[:120]


def _list_pending_edits() -> list[dict[str, Any]]:
    with PENDING_LOCK:
        items = list(PENDING_EDITS.values())
    items.sort(key=lambda x: int(x.get("created_at") or 0), reverse=True)
    return [_pending_summary(i) for i in items]


def _approve_pending_edit(edit_id: str) -> dict[str, Any]:
    with PENDING_LOCK:
        item = PENDING_EDITS.get(edit_id)
    if item is None:
        raise RuntimeError("pending edit not found")
    if item.get("status") != "pending":
        return _pending_summary(item)

    args = item.get("args") if isinstance(item.get("args"), dict) else {}
    action = str(item.get("action") or "")
    now = int(time.time())
    try:
        if action == "write_file":
            path = _resolve_workspace_path(str(args.get("path") or ""))
            result = _apply_write_file(
                path=path,
                content=str(args.get("content") or ""),
                overwrite=bool(args.get("overwrite", True)),
                ensure_parent=bool(args.get("ensure_parent", True)),
            )
        elif action == "mkdir":
            path = _resolve_workspace_path(str(args.get("path") or ""))
            result = _apply_mkdir(path=path, parents=bool(args.get("parents", True)))
        elif action == "run_command":
            cwd = WORKSPACE_ROOT
            cwd_raw = str(args.get("cwd") or "").strip()
            if cwd_raw:
                cwd_path = _resolve_workspace_path(cwd_raw, root=WORKSPACE_ROOT)
                if not cwd_path.exists():
                    raise RuntimeError("command cwd does not exist")
                if not cwd_path.is_dir():
                    raise RuntimeError("command cwd is not a directory")
                cwd = cwd_path
            result = _apply_run_command(
                command=str(args.get("command") or ""),
                timeout_seconds=int(args.get("timeout_seconds") or 120),
                cwd=cwd,
            )
        else:
            raise RuntimeError(f"unknown pending action: {action}")

        with PENDING_LOCK:
            cur = PENDING_EDITS.get(edit_id)
            if cur:
                cur["status"] = "approved"
                cur["updated_at"] = now
                cur["result"] = result
                cur["error"] = None
                item = cur
    except Exception as e:
        with PENDING_LOCK:
            cur = PENDING_EDITS.get(edit_id)
            if cur:
                cur["status"] = "error"
                cur["updated_at"] = now
                cur["error"] = str(e)
                item = cur
        raise

    return _pending_summary(item)


def _reject_pending_edit(edit_id: str) -> dict[str, Any]:
    with PENDING_LOCK:
        item = PENDING_EDITS.get(edit_id)
        if item is None:
            raise RuntimeError("pending edit not found")
        if item.get("status") == "pending":
            item["status"] = "rejected"
            item["updated_at"] = int(time.time())
    return _pending_summary(item)


def _base_url(provider: str, req: dict[str, Any]) -> str:
    v = str(req.get("base_url") or "").strip()
    if v:
        return v.rstrip("/")
    return DEFAULT_BASE_URL.get(provider, "https://api.openai.com/v1")


def _pick_model(provider: str, req: dict[str, Any]) -> str:
    mode = str(req.get("mode") or "")
    use_code_model = mode == "VIBE" or mode.startswith("diff")

    candidates: list[str] = []
    if use_code_model:
        candidates.extend(
            [
                str(req.get("code_model") or "").strip(),
                str(req.get("model") or "").strip(),
                str(req.get("chat_model") or "").strip(),
            ]
        )
    else:
        candidates.extend(
            [
                str(req.get("chat_model") or "").strip(),
                str(req.get("model") or "").strip(),
                str(req.get("code_model") or "").strip(),
            ]
        )

    for model in candidates:
        if model:
            return model
    return DEFAULT_MODEL.get(provider, "gpt-4o-mini")


def _pick_api_key(provider: str, req: dict[str, Any]) -> str:
    inline = str(req.get("api_key") or "").strip()
    if inline:
        return inline
    return _provider_key_from_env(provider)


def _preflight_provider_config(provider: str, base_url: str, api_key: str) -> None:
    p = str(provider or "").strip().lower()
    u = str(base_url or "").strip().lower()
    k = str(api_key or "").strip()

    if p in ("mistral", "codestral"):
        if "api.openai.com" in u:
            raise RuntimeError(
                "Provider is mistral/codestral but Base URL is OpenAI. "
                "Use https://api.mistral.ai/v1 or https://codestral.mistral.ai/v1."
            )
        if k.startswith("sk-proj-"):
            raise RuntimeError(
                "This API key looks like an OpenAI key (sk-proj-*). "
                "Use MISTRAL_API_KEY or SPIRAL_CODER_API_KEY for Mistral/Codestral."
            )

    if p == "openai-compatible" and "api.mistral.ai" in u:
        raise RuntimeError(
            "Provider is openai-compatible but Base URL points to Mistral. "
            "Switch provider to mistral/codestral or change Base URL."
        )

    if p == "gemini":
        if "api.openai.com" in u:
            raise RuntimeError(
                "Provider is gemini but Base URL is OpenAI. "
                "Use https://generativelanguage.googleapis.com/v1beta/openai."
            )
        if "generativelanguage.googleapis.com" not in u:
            raise RuntimeError(
                "Provider is gemini but Base URL does not look like Gemini API. "
                "Use https://generativelanguage.googleapis.com/v1beta/openai."
            )
        if "/openai" not in u:
            raise RuntimeError(
                "Gemini provider expects the OpenAI-compat endpoint path '/openai'. "
                "Example: https://generativelanguage.googleapis.com/v1beta/openai"
            )
        if k.startswith("sk-"):
            raise RuntimeError(
                "This API key looks like an OpenAI key (sk-*). Use GEMINI_API_KEY/GOOGLE_API_KEY."
            )


def _extract_text_from_anthropic_response(payload: dict[str, Any]) -> str:
    items = payload.get("content")
    if isinstance(items, list):
        chunks: list[str] = []
        for item in items:
            if not isinstance(item, dict):
                continue
            if item.get("type") == "text":
                chunks.append(str(item.get("text") or ""))
        return "".join(chunks)
    return ""


def _normalize_assistant_content(content: Any) -> str:
    if content is None:
        return ""
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        chunks: list[str] = []
        for item in content:
            if isinstance(item, str):
                chunks.append(item)
                continue
            if not isinstance(item, dict):
                continue
            if item.get("type") in ("output_text", "text"):
                txt = item.get("text")
                if isinstance(txt, str):
                    chunks.append(txt)
        return "".join(chunks)
    return str(content)


_SPIRAL_CODER_TOOL_RE = re.compile(
    r"```spiral-coder-tool\s*\r?\n(.*?)\r?\n```",
    re.IGNORECASE | re.DOTALL,
)


def _extract_spiral_coder_tool_calls(text: str) -> tuple[str, list[dict[str, Any]]]:
    s = str(text or "")
    calls: list[dict[str, Any]] = []

    def collect(obj: Any) -> None:
        if isinstance(obj, list):
            for item in obj:
                collect(item)
            return
        if not isinstance(obj, dict):
            return
        if isinstance(obj.get("calls"), list):
            collect(obj.get("calls"))
            return
        name = obj.get("name") or obj.get("tool") or obj.get("function")
        args = obj.get("arguments")
        if args is None:
            args = obj.get("args")
        if isinstance(name, str) and isinstance(args, dict):
            calls.append({"name": name, "arguments": args})

    def parse_block(block: str) -> None:
        b = str(block or "").strip()
        if not b:
            return
        try:
            payload = json.loads(b)
        except json.JSONDecodeError:
            payload = None
        if payload is not None:
            collect(payload)
            return
        for line in b.splitlines():
            ln = line.strip()
            if not ln:
                continue
            try:
                item = json.loads(ln)
            except json.JSONDecodeError:
                continue
            collect(item)

    for m in _SPIRAL_CODER_TOOL_RE.finditer(s):
        parse_block(m.group(1))

    cleaned = _SPIRAL_CODER_TOOL_RE.sub("", s).strip()
    return cleaned, calls


_FILE_LINE_RE = re.compile(
    r"^\s*(?:[-*]\s*)?(?:#{1,6}\s*)?(?:file|path|filename)?\s*[:\-]?\s*[*_]{0,2}`?"
    r"([A-Za-z]:\\[^\s`]+|(?:\.\.?/)?[A-Za-z0-9._-]+(?:/[A-Za-z0-9._-]+)*\.[A-Za-z0-9]{1,8})"
    r"`?[*_]{0,2}\s*(?:[:：])?\s*$",
    re.IGNORECASE,
)


def _extract_implied_write_file_calls(text: str, *, max_files: int = 12) -> list[dict[str, Any]]:
    """Extract file writes from common LLM output patterns:

    src/main.py
    ```python
    ...
    ```
    """
    s = str(text or "")
    if not s.strip():
        return []
    lines = s.splitlines()
    calls: list[dict[str, Any]] = []
    i = 0
    while i < len(lines) and len(calls) < max_files:
        m = _FILE_LINE_RE.match(lines[i] or "")
        if not m:
            i += 1
            continue
        raw_path = str(m.group(1) or "").strip()
        if not raw_path:
            i += 1
            continue

        j = i + 1
        while j < len(lines) and not str(lines[j] or "").strip():
            j += 1
        if j >= len(lines):
            break
        fence = str(lines[j] or "")
        if not fence.lstrip().startswith("```"):
            i += 1
            continue

        k = j + 1
        buf: list[str] = []
        while k < len(lines):
            if str(lines[k] or "").strip().startswith("```"):
                break
            buf.append(str(lines[k] or ""))
            k += 1
        if k >= len(lines):
            break

        content = "\n".join(buf).rstrip() + "\n"
        if content.strip():
            calls.append(
                {
                    "name": "write_file",
                    "arguments": {
                        "path": raw_path.replace("\\", "/"),
                        "content": content,
                    },
                }
            )
        i = k + 1
    return calls


_FENCE_START_RE = re.compile(r"^\s*```([A-Za-z0-9_-]+)?\s*$")
_PS_PROMPT_RE = re.compile(r"^\s*PS [^>]*>\s*")
_SHELL_PROMPT_RE = re.compile(r"^\s*(\$\s+|PS(?: [^>]+)?>\s+|>\s+)")
_ALLOWED_COMMAND_FENCES = {
    "bash",
    "sh",
    "shell",
    "powershell",
    "pwsh",
    "ps1",
    "cmd",
    "bat",
}

def _bash_script_to_powershell_script(script: str) -> str:
    """Translate a tiny subset of common bash scaffolding commands to PowerShell.

    This is intentionally conservative: it's meant to turn typical LLM output like
    mkdir/cd/touch into something runnable on Windows.
    """
    s = str(script or "")
    out_lines: list[str] = []
    for raw in s.splitlines():
        line = str(raw or "").strip()
        if not line:
            continue
        if line.startswith("#"):
            out_lines.append(line)
            continue
        if line.startswith("sudo "):
            line = line[5:].strip()

        if line.startswith("mkdir "):
            try:
                toks = shlex.split(line, posix=True)
            except ValueError:
                toks = line.split()
            # Drop -p/--parents when present.
            toks2 = [t for t in toks if t not in ("-p", "--parents")]
            if len(toks2) >= 2:
                out_lines.append("mkdir " + " ".join(toks2[1:]))
                continue

        if line.startswith("cd "):
            out_lines.append("cd " + line[3:].strip())
            continue

        if line.startswith("touch "):
            try:
                toks = shlex.split(line, posix=True)
            except ValueError:
                toks = line.split()
            paths = toks[1:]
            if paths:
                arr = ",".join("'" + p.replace("'", "''") + "'" for p in paths)
                out_lines.append(f"New-Item -ItemType File -Force -Path {arr} | Out-Null")
                continue

        if line.startswith("rm "):
            try:
                toks = shlex.split(line, posix=True)
            except ValueError:
                toks = line.split()
            paths = [t for t in toks[1:] if not t.startswith("-")]
            if paths:
                arr = ",".join("'" + p.replace("'", "''") + "'" for p in paths)
                out_lines.append(f"Remove-Item -Recurse -Force -Path {arr}")
                continue

        out_lines.append(line)

    return "\n".join(out_lines).strip()


def _strip_shell_prompt_line(line: str) -> str:
    s = str(line or "").rstrip("\r\n")
    ls = s.lstrip()
    if ls.startswith("$ "):
        return ls[2:]
    if ls.startswith("> "):
        return ls[2:]
    m = _PS_PROMPT_RE.match(ls)
    if m:
        return ls[m.end() :]
    return s


def _strip_shell_transcript(text: str) -> str:
    """Strip prompts from common shell transcripts.

    If the block contains prompts, we keep only prompt lines (drop output like stderr/exit/status).
    """
    raw = str(text or "").replace("\r\n", "\n")
    lines = raw.split("\n")
    has_prompt = any(_SHELL_PROMPT_RE.match(str(l or "")) for l in lines)
    out: list[str] = []
    for l0 in lines:
        l = str(l0 or "")
        if has_prompt:
            m = _SHELL_PROMPT_RE.match(l)
            if not m:
                continue
            out.append(l[m.end() :])
        else:
            out.append(re.sub(r"^\s*\$\s+", "", l))
    return "\n".join(out).strip()


def _sanitize_shellish_command(cmd: str) -> str:
    """Best-effort cleanup for LLM-produced shell transcripts.

    Common failure modes:
    - Prompt markers accidentally included in the command: `$ ...`, `PS> ...`, `> ...`
    - Stray trailing braces when a model mixes tool-call syntax into a shell line.

    This runs on both "tool-call" commands and implied command extraction.
    """
    raw = str(cmd or "").replace("\r\n", "\n").strip()
    if not raw:
        return ""

    out: list[str] = []
    for ln0 in raw.split("\n"):
        ln = str(ln0 or "").rstrip("\r\n")
        m = _SHELL_PROMPT_RE.match(ln)
        if m:
            ln = ln[m.end() :]
        ln = re.sub(r"^\s*\$\s+", "", ln)
        out.append(ln)

    s = "\n".join(out).strip()

    # Strip a leading "$" without a following space: "$git status"
    s = re.sub(r"^\s*\$(?=\S)", "", s).lstrip()

    # Strip trailing unmatched "}" (common artifact).
    if s.endswith("}") and s.count("{") < s.count("}"):
        while s.endswith("}") and s.count("{") < s.count("}"):
            s = s[:-1].rstrip()

    return s


_POISON_PROXY_RE = re.compile(r"^https?://(?:127\.0\.0\.1|localhost):9(?:/|$)", re.IGNORECASE)


def _clean_subprocess_env() -> dict[str, str]:
    """Environment for local subprocesses.

    Some environments poison proxy vars (e.g. http://127.0.0.1:9), which breaks `git push` and
    other networked tools by forcing traffic into a dead local proxy. We strip only that known-bad case.
    """
    env = dict(os.environ)
    for k in ("HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy"):
        v = str(env.get(k) or "").strip()
        if v and _POISON_PROXY_RE.match(v):
            env.pop(k, None)
    return env


def _script_to_local_command(lang: str, script: str) -> str:
    l = str(lang or "").strip().lower()
    s = str(script or "").strip()
    if not s:
        return ""
    if len(s) > 12000:
        s = s[:12000] + "\n"

    if l in ("powershell", "pwsh", "ps1"):
        # Avoid quoting/encoding headaches: UTF-16LE base64 for -EncodedCommand.
        enc = base64.b64encode(s.encode("utf-16le")).decode("ascii")
        return f"powershell -NoProfile -ExecutionPolicy Bypass -EncodedCommand {enc}"

    if l in ("bash", "sh", "shell"):
        if os.name == "nt":
            ps = _bash_script_to_powershell_script(s)
            if ps:
                enc = base64.b64encode(ps.encode("utf-16le")).decode("ascii")
                return f"powershell -NoProfile -ExecutionPolicy Bypass -EncodedCommand {enc}"
        # Non-Windows: use bash -lc for multi-line scripts.
        one = "; ".join([ln.strip() for ln in s.splitlines() if ln.strip()])
        one = one.replace('"', '\\"')
        return f'bash -lc \"set -e; {one}\"'

    if l in ("cmd", "bat"):
        parts = [ln.strip() for ln in s.splitlines() if ln.strip()]
        return " && ".join(parts)

    return s


def _extract_implied_run_command_calls(text: str, *, max_commands: int = 6) -> list[dict[str, Any]]:
    """Extract commands from fenced blocks like ```powershell / ```bash / ```cmd.

    Safety: we skip fences that look like file contents (a file path line directly above).
    """
    s = str(text or "")
    if not s.strip():
        return []
    lines = s.splitlines()
    calls: list[dict[str, Any]] = []
    i = 0
    while i < len(lines) and len(calls) < max_commands:
        m = _FENCE_START_RE.match(lines[i] or "")
        if not m:
            i += 1
            continue
        lang = str(m.group(1) or "").strip().lower()
        if lang == "spiral-coder-tool":
            # Tool blocks are handled elsewhere.
            i += 1
            while i < len(lines) and not str(lines[i] or "").strip().startswith("```"):
                i += 1
            i += 1
            continue

        j = i + 1
        buf: list[str] = []
        while j < len(lines):
            if str(lines[j] or "").strip().startswith("```"):
                break
            buf.append(str(lines[j] or ""))
            j += 1
        if j >= len(lines):
            break

        prev = i - 1
        while prev >= 0 and not str(lines[prev] or "").strip():
            prev -= 1
        if prev >= 0 and _FILE_LINE_RE.match(lines[prev] or ""):
            # It's a file content fence, not a command.
            i = j + 1
            continue

        if lang in _ALLOWED_COMMAND_FENCES:
            cleaned = _strip_shell_transcript("\n".join(str(ln or "") for ln in buf))
            cmd = _script_to_local_command(lang, cleaned)
            if cmd.strip():
                calls.append({"name": "run_command", "arguments": {"command": cmd}})

        i = j + 1
    return calls


def _swap_max_tokens_to_max_completion_tokens(payload: dict[str, Any]) -> dict[str, Any]:
    """Some OpenAI-compatible models reject max_tokens in favor of max_completion_tokens."""
    if "max_tokens" not in payload:
        return payload
    if payload.get("max_tokens") is None:
        return payload
    if "max_completion_tokens" in payload:
        return payload
    out = dict(payload)
    out["max_completion_tokens"] = out.pop("max_tokens")
    return out


def _should_use_max_completion_tokens(error_text: str) -> bool:
    msg = str(error_text or "")
    msg_lower = msg.lower()
    return "max_completion_tokens" in msg_lower and "max_tokens" in msg_lower


def _should_fallback_to_completions(error_text: str) -> bool:
    msg = str(error_text or "")
    msg_lower = msg.lower()
    return ("not a chat model" in msg_lower) or ("v1/completions" in msg_lower and "chat/completions" in msg_lower)


def _messages_to_prompt(messages: list[dict[str, Any]]) -> str:
    parts: list[str] = []
    for m in messages:
        if not isinstance(m, dict):
            continue
        role = str(m.get("role") or "").strip().upper() or "USER"
        content = _normalize_assistant_content(m.get("content"))
        if not str(content).strip():
            continue
        parts.append(f"{role}:\n{content.strip()}")
    parts.append("ASSISTANT:\n")
    return "\n\n".join(parts)


def _extract_text_from_openai_completions_response(res: dict[str, Any]) -> str:
    if not isinstance(res, dict):
        return str(res)
    choices = res.get("choices")
    if isinstance(choices, list) and choices:
        ch0 = choices[0]
        if isinstance(ch0, dict):
            txt = ch0.get("text")
            if isinstance(txt, str) and txt.strip():
                return txt
            msg = ch0.get("message")
            if isinstance(msg, dict):
                return _normalize_assistant_content(msg.get("content"))
    return _normalize_assistant_content(res.get("content"))


def _api_key_env_hint(provider: str) -> str:
    p = str(provider or "").strip().lower()
    if p == "openai-compatible":
        return "SPIRAL_CODER_API_KEY or OPENAI_API_KEY"
    if p == "gemini":
        return "GEMINI_API_KEY or GOOGLE_API_KEY"
    if p == "mistral":
        return "MISTRAL_API_KEY or SPIRAL_CODER_API_KEY"
    if p == "codestral":
        return "CODESTRAL_API_KEY or MISTRAL_API_KEY or SPIRAL_CODER_API_KEY"
    if p == "anthropic":
        return "ANTHROPIC_API_KEY"
    if p == "mistral-cli":
        return "run `vibe --setup` (Mistral CLI auth)"
    return "provider-specific API key env var"


def _model_suggestions(provider: str) -> list[str]:
    p = str(provider or "").strip().lower()
    if p == "openai-compatible":
        return ["gpt-4o-mini", "gpt-4.1-mini", "o4-mini"]
    if p == "gemini":
        return ["gemini-2.0-flash", "gemini-2.5-flash", "gemini-3-flash-preview"]
    if p == "codestral":
        return ["codestral-latest", "mistral-small-latest"]
    if p == "mistral":
        return ["mistral-small-latest", "codestral-latest", "mistral-large-latest"]
    if p == "anthropic":
        return ["claude-3-5-sonnet-latest", "claude-3-5-haiku-latest"]
    if p == "mistral-cli":
        return ["mistral-medium-latest", "mistral-small-latest"]
    return []


def _extract_provider_error_message(body: str) -> str:
    txt = str(body or "").strip()
    if not txt:
        return ""
    try:
        j = json.loads(txt)
    except Exception:
        return txt

    if isinstance(j, dict):
        err = j.get("error")
        if isinstance(err, dict):
            msg = err.get("message")
            if isinstance(msg, str) and msg.strip():
                return msg.strip()
            for k in ("type", "code"):
                v = err.get(k)
                if isinstance(v, str) and v.strip():
                    return v.strip()
        if isinstance(err, str) and err.strip():
            return err.strip()
        for k in ("message", "detail"):
            v = j.get(k)
            if isinstance(v, str) and v.strip():
                return v.strip()

    return txt


def _friendly_http_error(
    status: int, body: str, headers: Any, error_context: dict[str, Any] | None
) -> RuntimeError:
    ctx = error_context or {}
    provider = str(ctx.get("provider") or "").strip()
    model = str(ctx.get("model") or "").strip()

    msg = _extract_provider_error_message(body)
    msg_lower = msg.lower()
    lines: list[str] = []

    if status in (401, 403):
        lines.append(f"HTTP {status}: authentication failed.")
        lines.append(f"Set API key env var: {_api_key_env_hint(provider)}")
    elif status == 429:
        lines.append("HTTP 429: rate limited.")
        retry_after = ""
        if headers is not None:
            try:
                retry_after = str(headers.get("Retry-After") or "").strip()
            except Exception:
                retry_after = ""
        if retry_after:
            lines.append(f"retry_after: {retry_after}s")
        else:
            lines.append("retry_after: not provided")
    elif status == 400 and (
        "invalid model" in msg_lower
        or "invalid_model" in msg_lower
        or "model_not_found" in msg_lower
    ):
        lines.append("HTTP 400: invalid_model")
        if model:
            lines.append(f"requested_model: {model}")
        sugg = _model_suggestions(provider)
        if sugg:
            lines.append("fallback_candidates: " + ", ".join(sugg))
    else:
        lines.append(f"HTTP {status}")

    if msg:
        lines.append(msg if len(msg) <= 1200 else (msg[:1200] + "..."))

    return RuntimeError("\n".join(lines))


def _http_json(
    method: str,
    url: str,
    *,
    headers: dict[str, str] | None = None,
    body: dict[str, Any] | None = None,
    timeout_seconds: int = 120,
    error_context: dict[str, Any] | None = None,
) -> dict[str, Any]:
    req_headers = dict(headers or {})
    data: bytes | None = None
    if body is not None:
        data = json.dumps(body, ensure_ascii=False).encode("utf-8")
        req_headers.setdefault("Content-Type", "application/json")

    # urllib/http.client encodes headers as latin-1. Catch misconfigured keys early (common with pasted non-ASCII).
    for hk, hv in req_headers.items():
        try:
            str(hk).encode("ascii")
            str(hv).encode("latin-1")
        except UnicodeEncodeError:
            raise RuntimeError(
                f"request header is not latin-1 encodable: {hk}. "
                "Check API key/settings (no non-ASCII characters)."
            ) from None

    req = urlrequest.Request(url=url, data=data, headers=req_headers, method=method)
    try:
        # Force direct egress and ignore process/global proxy variables.
        with DIRECT_OPENER.open(req, timeout=timeout_seconds) as resp:
            raw = resp.read()
    except urlerror.HTTPError as e:
        raw = e.read() if e.fp is not None else b""
        text = raw.decode("utf-8", errors="replace")
        raise _friendly_http_error(int(e.code), text, e.headers, error_context) from None
    except urlerror.URLError as e:
        raise RuntimeError(f"request failed: {e.reason}") from None

    if not raw:
        return {}
    try:
        return json.loads(raw.decode("utf-8"))
    except json.JSONDecodeError:
        raise RuntimeError("invalid JSON response") from None


def _tool_root(req: dict[str, Any] | None) -> Path:
    if req is None:
        return WORKSPACE_ROOT
    raw = str(req.get("tool_root") or "").strip()
    if not raw:
        return WORKSPACE_ROOT

    p = Path(raw).expanduser()
    if not p.is_absolute():
        p = WORKSPACE_ROOT / p
    p = p.resolve()

    try:
        p.relative_to(WORKSPACE_ROOT)
    except ValueError:
        raise RuntimeError("tool_root escapes workspace root") from None

    # Allow non-existent tool_root so the agent can create a new project folder.
    # We still prevent pointing at an existing non-directory.
    if p.exists() and not p.is_dir():
        raise RuntimeError("tool_root is not a directory")
    return p


def _resolve_workspace_path(raw_path: str, root: Path | None = None) -> Path:
    s = str(raw_path or "").strip()
    if not s:
        raise RuntimeError("path is required")

    base = (root or WORKSPACE_ROOT).resolve()
    p = Path(s).expanduser()
    if not p.is_absolute():
        p = base / p
    p = p.resolve()

    try:
        p.relative_to(base)
    except ValueError:
        raise RuntimeError("path escapes tool root") from None

    try:
        p.relative_to(WORKSPACE_ROOT)
    except ValueError:
        raise RuntimeError("path escapes workspace root") from None

    return p


def _rel_path(p: Path) -> str:
    return p.resolve().relative_to(WORKSPACE_ROOT).as_posix()


def _tool_list_files(args: dict[str, Any], req: dict[str, Any] | None = None) -> dict[str, Any]:
    pattern = str(args.get("pattern") or "**/*").strip() or "**/*"
    max_results = int(args.get("max_results") or 200)
    max_results = min(1000, max(1, max_results))

    root = _tool_root(req)
    items: list[dict[str, Any]] = []
    for p in root.glob(pattern):
        if len(items) >= max_results:
            break
        try:
            rel = p.resolve().relative_to(WORKSPACE_ROOT).as_posix()
        except ValueError:
            continue
        if rel.startswith(".git/") or "/.git/" in rel:
            continue
        if p.is_dir():
            items.append({"path": rel, "type": "dir"})
        else:
            size = p.stat().st_size if p.exists() else 0
            items.append({"path": rel, "type": "file", "size": size})

    return {
        "workspace_root": WORKSPACE_ROOT.as_posix(),
        "tool_root": root.relative_to(WORKSPACE_ROOT).as_posix()
        if root != WORKSPACE_ROOT
        else "",
        "pattern": pattern,
        "items": items,
        "truncated": len(items) >= max_results,
    }


def _tool_read_file(args: dict[str, Any], req: dict[str, Any] | None = None) -> dict[str, Any]:
    root = _tool_root(req)
    path = _resolve_workspace_path(str(args.get("path") or ""), root=root)
    if not path.exists():
        raise RuntimeError("file does not exist")
    if not path.is_file():
        raise RuntimeError("path is not a file")

    max_bytes = int(args.get("max_bytes") or 120000)
    max_bytes = min(300000, max(1, max_bytes))

    raw = path.read_bytes()
    data = raw[:max_bytes].decode("utf-8", errors="replace")
    return {
        "path": _rel_path(path),
        "bytes": len(raw),
        "truncated": len(raw) > max_bytes,
        "content": data,
    }


def _apply_write_file(
    path: Path, content: str, overwrite: bool, ensure_parent: bool
) -> dict[str, Any]:
    if path.exists() and path.is_dir():
        raise RuntimeError("path is a directory")
    if path.exists() and not overwrite:
        raise RuntimeError("file exists and overwrite=false")
    if ensure_parent:
        path.parent.mkdir(parents=True, exist_ok=True)

    existed = path.exists()
    written = path.write_bytes(content.encode("utf-8"))
    out = {
        "path": _rel_path(path),
        "bytes_written": written,
        "created": not existed,
        "workspace_root": WORKSPACE_ROOT.as_posix(),
    }
    _log_change("write_file", {"path": out["path"], "created": out["created"], "bytes": out["bytes_written"]})
    return out


def _diff_preview(path: Path, new_content: str) -> str:
    rel = _rel_path(path)
    old = ""
    try:
        if path.exists() and path.is_file():
            raw = path.read_bytes()
            old = raw[:250000].decode("utf-8", errors="replace")
    except Exception:
        old = ""

    old_lines = old.splitlines(keepends=True)
    new_lines = str(new_content or "").splitlines(keepends=True)
    if old_lines == new_lines:
        return ""

    diff_lines = list(
        difflib.unified_diff(
            old_lines,
            new_lines,
            fromfile=rel,
            tofile=rel,
            lineterm="",
        )
    )
    if not diff_lines:
        return ""
    diff = "\n".join(diff_lines)
    if len(diff) > 20000:
        diff = diff[:20000] + "\n...truncated..."
    return diff


def _apply_mkdir(path: Path, parents: bool) -> dict[str, Any]:
    path.mkdir(parents=parents, exist_ok=True)
    out = {"path": _rel_path(path), "workspace_root": WORKSPACE_ROOT.as_posix()}
    _log_change("mkdir", {"path": out["path"]})
    return out


def _apply_run_command(command: str, timeout_seconds: int, *, cwd: Path | None = None) -> dict[str, Any]:
    cmd = _sanitize_shellish_command(command)
    if not cmd:
        raise RuntimeError("command is required")

    s = re.sub(r"\s+", " ", cmd.strip().lower())
    if "git reset --hard" in s:
        raise RuntimeError("refusing to run dangerous command via Spiral-Coder: git reset --hard")
    if re.search(r"\bgit\s+clean\b", s) and re.search(r"\b-[a-z]*f[a-z]*\b", s) and re.search(r"\b-[a-z]*d[a-z]*\b", s):
        raise RuntimeError("refusing to run dangerous command via Spiral-Coder: git clean -fd")
    if re.search(r"\bgit\s+rm\b", s) and re.search(r"\b(--cached|-r|--recursive)\b", s) and re.search(r"(^|\\s)\\.(\\s|$)", s):
        raise RuntimeError("refusing to run dangerous command via Spiral-Coder: git rm ... .")

    timeout = min(600, max(1, int(timeout_seconds or 120)))
    run_cwd = (cwd or WORKSPACE_ROOT).resolve()
    if run_cwd == WORKSPACE_ROOT and s.startswith("git init"):
        rest = s[len("git init") :].strip()
        # Block `git init` (or `git init -b main`) at workspace root.
        # Allow `git init <dir>` which initializes a repo in a subdirectory.
        path_tok = ""
        if rest:
            for tok in rest.split(" "):
                if not tok or tok.startswith("-"):
                    continue
                path_tok = tok
                break
        if (not path_tok) or path_tok in (".", "./", ".\\"):
            raise RuntimeError("refusing to run git init at workspace root; create a project directory first")
    try:
        cp = subprocess.run(
            cmd,
            cwd=str(run_cwd),
            shell=True,
            capture_output=True,
            text=True,
            timeout=timeout,
            env=_clean_subprocess_env(),
            encoding="utf-8",
            errors="replace",
        )
    except subprocess.TimeoutExpired:
        raise RuntimeError(f"command timed out after {timeout}s") from None

    stdout = str(cp.stdout or "")
    stderr = str(cp.stderr or "")
    if len(stdout) > 120000:
        stdout = stdout[:120000] + "\n...truncated..."
    if len(stderr) > 120000:
        stderr = stderr[:120000] + "\n...truncated..."

    out = {
        "command": cmd,
        "cwd": run_cwd.as_posix(),
        "exit_code": int(cp.returncode),
        "ok": cp.returncode == 0,
        "stdout": stdout,
        "stderr": stderr,
    }
    _log_change(
        "run_command",
        {
            "cwd": out["cwd"],
            "command": out["command"],
            "ok": out["ok"],
            "exit_code": out["exit_code"],
        },
    )
    return out


def _exec_impl(req: dict[str, Any]) -> dict[str, Any]:
    cmd = str(req.get("command") or "").strip()
    if not cmd:
        raise RuntimeError("command is required")

    cwd = WORKSPACE_ROOT
    cwd_raw = str(req.get("cwd") or "").strip()
    if cwd_raw:
        cwd = _resolve_workspace_path(cwd_raw, root=WORKSPACE_ROOT)
    if cwd.exists() and not cwd.is_dir():
        raise RuntimeError("cwd is not a directory")
    cwd.mkdir(parents=True, exist_ok=True)

    timeout = min(600, max(1, int(req.get("timeout_seconds") or 120)))
    if sys.platform.startswith("win"):
        prelude = "[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; $OutputEncoding=[System.Text.Encoding]::UTF8; "
        cp = subprocess.run(
            ["powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", prelude + cmd],
            cwd=str(cwd),
            capture_output=True,
            text=True,
            timeout=timeout,
            env=_clean_subprocess_env(),
            encoding="utf-8",
            errors="replace",
        )
    else:
        cp = subprocess.run(
            cmd,
            cwd=str(cwd),
            shell=True,
            capture_output=True,
            text=True,
            timeout=timeout,
            env=_clean_subprocess_env(),
            encoding="utf-8",
            errors="replace",
        )

    out = {
        "command": cmd,
        "cwd": cwd.as_posix(),
        "exit_code": int(cp.returncode),
        "stdout": str(cp.stdout or ""),
        "stderr": str(cp.stderr or ""),
    }
    _log_change("exec", {"cwd": out["cwd"], "command": out["command"], "exit_code": out["exit_code"]})
    return out


def _tool_write_file(args: dict[str, Any], req: dict[str, Any] | None = None) -> dict[str, Any]:
    root = _tool_root(req)
    path = _resolve_workspace_path(str(args.get("path") or ""), root=root)
    content = str(args.get("content") or "")
    overwrite = bool(args.get("overwrite", True))
    ensure_parent = bool(args.get("ensure_parent", True))

    if _requires_edit_approval(req):
        diff_preview = _diff_preview(path, content)
        pending = _queue_pending_edit(
            "write_file",
            {
                "path": _rel_path(path),
                "content": content,
                "content_preview": content[:2000],
                "diff_preview": diff_preview,
                "overwrite": overwrite,
                "ensure_parent": ensure_parent,
            },
        )
        return {
            "needs_approval": True,
            "approval_id": pending["id"],
            "action": "write_file",
            "path": _rel_path(path),
            "message": "Awaiting approval via /api/approve_edit",
        }

    return _apply_write_file(path, content, overwrite, ensure_parent)


def _tool_mkdir(args: dict[str, Any], req: dict[str, Any] | None = None) -> dict[str, Any]:
    root = _tool_root(req)
    path = _resolve_workspace_path(str(args.get("path") or ""), root=root)
    parents = bool(args.get("parents", True))
    if _requires_edit_approval(req):
        pending = _queue_pending_edit(
            "mkdir",
            {
                "path": _rel_path(path),
                "parents": parents,
            },
        )
        return {
            "needs_approval": True,
            "approval_id": pending["id"],
            "action": "mkdir",
            "path": _rel_path(path),
            "message": "Awaiting approval via /api/approve_edit",
        }
    return _apply_mkdir(path, parents)


def _tool_run_command(args: dict[str, Any], req: dict[str, Any] | None = None) -> dict[str, Any]:
    command = _sanitize_shellish_command(str(args.get("command") or ""))
    if not command:
        raise RuntimeError("command is required")
    timeout_seconds = int(args.get("timeout_seconds") or 120)
    cwd = _tool_root(req)
    if _requires_command_approval(req):
        pending = _queue_pending_edit(
            "run_command",
            {
                "command": command,
                "command_preview": command[:2000],
                "timeout_seconds": timeout_seconds,
                "cwd": _rel_path(cwd),
            },
        )
        return {
            "needs_approval": True,
            "approval_id": pending["id"],
            "action": "run_command",
            "path": "",
            "message": "Awaiting approval via /api/approve_edit",
        }
    return _apply_run_command(command=command, timeout_seconds=timeout_seconds, cwd=cwd)


def _tool_dispatch(
    name: str, args_json: Any, req: dict[str, Any] | None = None
) -> dict[str, Any]:
    args: dict[str, Any] = {}
    if isinstance(args_json, str) and args_json.strip():
        try:
            parsed = json.loads(args_json)
        except json.JSONDecodeError:
            raise RuntimeError("tool arguments are not valid JSON") from None
        if not isinstance(parsed, dict):
            raise RuntimeError("tool arguments must be a JSON object")
        args = parsed
    elif isinstance(args_json, dict):
        args = args_json

    if name == "list_files":
        return _tool_list_files(args, req=req)
    if name == "read_file":
        return _tool_read_file(args, req=req)
    if name == "write_file":
        return _tool_write_file(args, req=req)
    if name == "mkdir":
        return _tool_mkdir(args, req=req)
    if name == "run_command":
        return _tool_run_command(args, req=req)
    raise RuntimeError(f"unknown tool: {name}")


def _local_tools_enabled(req: dict[str, Any]) -> bool:
    flag = str(_env("SPIRAL_CODER_ENABLE_LOCAL_TOOLS", "1")).strip().lower()
    if flag in ("0", "false", "off", "no"):
        return False

    if "local_tools" in req and isinstance(req["local_tools"], bool):
        return bool(req["local_tools"])

    mode = str(req.get("mode") or "").strip().lower()
    if mode == "observer":
        # Observer pane disables tools in UI, but "mode" can be user-controlled.
        # If the caller explicitly forces tools, allow them.
        if _as_bool(req.get("force_tools"), False):
            return True
        return False
    return True


def _chat_paths_for_provider(provider: str, base_url: str) -> list[str]:
    p = str(provider or "").strip().lower()
    base = str(base_url or "").strip().lower()
    if p == "codestral" or "codestral.mistral.ai" in base:
        return ["/chat/completion", "/chat/completions"]
    return ["/chat/completions"]


def _chat_openai_compat(
    *,
    req: dict[str, Any],
    provider: str,
    base_url: str,
    model: str,
    api_key: str,
    timeout: int,
    temperature: Any,
    max_tokens: Any,
    messages: list[dict[str, Any]],
    tools_enabled: bool,
    max_tool_steps: int,
) -> dict[str, Any]:
    headers = {"Accept": "application/json"}
    if api_key:
        headers["Authorization"] = f"Bearer {api_key}"

    msg_list = [dict(m) for m in messages]
    if tools_enabled:
        root = _tool_root(req)
        tool_root = ""
        try:
            tool_root = root.relative_to(WORKSPACE_ROOT).as_posix() if root != WORKSPACE_ROOT else ""
        except Exception:
            tool_root = ""
        msg_list.insert(
            0,
            {
                "role": "system",
                # JSON examples are literal prompt text, not format fields.
                "content": (
                    f"Workspace root: {WORKSPACE_ROOT.as_posix()}\n"
                    f"Tool root (if set): {tool_root}\n\n"
                    + LOCAL_TOOL_PROMPT
                ),
            },
        )

    force_tools = tools_enabled and _force_tool_use(req)
    made_any_tool_call = False
    needs_material = force_tools and _wants_material_change(req)
    made_material_change = False
    tool_refusals = 0
    material_refusals = 0
    prefer_command = needs_material and _wants_command_action(req)
    use_completions = False

    for _ in range(max_tool_steps):
        payload: dict[str, Any] = {
            "model": model,
            "messages": msg_list,
            "stream": False,
        }
        if temperature is not None:
            payload["temperature"] = temperature
        if max_tokens is not None:
            payload["max_tokens"] = max_tokens
        if tools_enabled:
            payload["tools"] = LOCAL_TOOLS
            if force_tools and not made_any_tool_call:
                # Kickstart the tool loop with a harmless call.
                payload["tool_choice"] = {
                    "type": "function",
                    "function": {"name": "list_files"},
                }
            elif needs_material and not made_material_change:
                # If user asked to "create/scaffold/implement/run commands", do not stop at advice.
                # Start permissive (any tool) but escalate to a specific tool if the model keeps avoiding
                # material actions (mkdir/write_file/run_command).
                if material_refusals >= 2:
                    force_name = "run_command" if prefer_command else "write_file"
                    payload["tool_choice"] = {
                        "type": "function",
                        "function": {"name": force_name},
                    }
                else:
                    payload["tool_choice"] = "required"
            else:
                payload["tool_choice"] = "auto"

        res: dict[str, Any] | None = None
        last_err: RuntimeError | None = None
        from_completions = bool(use_completions and provider == "openai-compatible")

        if from_completions:
            comp_payload: dict[str, Any] = {
                "model": model,
                "prompt": _messages_to_prompt(msg_list),
                "stream": False,
            }
            if temperature is not None:
                comp_payload["temperature"] = temperature
            if max_tokens is not None:
                comp_payload["max_tokens"] = max_tokens
            try:
                res = _http_json(
                    "POST",
                    f"{base_url}/completions",
                    headers=headers,
                    body=comp_payload,
                    timeout_seconds=timeout,
                    error_context={"provider": provider, "model": model},
                )
            except RuntimeError as e:
                msg = str(e)
                if _should_use_max_completion_tokens(msg) and "max_tokens" in comp_payload:
                    comp_payload2 = _swap_max_tokens_to_max_completion_tokens(comp_payload)
                    res = _http_json(
                        "POST",
                        f"{base_url}/completions",
                        headers=headers,
                        body=comp_payload2,
                        timeout_seconds=timeout,
                        error_context={"provider": provider, "model": model},
                    )
                else:
                    raise
        else:
            paths = _chat_paths_for_provider(provider, base_url)
            for idx, path in enumerate(paths):
                try:
                    res = _http_json(
                        "POST",
                        f"{base_url}{path}",
                        headers=headers,
                        body=payload,
                        timeout_seconds=timeout,
                        error_context={"provider": provider, "model": model},
                    )
                    last_err = None
                    break
                except RuntimeError as e:
                    msg = str(e)
                    if provider == "openai-compatible" and _should_fallback_to_completions(msg):
                        use_completions = True
                        res = None
                        last_err = None
                        break

                    if _should_use_max_completion_tokens(msg) and "max_tokens" in payload:
                        payload2 = _swap_max_tokens_to_max_completion_tokens(payload)
                        try:
                            res = _http_json(
                                "POST",
                                f"{base_url}{path}",
                                headers=headers,
                                body=payload2,
                                timeout_seconds=timeout,
                                error_context={"provider": provider, "model": model},
                            )
                            last_err = None
                            break
                        except RuntimeError:
                            pass

                    # Some providers differ on tool_choice schema. Fallback to string "required" if
                    # a forced function tool_choice object is rejected.
                    if (
                        tools_enabled
                        and isinstance(payload.get("tool_choice"), dict)
                        and "HTTP 400" in msg
                        and ("tool_choice" in msg or "tool choice" in msg)
                    ):
                        payload3 = dict(payload)
                        payload3["tool_choice"] = "required"
                        try:
                            res = _http_json(
                                "POST",
                                f"{base_url}{path}",
                                headers=headers,
                                body=payload3,
                                timeout_seconds=timeout,
                                error_context={"provider": provider, "model": model},
                            )
                            last_err = None
                            break
                        except RuntimeError:
                            pass

                    # Some providers reject string tool_choice="required". Fall back to auto in that case.
                    if (
                        tools_enabled
                        and payload.get("tool_choice") == "required"
                        and "HTTP 400" in msg
                        and ("tool_choice" in msg or "tool choice" in msg)
                    ):
                        payload4 = dict(payload)
                        payload4["tool_choice"] = "auto"
                        try:
                            res = _http_json(
                                "POST",
                                f"{base_url}{path}",
                                headers=headers,
                                body=payload4,
                                timeout_seconds=timeout,
                                error_context={"provider": provider, "model": model},
                            )
                            last_err = None
                            break
                        except RuntimeError:
                            pass

                    last_err = e
                    # Endpoint variants differ across providers.
                    if ("HTTP 404" in msg or "HTTP 405" in msg) and idx < (len(paths) - 1):
                        continue
                    raise

            if res is None and use_completions and provider == "openai-compatible":
                from_completions = True
                comp_payload = {
                    "model": model,
                    "prompt": _messages_to_prompt(msg_list),
                    "stream": False,
                }
                if temperature is not None:
                    comp_payload["temperature"] = temperature
                if max_tokens is not None:
                    comp_payload["max_tokens"] = max_tokens
                try:
                    res = _http_json(
                        "POST",
                        f"{base_url}/completions",
                        headers=headers,
                        body=comp_payload,
                        timeout_seconds=timeout,
                        error_context={"provider": provider, "model": model},
                    )
                except RuntimeError as e:
                    msg = str(e)
                    if _should_use_max_completion_tokens(msg) and "max_tokens" in comp_payload:
                        comp_payload2 = _swap_max_tokens_to_max_completion_tokens(comp_payload)
                        res = _http_json(
                            "POST",
                            f"{base_url}/completions",
                            headers=headers,
                            body=comp_payload2,
                            timeout_seconds=timeout,
                            error_context={"provider": provider, "model": model},
                        )
                    else:
                        raise

        if res is None:
            if last_err is not None:
                raise last_err
            raise RuntimeError("chat request failed")

        if from_completions:
            content = _extract_text_from_openai_completions_response(res)
            tool_calls = None
        else:
            choice = (res.get("choices") or [{}])[0]
            message = choice.get("message") if isinstance(choice, dict) else {}
            message = message if isinstance(message, dict) else {}
            content = _normalize_assistant_content(message.get("content"))
            tool_calls = message.get("tool_calls")

        clean_content = str(content)
        text_tool_calls: list[dict[str, Any]] = []
        if tools_enabled:
            clean_content, text_tool_calls = _extract_spiral_coder_tool_calls(clean_content)
            if needs_material:
                # Some models refuse tool-calling and instead print file blocks / shell blocks.
                implied: list[dict[str, Any]] = []
                implied.extend(_extract_implied_write_file_calls(clean_content))
                implied.extend(_extract_implied_run_command_calls(clean_content))
                if implied:
                    merged: list[dict[str, Any]] = []
                    seen: set[tuple[str, str]] = set()
                    for c in (text_tool_calls + implied):
                        if not isinstance(c, dict):
                            continue
                        name = str(c.get("name") or "")
                        args = c.get("arguments") if isinstance(c.get("arguments"), dict) else {}
                        key = ""
                        if name in ("write_file", "mkdir"):
                            key = str(args.get("path") or "")
                        elif name == "run_command":
                            key = str(args.get("command") or "")
                        sig = (name, key)
                        if sig in seen:
                            continue
                        seen.add(sig)
                        merged.append(c)
                    text_tool_calls = merged

        if not tools_enabled:
            return {"content": str(clean_content), "model": model}

        if not isinstance(tool_calls, list) or not tool_calls:
            if text_tool_calls:
                made_any_tool_call = True
                msg_list.append({"role": "assistant", "content": clean_content or ""})
                pending: list[dict[str, str]] = []
                material_this_round = False
                for c in text_tool_calls:
                    name = str(c.get("name") or "")
                    args = c.get("arguments") if isinstance(c.get("arguments"), dict) else {}
                    try:
                        tool_out = _tool_dispatch(name, args, req=req)
                        if name in ("write_file", "mkdir", "run_command"):
                            made_material_change = True
                            material_this_round = True
                        result = {"ok": True, "result": tool_out}
                        if isinstance(tool_out, dict) and tool_out.get("needs_approval"):
                            pending.append(
                                {
                                    "id": str(tool_out.get("approval_id") or ""),
                                    "action": str(tool_out.get("action") or name),
                                    "path": str(tool_out.get("path") or ""),
                                }
                            )
                    except Exception as e:
                        result = {"ok": False, "error": str(e)}
                    msg_list.append(
                        {
                            "role": "user",
                            "content": "[spiral-coder-tool result]\n"
                            + json.dumps(
                                {"name": name, "result": result},
                                ensure_ascii=False,
                            ),
                        }
                    )
                if needs_material and not made_material_change and not material_this_round:
                    material_refusals += 1
                if pending:
                    lines = ["[pending approval]"]
                    for it in pending:
                        lines.append(
                            f"- {it.get('action')} {it.get('path')} id={it.get('id')}"
                        )
                    lines.append("Approve/reject in UI: Pending edits.")
                    return {"content": "\n".join(lines), "model": model}
                continue

            if needs_material and not made_material_change:
                tool_refusals += 1
                if tool_refusals <= 2:
                    msg_list.append(
                        {
                            "role": "system",
                            "content": (
                                "You must actually create/edit files now (not just describe steps).\n"
                                "Call mkdir/write_file/run_command, emit an ```spiral-coder-tool``` block, or use fallback blocks.\n"
                                "Fallback:\n"
                                "- file: <path> line then a fenced code block\n"
                                "- command: ```powershell```/```cmd```/```bash```\n"
                                "Example:\n"
                                "```spiral-coder-tool\n"
                                "{\"name\":\"mkdir\",\"arguments\":{\"path\":\"projects/maze-game\"}}\n"
                                "{\"name\":\"write_file\",\"arguments\":{\"path\":\"projects/maze-game/README.md\",\"content\":\"# Maze Game\\n\"}}\n"
                                "```"
                            ),
                        }
                    )
                    continue
                return {
                    "content": str(clean_content)
                    + "\n\n[Spiral-Coder] Material change was required but the model did not call tools.",
                    "model": model,
                }

            if force_tools and not made_any_tool_call:
                tool_refusals += 1
                if tool_refusals <= 2:
                    msg_list.append(
                        {
                            "role": "system",
                            "content": (
                                "Tool use is REQUIRED. Do not reply with steps only.\n"
                                "Call a tool, emit an ```spiral-coder-tool``` block of JSON, or use fallback blocks.\n"
                                "Fallback:\n"
                                "- file: <path> line then a fenced code block\n"
                                "- command: ```powershell```/```cmd```/```bash```\n"
                                "If unsure, start with:\n"
                                "```spiral-coder-tool\n"
                                "{\"name\":\"list_files\",\"arguments\":{\"pattern\":\"**/*\",\"max_results\":200}}\n"
                                "```"
                            ),
                        }
                    )
                    continue
                return {
                    "content": str(clean_content)
                    + "\n\n[Spiral-Coder] Tool use was required but the model did not call tools.",
                    "model": model,
                }

            return {"content": str(clean_content), "model": model}
        made_any_tool_call = True

        msg_list.append(
            {
                "role": "assistant",
                "content": clean_content or "",
                "tool_calls": tool_calls,
            }
        )

        pending2: list[dict[str, str]] = []
        material_this_round2 = False
        for tc in tool_calls:
            if not isinstance(tc, dict):
                continue
            tcid = str(tc.get("id") or "")
            fn = tc.get("function") if isinstance(tc.get("function"), dict) else {}
            name = str(fn.get("name") or "")
            args_json = fn.get("arguments")
            try:
                tool_out = _tool_dispatch(name, args_json, req=req)
                if name in ("write_file", "mkdir", "run_command"):
                    made_material_change = True
                    material_this_round2 = True
                result = {"ok": True, "result": tool_out}
                if isinstance(tool_out, dict) and tool_out.get("needs_approval"):
                    pending2.append(
                        {
                            "id": str(tool_out.get("approval_id") or ""),
                            "action": str(tool_out.get("action") or name),
                            "path": str(tool_out.get("path") or ""),
                        }
                    )
            except Exception as e:
                result = {"ok": False, "error": str(e)}
            out = json.dumps(result, ensure_ascii=False)
            if len(out) > 150000:
                out = out[:150000] + "\n...truncated..."
            msg_list.append(
                {
                    "role": "tool",
                    "tool_call_id": tcid,
                    "content": out,
                }
            )

        if pending2:
            lines = ["[pending approval]"]
            for it in pending2:
                lines.append(f"- {it.get('action')} {it.get('path')} id={it.get('id')}")
            lines.append("Approve/reject in UI: Pending edits.")
            return {"content": "\n".join(lines), "model": model}

        if needs_material and not made_material_change and not material_this_round2:
            material_refusals += 1

    raise RuntimeError("tool loop exceeded max steps")


def _build_messages(req: dict[str, Any]) -> list[dict[str, str]]:
    history = req.get("history") if isinstance(req.get("history"), list) else []
    messages: list[dict[str, str]] = []
    for item in history:
        if not isinstance(item, dict):
            continue
        role = str(item.get("role") or "").strip()
        if role not in ("user", "assistant"):
            continue
        messages.append({"role": role, "content": str(item.get("content") or "")})

    mode = str(req.get("mode") or "").strip()
    persona = str(req.get("persona") or "").strip()
    diff = str(req.get("diff") or "").strip()
    cot = _cot_level(req)
    autonomy = _autonomy_level(req)
    mode_key = mode.lower()
    meta_prompts = _load_meta_prompts()
    lang_raw = str(req.get("lang") or "").strip()
    # If we have a UI language set, always include a system message so language
    # constraints actually apply (especially important for Observer).
    if mode or persona or diff or lang_raw:
        system_lines: list[str] = []
        system_lines.append(_language_instruction(lang_raw))
        if mode:
            system_lines.append(f"Mode: {mode}")
        if persona:
            system_lines.append(f"Persona: {persona}")
        if diff:
            system_lines.append("Diff:\n" + diff)
        if mode_key != "observer":
            ui_lang = str(req.get("lang") or "").strip().lower()
            if ui_lang == "fr":
                system_lines.append("Language: French. Write your response in French.")
            elif ui_lang == "en":
                system_lines.append("Language: English. Write your response in English.")
            else:
                system_lines.append("Language: Japanese. Write your response in Japanese.")
            if cot == "brief":
                system_lines.append(
                    "Reasoning format: include a short 'Reasoning (brief)' section "
                    "with at most 3 bullets, then provide the final answer."
                )
            if cot == "structured":
                system_lines.append(
                    "Reasoning format: include these sections:\n"
                    "1) Plan: 3-7 bullets (high-level)\n"
                    "2) Risks: 2-5 bullets (what is most likely to break)\n"
                    "3) Result: what you did / what to do next\n"
                    "Keep it concise and do not include hidden chain-of-thought. Prefer concrete next actions (tools/files/commands)."
                )
            if cot == "deep":
                system_lines.append(
                    "Reasoning format: include these sections:\n"
                    "1) Plan: 5-12 bullets (detailed)\n"
                    "2) Risks: 3-8 bullets (what is most likely to break)\n"
                    "3) Checks: 2-6 bullets (how you will verify locally)\n"
                    "4) Result: what you did / what to do next\n"
                    "Do not include hidden chain-of-thought. Prefer concrete next actions (tools/files/commands)."
                )
            if autonomy == "longrun":
                system_lines.append(LONGRUN_AUTONOMY_PROMPT)
            system_lines.append(CODER_IDENTITY_PROMPT)
            extra = str(meta_prompts.get("coder_system_append") or "").strip()
            if extra:
                system_lines.append("[Meta prompt override]\n" + extra)
        if mode_key == "observer":
            system_lines.append(
                "Observer output rules:\n"
                "- Do not include 'Reasoning'/'Plan' headings even if cot/autonomy are enabled.\n"
                "- Do not produce step-by-step coding plans unless explicitly asked.\n"
            )
            system_lines.append(
                "Observer role: review coder outputs and code snippets from coder_context.\n"
                "- Focus on bugs, regressions, missing tests, and unsafe assumptions.\n"
                "- You MUST cite at least 1 concrete evidence from coder_context (quote or paraphrase).\n"
                "- If coder_context contains concrete errors (for example 429, 401, 403, 10061), "
                "you must explicitly mention them.\n"
                "- If you have actionable guidance, append:\n"
                "--- proposals ---\n"
                "1) title: <short title>\n"
                "   to_coder: <message to send>\n"
                "   severity: info|warn|crit"
            )
            persona_style = _observer_persona_prompt(persona, ui_lang=str(req.get("lang") or ""))
            if persona_style:
                system_lines.append(persona_style)
            extra = str(meta_prompts.get("observer_system_append") or "").strip()
            if extra:
                system_lines.append("[Meta prompt override]\n" + extra)
        messages.insert(0, {"role": "system", "content": "\n\n".join(system_lines)})

    messages.append({"role": "user", "content": str(req.get("input") or "")})
    return messages


def _extract_cli_message_text(msg: dict[str, Any]) -> str:
    content = msg.get("content")
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        chunks: list[str] = []
        for part in content:
            if isinstance(part, str):
                chunks.append(part)
                continue
            if not isinstance(part, dict):
                continue
            text = part.get("text")
            if isinstance(text, str):
                chunks.append(text)
        return "".join(chunks)
    return ""


def _extract_mistral_cli_output(stdout: str) -> str:
    raw = str(stdout or "").strip()
    if not raw:
        return ""

    try:
        payload = json.loads(raw)
    except json.JSONDecodeError:
        return raw

    if isinstance(payload, dict):
        if isinstance(payload.get("content"), str):
            return payload["content"]
        messages = payload.get("messages")
        if isinstance(messages, list):
            for m in reversed(messages):
                if isinstance(m, dict) and str(m.get("role", "")).lower() == "assistant":
                    txt = _extract_cli_message_text(m)
                    if txt.strip():
                        return txt
        text = payload.get("text")
        if isinstance(text, str) and text.strip():
            return text

    if isinstance(payload, list):
        for m in reversed(payload):
            if isinstance(m, dict) and str(m.get("role", "")).lower() == "assistant":
                txt = _extract_cli_message_text(m)
                if txt.strip():
                    return txt

    return raw


def _chat_mistral_cli(req: dict[str, Any], timeout: int) -> dict[str, Any]:
    cmd_raw = _env("SPIRAL_CODER_MISTRAL_CLI_CMD", "vibe").strip() or "vibe"
    base_cmd = shlex.split(cmd_raw)
    if not base_cmd:
        base_cmd = ["vibe"]

    mode = str(req.get("mode") or "").strip().lower()
    default_agent = "accept-edits" if mode != "observer" else "plan"
    agent = str(
        req.get("mistral_cli_agent")
        or _env("SPIRAL_CODER_MISTRAL_CLI_AGENT", default_agent)
    ).strip()
    autonomy = _autonomy_level(req)
    default_turns = "8" if autonomy == "longrun" else "4"
    max_turns = int(
        req.get("mistral_cli_max_turns")
        or _env("SPIRAL_CODER_MISTRAL_CLI_MAX_TURNS", default_turns)
    )
    max_turns = min(12, max(1, max_turns))

    messages = _build_messages(req)
    prompt_parts: list[str] = []
    for m in messages:
        role = str(m.get("role") or "").upper() or "USER"
        txt = str(m.get("content") or "").strip()
        if txt:
            prompt_parts.append(f"{role}:\n{txt}")
    prompt_parts.append("ASSISTANT:\n")
    prompt_text = "\n\n".join(prompt_parts)

    cmd = list(base_cmd)
    cmd.extend(["--output", "json", "--max-turns", str(max_turns)])
    if agent:
        cmd.extend(["--agent", agent])
    cmd.append(prompt_text)

    try:
        cp = subprocess.run(
            cmd,
            cwd=str(WORKSPACE_ROOT),
            capture_output=True,
            text=True,
            timeout=timeout,
            encoding="utf-8",
            errors="replace",
            shell=False,
        )
    except FileNotFoundError:
        raise RuntimeError(
            "Mistral CLI not found. Install with: uv tool install mistral-vibe "
            "or set SPIRAL_CODER_MISTRAL_CLI_CMD."
        ) from None
    except subprocess.TimeoutExpired:
        raise RuntimeError(f"Mistral CLI timed out after {timeout}s") from None

    if cp.returncode != 0:
        detail = (cp.stderr or cp.stdout or "").strip()
        if len(detail) > 3000:
            detail = detail[:3000] + "\n...truncated..."
        raise RuntimeError(f"Mistral CLI failed (exit {cp.returncode})\n{detail}")

    content = _extract_mistral_cli_output(cp.stdout)
    return {"content": content, "model": "mistral-cli"}


def _chat_impl(req: dict[str, Any]) -> dict[str, Any]:
    provider = _provider_from_req(req)
    if provider == "hf":
        raise RuntimeError("HF provider is not supported in lite mode")

    base_url = _base_url(provider, req)
    model = _pick_model(provider, req)
    api_key = _pick_api_key(provider, req)
    timeout = int(req.get("timeout_seconds") or 120)
    temperature = req.get("temperature")
    max_tokens = req.get("max_tokens")
    messages = _build_messages(req)

    if provider in ("mistral", "codestral", "openai-compatible", "gemini"):
        tools_enabled = provider in ("openai-compatible", "codestral", "mistral", "gemini") and _local_tools_enabled(req)
        autonomy = _autonomy_level(req)
        max_tool_steps = 60 if autonomy == "longrun" else LOCAL_TOOL_MAX_STEPS
        return _chat_openai_compat(
            req=req,
            provider=provider,
            base_url=base_url,
            model=model,
            api_key=api_key,
            timeout=timeout,
            temperature=temperature,
            max_tokens=max_tokens,
            messages=messages,
            tools_enabled=tools_enabled,
            max_tool_steps=max_tool_steps,
        )

    if provider == "mistral-cli":
        return _chat_mistral_cli(req, timeout=timeout)

    if provider == "anthropic":
        # Anthropic ignores "system" role in messages; send it as top-level field.
        system_text = ""
        anthropic_messages: list[dict[str, Any]] = []
        for m in messages:
            if m["role"] == "system":
                system_text = m["content"]
                continue
            anthropic_messages.append({"role": m["role"], "content": m["content"]})

        payload = {
            "model": model,
            "messages": anthropic_messages,
            "max_tokens": int(max_tokens or 1024),
        }
        if system_text:
            payload["system"] = system_text
        if temperature is not None:
            payload["temperature"] = temperature

        headers = {
            "Accept": "application/json",
            "anthropic-version": ANTHROPIC_VERSION,
        }
        if api_key:
            headers["x-api-key"] = api_key

        res = _http_json(
            "POST",
            f"{base_url}/messages",
            headers=headers,
            body=payload,
            timeout_seconds=timeout,
            error_context={"provider": provider, "model": model},
        )
        content = _extract_text_from_anthropic_response(res)
        return {"content": content, "model": model}

    raise RuntimeError(f"unsupported provider: {provider}")


def _models_impl(req: dict[str, Any]) -> dict[str, Any]:
    provider = _provider_from_req(req)
    if provider == "hf":
        return {"models": []}
    if provider == "mistral-cli":
        return {"models": ["mistral-medium-latest", "mistral-small-latest"]}
    if provider == "codestral":
        return {"models": ["codestral-latest", "mistral-small-latest"]}

    base_url = _base_url(provider, req)
    api_key = _pick_api_key(provider, req)
    timeout = 60

    headers: dict[str, str] = {"Accept": "application/json"}
    if provider == "anthropic":
        if api_key:
            headers["x-api-key"] = api_key
        headers["anthropic-version"] = ANTHROPIC_VERSION
    elif api_key:
        headers["Authorization"] = f"Bearer {api_key}"

    res = _http_json(
        "GET",
        f"{base_url}/models",
        headers=headers,
        timeout_seconds=timeout,
        error_context={"provider": provider},
    )

    models: list[str] = []
    if isinstance(res, dict) and isinstance(res.get("data"), list):
        for item in res["data"]:
            if isinstance(item, dict) and item.get("id"):
                models.append(str(item["id"]))
    elif isinstance(res, list):
        for item in res:
            if isinstance(item, str):
                models.append(item)

    models = sorted(set(m for m in models if m.strip()))
    return {"models": models}


class LiteHandler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def _send_bytes(self, code: int, content_type: str, payload: bytes) -> None:
        self.send_response(code)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(payload)))
        # Avoid stale JS/CSS during fast iterate loops (and reduce "why is it still English?" confusion).
        self.send_header("Cache-Control", "no-store")
        self.send_header("Pragma", "no-cache")
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(payload)

    def _send_json(self, code: int, payload: dict[str, Any]) -> None:
        data = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        self._send_bytes(code, "application/json; charset=utf-8", data)

    def _send_text(self, code: int, text: str) -> None:
        self._send_bytes(code, "text/plain; charset=utf-8", text.encode("utf-8"))

    def _read_json_body(self) -> dict[str, Any]:
        raw_len = self.headers.get("Content-Length", "0")
        try:
            length = int(raw_len)
        except ValueError:
            raise ValueError("invalid content-length")
        if length < 0 or length > MAX_BODY_BYTES:
            raise ValueError("request body too large")
        raw = self.rfile.read(length) if length else b"{}"
        try:
            payload = json.loads(raw.decode("utf-8"))
        except json.JSONDecodeError:
            raise ValueError("invalid JSON body")
        if not isinstance(payload, dict):
            raise ValueError("JSON body must be an object")
        return payload

    def _serve_file(self, path: Path, content_type: str) -> None:
        if not path.exists():
            self._send_text(404, "not found\n")
            return
        self._send_bytes(200, content_type, path.read_bytes())

    def do_GET(self) -> None:
        path = self.path.split("?", 1)[0]
        if path == "/":
            self._serve_file(WEB_ROOT / "index.html", "text/html; charset=utf-8")
            return
        static_assets = {
            "/assets/app.js": ("app.js", "text/javascript; charset=utf-8"),
            "/assets/styles.css": ("styles.css", "text/css; charset=utf-8"),
            **{
                f"/assets/{name}": (name, "text/javascript; charset=utf-8")
                for name in (
                    "core/state.js", "core/ui.js", "core/sandbox.js", "core/exec.js", "observer/logic.js",
                    "vendor/react.production.min.js", "vendor/react-dom.production.min.js",
                )
            },
        }
        if path in static_assets:
            name, content_type = static_assets[path]
            self._serve_file(WEB_ROOT / name, content_type)
            return
        if path == "/assets/governor_contract.js":
            contract = json.loads((REPO_ROOT / "shared/governor_contract.json").read_text(encoding="utf-8"))
            script = "window.__SPIRAL_CODER_GOVERNOR_CONTRACT_FALLBACK__ = " + json.dumps(contract) + ";\n"
            self._send_bytes(200, "text/javascript; charset=utf-8", script.encode("utf-8"))
            return
        if path == "/api/status":
            self._send_json(
                200,
                {
                    "ok": True,
                    "version": "lite",
                    "workspace_root": WORKSPACE_ROOT.as_posix(),
                    "features": {
                        "exec": True,
                        "pending_edits": True,
                        "pending_commands": False,
                        "harness_promotions": False,
                        "merge_gate": False,
                        "project_scan": False,
                        "chat_tools": False,
                        "meta_prompts": True,
                        "edit_approval_default": _as_bool(_env("SPIRAL_CODER_REQUIRE_EDIT_APPROVAL"), True),
                        "command_approval_default": _as_bool(_env("SPIRAL_CODER_REQUIRE_COMMAND_APPROVAL"), True),
                    },
                    "providers": {
                        "mistral": {
                            "api_key_present": _env_present("MISTRAL_API_KEY")
                            or _env_present("SPIRAL_CODER_API_KEY")
                        },
                        "codestral": {
                            "api_key_present": _env_present("CODESTRAL_API_KEY")
                            or _env_present("MISTRAL_API_KEY")
                            or _env_present("SPIRAL_CODER_API_KEY")
                        },
                        "gemini": {
                            "api_key_present": _env_present("GEMINI_API_KEY")
                            or _env_present("GOOGLE_API_KEY")
                        },
                        "anthropic": {
                            "api_key_present": _env_present("ANTHROPIC_API_KEY")
                        },
                        "openai-compatible": {
                            "api_key_present": _env_present("SPIRAL_CODER_API_KEY")
                            or _env_present("OPENAI_API_KEY")
                        },
                    },
                },
            )
            return
        if path == "/api/pending_edits":
            self._send_json(200, {"pending": _list_pending_edits()})
            return
        if path == "/api/meta_prompts":
            self._send_json(
                200,
                {
                    "ok": True,
                    "path": META_PROMPTS_REL,
                    "prompts": _load_meta_prompts(),
                },
            )
            return

        self._send_text(404, "not found\n")

    def do_POST(self) -> None:
        try:
            payload = self._read_json_body()
        except ValueError as e:
            self._send_json(400, {"error": str(e)})
            return

        path = self.path.split("?", 1)[0]

        if path == "/api/models":
            try:
                self._send_json(200, _models_impl(payload))
            except Exception as e:
                out = {"error": str(e)}
                if _as_bool(_env("SPIRAL_CODER_DEBUG"), False):
                    out["trace"] = traceback.format_exc()
                self._send_json(502, out)
            return

        if path == "/api/chat":
            try:
                self._send_json(200, _chat_impl(payload))
            except Exception as e:
                out = {"error": str(e)}
                if _as_bool(_env("SPIRAL_CODER_DEBUG"), False):
                    out["trace"] = traceback.format_exc()
                self._send_json(502, out)
            return

        if path == "/api/chat_stream":
            self._serve_chat_stream(payload)
            return

        if path == "/api/exec":
            try:
                self._send_json(200, _exec_impl(payload))
            except Exception as e:
                out = {"error": str(e)}
                if _as_bool(_env("SPIRAL_CODER_DEBUG"), False):
                    out["trace"] = traceback.format_exc()
                self._send_json(400, out)
            return

        if path == "/api/approve_edit":
            try:
                edit_id = str(payload.get("id") or "").strip()
                if not edit_id:
                    raise RuntimeError("id is required")
                item = _approve_pending_edit(edit_id)
                self._send_json(200, {"ok": True, "item": item})
            except Exception as e:
                out = {"error": str(e)}
                if _as_bool(_env("SPIRAL_CODER_DEBUG"), False):
                    out["trace"] = traceback.format_exc()
                self._send_json(400, out)
            return

        if path == "/api/reject_edit":
            try:
                edit_id = str(payload.get("id") or "").strip()
                if not edit_id:
                    raise RuntimeError("id is required")
                item = _reject_pending_edit(edit_id)
                self._send_json(200, {"ok": True, "item": item})
            except Exception as e:
                out = {"error": str(e)}
                if _as_bool(_env("SPIRAL_CODER_DEBUG"), False):
                    out["trace"] = traceback.format_exc()
                self._send_json(400, out)
            return
        
        if path == "/api/meta_prompts":
            try:
                op = str(payload.get("op") or "").strip().lower()
                target = str(payload.get("target") or "").strip().lower()
                text = str(payload.get("text") or "")
                if op not in ("set", "append"):
                    raise RuntimeError("op must be 'set' or 'append'")
                if target not in ("coder", "observer"):
                    raise RuntimeError("target must be 'coder' or 'observer'")
                if len(text) > 20000:
                    raise RuntimeError("text too large")

                cur = _load_meta_prompts()
                key = "coder_system_append" if target == "coder" else "observer_system_append"
                base = str(cur.get(key) or "")
                new_val = text.strip()
                if op == "append":
                    if base.strip():
                        new_val = (base.rstrip() + "\n" + new_val).strip()
                cur[key] = new_val

                self._send_json(200, _queue_meta_prompts_write(cur))
            except Exception as e:
                out = {"error": str(e)}
                if _as_bool(_env("SPIRAL_CODER_DEBUG"), False):
                    out["trace"] = traceback.format_exc()
                self._send_json(400, out)
            return

        self._send_text(404, "not found\n")

    def _serve_chat_stream(self, payload: dict[str, Any]) -> None:
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream; charset=utf-8")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Connection", "close")
        self.end_headers()

        try:
            result = _chat_impl(payload)
            content = str(result.get("content") or "")
            if not content:
                self._write_sse("done", {})
                return
            chunk_size = 24
            for i in range(0, len(content), chunk_size):
                self._write_sse("delta", {"delta": content[i : i + chunk_size]})
            self._write_sse("done", {})
        except Exception as e:
            out = {"error": str(e)}
            if _as_bool(_env("SPIRAL_CODER_DEBUG"), False):
                out["trace"] = traceback.format_exc()
            self._write_sse("error", out)
            self._write_sse("done", {})
        except BrokenPipeError:
            return

    def _write_sse(self, event: str, data: dict[str, Any]) -> None:
        payload = json.dumps(data, ensure_ascii=False)
        msg = f"event: {event}\ndata: {payload}\n\n".encode("utf-8")
        self.wfile.write(msg)
        self.wfile.flush()

    def log_message(self, fmt: str, *args: Any) -> None:
        sys.stderr.write("%s - - [%s] %s\n" % (self.client_address[0], self.log_date_time_string(), fmt % args))


def main() -> int:
    parser = argparse.ArgumentParser(description="Run Spiral-Coder lite server")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=18080)
    parser.add_argument(
        "--workspace",
        default=_env("SPIRAL_CODER_WORKSPACE_ROOT", "").strip() or str(DEFAULT_WORKSPACE_ROOT),
        help="Workspace root for local tools (default: $SPIRAL_CODER_WORKSPACE_ROOT or ~/spiral-coder-work).",
    )
    args = parser.parse_args()

    _set_workspace_root(str(args.workspace))

    server = ThreadingHTTPServer((args.host, args.port), LiteHandler)
    print(f"Spiral-Coder Lite UI: http://{args.host}:{args.port}/")
    print(f"Workspace root: {WORKSPACE_ROOT.as_posix()}")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
