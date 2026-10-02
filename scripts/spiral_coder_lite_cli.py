#!/usr/bin/env python3
"""CLI front-end for scripts/serve_lite.py chat runtime."""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import threading
import time
from pathlib import Path
from typing import Any
from urllib import error as urlerror
from urllib import request as urlrequest

try:
    from . import serve_lite  # type: ignore
except Exception:
    import serve_lite  # type: ignore


PROVIDERS = ["openai-compatible", "mistral", "codestral", "gemini", "anthropic", "mistral-cli", "hf"]
CLI_DEFAULT_PROVIDER = "openai-compatible"
CLI_DEFAULT_MODEL = "gpt-4o-mini"
CLI_DEFAULT_BASE_URL = "https://api.openai.com/v1"

LANGS = ("ja", "en", "fr")


def _configure_stdio() -> None:
    # Windows consoles often use a legacy codepage (e.g. cp932). Ensure we never
    # crash on output due to encoding errors (best-effort, no deps).
    for stream in (sys.stdout, sys.stderr):
        try:
            # TextIOWrapper in Python 3.7+
            stream.reconfigure(errors="replace")  # type: ignore[attr-defined]
        except Exception:
            pass


def _enable_vt_ansi() -> None:
    # Enable ANSI escapes on Windows when possible (Windows Terminal / modern conhost).
    if os.name != "nt":
        return
    try:
        import ctypes  # noqa: PLC0415

        kernel32 = ctypes.windll.kernel32
        STD_OUTPUT_HANDLE = -11
        ENABLE_VIRTUAL_TERMINAL_PROCESSING = 0x0004

        h = kernel32.GetStdHandle(STD_OUTPUT_HANDLE)
        mode = ctypes.c_uint32()
        if not kernel32.GetConsoleMode(h, ctypes.byref(mode)):
            return
        kernel32.SetConsoleMode(h, mode.value | ENABLE_VIRTUAL_TERMINAL_PROCESSING)
    except Exception:
        return


_configure_stdio()
_enable_vt_ansi()


def _term_supports_color() -> bool:
    try:
        if not sys.stdout.isatty():
            return False
        if os.environ.get("NO_COLOR"):
            return False
        # Windows Terminal / modern consoles generally support ANSI escapes.
        return True
    except Exception:
        return False


_COLOR_OK = _term_supports_color()


def _c(code: str, s: str) -> str:
    if not _COLOR_OK:
        return s
    return f"\x1b[{code}m{s}\x1b[0m"


def _dim(s: str) -> str:
    return _c("2", s)


def _bold(s: str) -> str:
    return _c("1", s)


def _fg(color_code: int, s: str) -> str:
    return _c(str(color_code), s)


_TXT: dict[str, dict[str, str]] = {
    "en": {
        "banner": "Spiral-Coder Lite TUI (Python)",
        "hint_help": "Type /help for commands.",
        "prompt_coder": "coder> ",
        "prompt_observer": "observer> ",
        "pending": "Pending edits",
        "approve": "approve",
        "reject": "reject",
        "approved": "approved",
        "rejected": "rejected",
        "no_pending": "(no pending edits)",
        "proposals": "Proposals",
        "apply_meta": "apply meta",
        "send_to_coder": "send to coder",
        "err": "Error",
    },
    "ja": {
        "banner": "Spiral-Coder Lite TUI (Python)",
        "hint_help": "/help でコマンド一覧",
        "prompt_coder": "coder> ",
        "prompt_observer": "observer> ",
        "pending": "承認待ち",
        "approve": "承認",
        "reject": "却下",
        "approved": "承認済み",
        "rejected": "却下済み",
        "no_pending": "（承認待ちなし）",
        "proposals": "提案",
        "apply_meta": "meta適用",
        "send_to_coder": "Coderへ送る",
        "err": "エラー",
    },
    "fr": {
        "banner": "Spiral-Coder Lite TUI (Python)",
        "hint_help": "Tapez /help pour les commandes.",
        "prompt_coder": "codeur> ",
        "prompt_observer": "observateur> ",
        # Keep strings ASCII to avoid UnicodeEncodeError on legacy Windows codepages.
        "pending": "Editions en attente",
        "approve": "approuver",
        "reject": "rejeter",
        "approved": "approuve",
        "rejected": "rejete",
        "no_pending": "(aucune edition en attente)",
        "proposals": "Propositions",
        "apply_meta": "appliquer meta",
        "send_to_coder": "envoyer au codeur",
        "err": "Erreur",
    },
}


def _t(lang: str, key: str) -> str:
    l = str(lang or "").strip().lower()
    if l not in LANGS:
        l = "en"
    return (_TXT.get(l) or _TXT["en"]).get(key) or (_TXT["en"].get(key) or key)


def _read_optional(path: str | None) -> str | None:
    if not path:
        return None
    p = Path(path)
    if not p.exists():
        raise RuntimeError(f"file not found: {p}")
    return p.read_text(encoding="utf-8", errors="replace")


def _server_join(base: str, path: str) -> str:
    b = str(base or "").strip().rstrip("/")
    p = str(path or "").strip()
    if not p.startswith("/"):
        p = "/" + p
    return b + p


def _http_json(method: str, url: str, body: dict[str, Any] | None = None, timeout: int = 30) -> Any:
    data = None
    headers: dict[str, str] = {"Accept": "application/json"}
    if body is not None:
        data = json.dumps(body).encode("utf-8")
        headers["Content-Type"] = "application/json"
    req = urlrequest.Request(url=url, data=data, headers=headers, method=method)
    try:
        with urlrequest.urlopen(req, timeout=timeout) as resp:
            raw = resp.read()
    except urlerror.HTTPError as e:
        raw = e.read()
        msg = raw.decode("utf-8", errors="replace")
        raise RuntimeError(f"HTTP {e.code}\n{msg}") from None
    except urlerror.URLError as e:
        raise RuntimeError(f"request failed: {e.reason}") from None

    if not raw:
        return {}
    try:
        return json.loads(raw.decode("utf-8"))
    except json.JSONDecodeError:
        return raw.decode("utf-8", errors="replace")


def _coerce_provider_defaults(
    provider: str, base_url: str | None, model: str | None
) -> tuple[str, str]:
    p = str(provider or "").strip() or CLI_DEFAULT_PROVIDER
    b = str(base_url or "").strip()
    m = str(model or "").strip()

    # If user switched provider but left CLI defaults, apply provider defaults.
    if not b or (p != CLI_DEFAULT_PROVIDER and b == CLI_DEFAULT_BASE_URL):
        b = str(serve_lite.DEFAULT_BASE_URL.get(p, CLI_DEFAULT_BASE_URL))
    if not m or (p != CLI_DEFAULT_PROVIDER and m == CLI_DEFAULT_MODEL):
        m = str(serve_lite.DEFAULT_MODEL.get(p, CLI_DEFAULT_MODEL))
    return b, m


def _provider_needs_api_key(provider: str) -> bool:
    return provider in ("openai-compatible", "mistral", "codestral", "gemini", "anthropic")


def _effective_api_key(provider: str, inline_key: str | None) -> tuple[str, str]:
    inline = str(inline_key or "").strip()
    if inline:
        return inline, "inline"
    env_key = str(serve_lite._provider_key_from_env(provider) or "").strip()
    if env_key:
        return env_key, "env"
    return "", "none"


def _maybe_set_workspace(args: argparse.Namespace) -> None:
    """Initialize WORKSPACE_ROOT when running in-process (no --server)."""
    server = str(getattr(args, "server", "") or "").strip()
    if server:
        return
    ws = str(getattr(args, "workspace", "") or "").strip()
    if not ws:
        ws = str(serve_lite.DEFAULT_WORKSPACE_ROOT)
    serve_lite._set_workspace_root(ws)


def _preflight(req: dict[str, Any]) -> None:
    provider = str(req.get("provider") or "").strip()
    base_url = str(req.get("base_url") or "").strip()
    api_key = str(req.get("api_key") or "").strip()
    if hasattr(serve_lite, "_preflight_provider_config"):
        serve_lite._preflight_provider_config(provider, base_url, api_key)


def _run_doctor_from_req(req: dict[str, Any], check_models: bool = False) -> int:
    provider = str(req.get("provider") or "")
    base_url = str(req.get("base_url") or "")
    model = str(req.get("model") or "")
    tool_root = str(req.get("tool_root") or "")
    api_key, key_source = _effective_api_key(provider, str(req.get("api_key") or ""))
    needs_key = _provider_needs_api_key(provider)

    print("Spiral-Coder Doctor")
    print(f"  provider:   {provider}")
    print(f"  mode:       {req.get('mode')}")
    print(f"  base_url:   {base_url or '(empty)'}")
    print(f"  model:      {model or '(empty)'}")
    if tool_root.strip():
        print(f"  tool_root:  {tool_root.strip()}")
    print(f"  api_key:    {'present' if bool(api_key) else 'missing'} ({key_source})")
    print(f"  cot:        {req.get('cot')}")
    print(f"  autonomy:   {req.get('autonomy')}")
    print(f"  approval:   {req.get('require_edit_approval')}")
    print(f"  cmd apprv:  {req.get('require_command_approval')}")

    warnings: list[str] = []
    try:
        _preflight({**req, "api_key": api_key})
    except Exception as e:
        warnings.append(str(e))

    if needs_key and not api_key:
        if provider in ("mistral", "codestral"):
            warnings.append("API key missing. Set MISTRAL_API_KEY or SPIRAL_CODER_API_KEY.")
        elif provider == "openai-compatible":
            warnings.append("API key missing. Set OPENAI_API_KEY or SPIRAL_CODER_API_KEY.")
        elif provider == "anthropic":
            warnings.append("API key missing. Set ANTHROPIC_API_KEY.")

    if warnings:
        print("")
        print("Warnings:")
        for w in warnings:
            print(f"  - {w}")
    else:
        print("")
        print("Config check: OK")

    if check_models:
        print("")
        print("Model probe:")
        try:
            out = serve_lite._models_impl(
                {
                    "provider": provider,
                    "base_url": base_url,
                    "api_key": api_key,
                }
            )
            models = out.get("models") if isinstance(out, dict) else None
            count = len(models) if isinstance(models, list) else 0
            print(f"  success: {count} models")
            if isinstance(models, list) and models:
                for m in models[:8]:
                    print(f"  - {m}")
        except Exception as e:
            print(f"  failed: {e}")

    return 0


def _common_req(args: argparse.Namespace) -> dict[str, Any]:
    provider = str(args.provider or "").strip() or CLI_DEFAULT_PROVIDER
    base_url, model = _coerce_provider_defaults(provider, args.base_url, args.model)
    if provider == "mistral-cli":
        base_url = ""

    req: dict[str, Any] = {
        "provider": provider,
        "model": model,
        "chat_model": args.chat_model or model,
        "code_model": args.code_model or model,
        "base_url": base_url,
        "api_key": args.api_key,
        "lang": str(getattr(args, "lang", "") or "").strip() or "ja",
        "tool_root": args.tool_root,
        "mode": args.mode,
        "persona": args.persona,
        "temperature": args.temperature,
        "max_tokens": args.max_tokens,
        "timeout_seconds": args.timeout_seconds,
        "cot": args.cot,
        "autonomy": args.autonomy,
        "require_edit_approval": args.require_edit_approval,
        "require_command_approval": args.require_command_approval,
    }
    if args.mistral_cli_agent:
        req["mistral_cli_agent"] = args.mistral_cli_agent
    if args.mistral_cli_max_turns:
        req["mistral_cli_max_turns"] = args.mistral_cli_max_turns
    return req


def _run_chat(args: argparse.Namespace) -> int:
    prompt = str(args.prompt or "").strip()
    if args.stdin:
        stdin_text = sys.stdin.read()
        if stdin_text.strip():
            prompt = (prompt + "\n\n[stdin]\n" + stdin_text).strip()
    if not prompt:
        raise RuntimeError("prompt is required")

    diff_text = _read_optional(args.diff_file)
    req = _common_req(args)
    req["lang"] = str(args.lang or "").strip() or "ja"
    req["input"] = prompt
    req["history"] = []
    req["diff"] = diff_text

    # Fail fast for obvious provider/base_url/api-key mismatches.
    _preflight({**req, "api_key": _effective_api_key(req["provider"], req.get("api_key"))[0]})

    if str(args.server or "").strip():
        url = _server_join(str(args.server), "/api/chat")
        out = _http_json("POST", url, req, timeout=int(req.get("timeout_seconds") or 120) + 10)
        if isinstance(out, dict) and out.get("error"):
            raise RuntimeError(str(out.get("error")))
        if isinstance(out, dict):
            print(str(out.get("content") or ""))
        else:
            print(str(out))
        return 0

    _maybe_set_workspace(args)
    out2 = serve_lite._chat_impl(req)
    print(str(out2.get("content") or ""))
    return 0


def _run_repl(args: argparse.Namespace) -> int:
    _maybe_set_workspace(args)
    history: list[dict[str, str]] = []
    diff_text = _read_optional(args.diff_file)
    base_req = _common_req(args)
    base_req["lang"] = str(args.lang or "").strip() or "ja"
    server = str(args.server or "").strip()
    use_server = bool(server)
    print("Spiral-Coder Lite CLI REPL")
    print("  /help  show commands")
    print("  /exit  quit")
    print(
        "  provider={}  model={}  mode={}".format(
            base_req.get("provider"), base_req.get("model"), base_req.get("mode")
        )
    )
    if use_server:
        print(f"  server={server}")
    try:
        _preflight(
            {
                **base_req,
                "api_key": _effective_api_key(
                    str(base_req.get("provider")), str(base_req.get("api_key") or "")
                )[0],
            }
        )
    except Exception as e:
        print(f"  preflight warning: {e}")
    print("")

    while True:
        try:
            line = input("spiral-coder-lite> ").strip()
        except EOFError:
            print("")
            break
        except KeyboardInterrupt:
            print("")
            continue

        if not line:
            continue
        if line in ("/exit", "/quit"):
            break
        if line == "/help":
            print("Commands:")
            print("  /help")
            print("  /exit | /quit")
            print("  /reset")
            print("  /config")
            print("  /doctor")
            continue
        if line == "/reset":
            history = []
            print("(history cleared)")
            continue
        if line == "/config":
            cfg = _common_req(args)
            safe = {
                "provider": cfg.get("provider"),
                "model": cfg.get("model"),
                "chat_model": cfg.get("chat_model"),
                "code_model": cfg.get("code_model"),
                "base_url": cfg.get("base_url"),
                "tool_root": cfg.get("tool_root"),
                "mode": cfg.get("mode"),
                "persona": cfg.get("persona"),
                "temperature": cfg.get("temperature"),
                "max_tokens": cfg.get("max_tokens"),
                "timeout_seconds": cfg.get("timeout_seconds"),
                "cot": cfg.get("cot"),
                "autonomy": cfg.get("autonomy"),
                "require_edit_approval": cfg.get("require_edit_approval"),
                "require_command_approval": cfg.get("require_command_approval"),
            }
            print(json.dumps(safe, ensure_ascii=False, indent=2))
            continue
        if line == "/doctor":
            _run_doctor_from_req(_common_req(args), check_models=False)
            continue

        req = _common_req(args)
        req["lang"] = str(args.lang or "").strip() or "ja"
        req["input"] = line
        req["history"] = history
        req["diff"] = diff_text

        try:
            if use_server:
                url = _server_join(server, "/api/chat")
                out = _http_json(
                    "POST",
                    url,
                    req,
                    timeout=int(req.get("timeout_seconds") or 120) + 10,
                )
                if isinstance(out, dict) and out.get("error"):
                    raise RuntimeError(str(out.get("error")))
            else:
                out = serve_lite._chat_impl(req)
        except Exception as e:
            print(f"Error: {e}", file=sys.stderr)
            continue

        if isinstance(out, dict):
            content = str(out.get("content") or "")
        else:
            content = str(out or "")
        print("")
        print(content)
        print("")
        history.append({"role": "user", "content": line})
        history.append({"role": "assistant", "content": content})

    return 0


def _parse_proposals(text: str) -> list[dict[str, Any]]:
    s = str(text or "")
    m = re.search(r"---\s*proposals\s*---", s, flags=re.IGNORECASE)
    if not m:
        return []

    tail = s[m.end() :]
    lines = tail.splitlines()

    out: list[dict[str, Any]] = []
    cur: dict[str, Any] | None = None
    last_key = ""

    def finish() -> None:
        nonlocal cur, last_key
        if cur is None:
            return
        title = str(cur.get("title") or "").strip()
        to_coder = str(cur.get("to_coder") or "").strip()
        if not title and not to_coder:
            cur = None
            last_key = ""
            return
        sev = str(cur.get("severity") or "info").strip().lower()
        if sev not in ("info", "warn", "crit"):
            sev = "info"
        try:
            score = int(str(cur.get("score") or "").strip() or "50")
        except Exception:
            score = 50
        score = max(0, min(100, score))
        phase = str(cur.get("phase") or "any").strip().lower() or "any"
        impact = str(cur.get("impact") or "").strip()
        cost = str(cur.get("cost") or "").strip().lower()
        out.append(
            {
                "id": str(len(out) + 1),
                "title": title or "(untitled)",
                "to_coder": to_coder,
                "severity": sev,
                "score": score,
                "phase": phase,
                "impact": impact,
                "cost": cost,
            }
        )
        cur = None
        last_key = ""

    for line in lines:
        if re.match(r"^\s*---", line):
            if out or cur is not None:
                finish()
                break
            continue

        start = re.match(r"^\s*(\d+)\)\s*title\s*:\s*(.*)\s*$", line)
        if start:
            finish()
            cur = {"title": start.group(2), "to_coder": "", "severity": "info"}
            last_key = "title"
            continue
        if cur is None:
            continue

        m_to = re.match(r"^\s*to_coder\s*:\s*(.*)\s*$", line)
        if m_to:
            cur["to_coder"] = m_to.group(1)
            last_key = "to_coder"
            continue

        m_sev = re.match(r"^\s*severity\s*:\s*(info|warn|crit)\b", line, flags=re.IGNORECASE)
        if m_sev:
            cur["severity"] = m_sev.group(1).lower()
            last_key = "severity"
            continue

        m_score = re.match(r"^\s*score\s*:\s*(\d+)", line)
        if m_score:
            cur["score"] = m_score.group(1)
            last_key = "score"
            continue

        m_phase = re.match(r"^\s*phase\s*:\s*(\w+)", line)
        if m_phase:
            cur["phase"] = m_phase.group(1)
            last_key = "phase"
            continue

        m_imp = re.match(r"^\s*impact\s*:\s*(.+)$", line)
        if m_imp:
            cur["impact"] = m_imp.group(1)
            last_key = "impact"
            continue

        m_cost = re.match(r"^\s*cost\s*:\s*(\w+)", line)
        if m_cost:
            cur["cost"] = m_cost.group(1)
            last_key = "cost"
            continue

        if re.match(r"^\s+", line):
            cont = line.strip()
            if not cont:
                continue
            if last_key == "to_coder":
                cur["to_coder"] = str(cur.get("to_coder") or "") + "\n" + cont
            elif last_key == "title":
                cur["title"] = str(cur.get("title") or "") + " " + cont
            elif last_key == "impact":
                cur["impact"] = str(cur.get("impact") or "") + " " + cont

    finish()
    return out


def _parse_meta_prompt_op(to_coder: str) -> dict[str, str] | None:
    s = str(to_coder or "")
    m = re.search(
        r"^\s*META_(SET|APPEND)_(CODER|OBSERVER)\s*:\s*(.*)\s*$",
        s,
        flags=re.IGNORECASE | re.MULTILINE,
    )
    if not m:
        return None
    op = "append" if m.group(1).strip().lower() == "append" else "set"
    target = "observer" if m.group(2).strip().lower() == "observer" else "coder"
    head = str(m.group(3) or "").strip()
    rest = s[m.end() :].lstrip("\r\n").strip()
    text = (head + ("\n" + rest if rest else "")).strip() if head else rest
    if not text.strip():
        return None
    return {"op": op, "target": target, "text": text}


def _auto_observe_prompt(lang: str) -> str:
    l = str(lang or "").strip().lower()
    if l == "fr":
        return (
            "[AUTO-OBSERVE] Le codeur a produit un nouvel output. "
            "Decris en une phrase ce qui s'est passe, puis analyse 5 axes de risques, "
            "et termine par un bloc --- proposals ---."
        )
    if l == "en":
        return (
            "[AUTO-OBSERVE] The coder produced a new output. "
            "In one sentence say what happened, then analyze 5 risk dimensions, "
            "and end with a --- proposals --- block."
        )
    return (
        "[AUTO-OBSERVE] コーダーが新しいアウトプットを生成した。"
        "何が起きたかを一文で述べてから、5軸でリスクを洗い出し、最後に --- proposals --- ブロックを出力せよ。"
    )


def _observer_intensity_instr(intensity: str) -> str:
    lvl = str(intensity or "critical").strip().lower()
    if lvl == "polite":
        return (
            "Intensity: polite. Be constructive and encouraging.\n"
            "Still flag concrete issues across all five dimensions.\n"
            "Anti-loop: NEW issues only; if nothing new, summarise still-open items as [OPEN]."
        )
    if lvl == "brutal":
        return (
            "Intensity: brutal. Assume this ships to 10,000 users at midnight.\n"
            "Required: identify at least TWO failure modes (one correctness bug, one operational risk).\n"
            "Forbidden: generic praise. Only concrete flaws with concrete fixes.\n"
            "Anti-loop: unresolved issues must be marked [ESCALATED] and score +10."
        )
    return (
        "Intensity: critical. Treat this as a pre-merge review.\n"
        "Required: identify at least ONE concrete bug/security risk/architectural weakness.\n"
        "Anti-loop: if no new signal, reply exactly: [Observer] No new critique. Loop detected."
    )


def _coder_context_packet(req: dict[str, Any], history: list[dict[str, str]]) -> str:
    last_user = None
    last_asst = None
    for it in reversed(history):
        if it.get("role") == "assistant" and last_asst is None:
            last_asst = str(it.get("content") or "")
        if it.get("role") == "user" and last_user is None:
            last_user = str(it.get("content") or "")
        if last_user is not None and last_asst is not None:
            break

    def cut(s: str | None, n: int) -> str:
        ss = str(s or "").strip()
        return ss if len(ss) <= n else (ss[:n] + "...")

    parts: list[str] = []
    parts.append("--- coder_context ---")
    parts.append(f"provider: {req.get('provider')}")
    parts.append(f"model: {req.get('model')}")
    parts.append(f"mode: {req.get('mode')}")
    parts.append(f"tool_root: {req.get('tool_root') or ''}")
    if last_user:
        parts.append("last_user:\n" + cut(last_user, 1200))
    if last_asst:
        parts.append("last_assistant:\n" + cut(last_asst, 2600))
    pend = []
    try:
        pend = serve_lite._list_pending_edits()
    except Exception:
        pend = []
    if pend:
        p2 = [it for it in pend if isinstance(it, dict) and str(it.get("status") or "") == "pending"]
        if p2:
            parts.append("pending_edits:")
            for it in p2[:6]:
                parts.append(
                    f"- {it.get('action')} {it.get('path')} id={it.get('id')}"
                )
    return "\n".join(parts)


def _run_tui(args: argparse.Namespace) -> int:
    _maybe_set_workspace(args)

    lang = str(args.lang or "ja").strip().lower()
    provider = str(args.provider or "").strip()
    base_url, model = _coerce_provider_defaults(provider, args.base_url, args.model)
    use_server = bool(str(args.server or "").strip())
    server = str(args.server or "").strip()

    coder_req_base = _common_req(args)
    coder_history: list[dict[str, str]] = []

    # Observer config: same provider/base/model by default.
    observer_persona = str(getattr(args, "observer_persona", "") or "novelist").strip()
    observer_intensity = str(getattr(args, "observer_intensity", "") or "critical").strip().lower()
    auto_observe = bool(getattr(args, "auto_observe", True))
    observer_req_base = dict(coder_req_base)
    observer_req_base["mode"] = "Observer"
    observer_req_base["persona"] = observer_persona

    proposals: list[dict[str, Any]] = []
    active_pane = "coder"

    print(_bold(_t(lang, "banner")))
    if not use_server:
        print(_dim(f"workspace: {serve_lite.WORKSPACE_ROOT.as_posix()}"))
    else:
        print(_dim(f"server: {server}"))
    print(_dim(_t(lang, "hint_help")))
    print("")

    def list_pending() -> list[dict[str, Any]]:
        if use_server:
            out = _http_json("GET", _server_join(server, "/api/pending_edits"), None, timeout=10)
            items = out.get("pending") if isinstance(out, dict) else []
            return items if isinstance(items, list) else []
        return serve_lite._list_pending_edits()

    def resolve_pending(edit_id: str, approve: bool) -> Any:
        if use_server:
            path = "/api/approve_edit" if approve else "/api/reject_edit"
            return _http_json("POST", _server_join(server, path), {"id": edit_id}, timeout=30)
        return serve_lite._approve_pending_edit(edit_id) if approve else serve_lite._reject_pending_edit(edit_id)

    def chat(req: dict[str, Any]) -> str:
        if use_server:
            out = _http_json("POST", _server_join(server, "/api/chat"), req, timeout=int(req.get("timeout_seconds") or 120) + 10)
            if isinstance(out, dict) and out.get("error"):
                raise RuntimeError(str(out.get("error")))
            return str(out.get("content") or "") if isinstance(out, dict) else str(out)
        out2 = serve_lite._chat_impl(req)
        return str(out2.get("content") or "")

    def send_to_coder(user_text: str) -> str:
        nonlocal coder_history
        req = dict(coder_req_base)
        req["input"] = user_text
        req["history"] = list(coder_history)
        req["diff"] = _read_optional(args.diff_file)
        content = chat(req)
        coder_history = coder_history + [{"role": "user", "content": user_text}, {"role": "assistant", "content": content}]
        return content

    def send_to_observer(user_text: str) -> str:
        req = dict(observer_req_base)
        req["input"] = user_text
        req["history"] = []  # keep observer stateless by default
        req["diff"] = _read_optional(args.diff_file)

        bridge = "\n".join(
            [
                "[Observer bridge]",
                _observer_intensity_instr(observer_intensity),
                "Review the coder artifacts below. Check each dimension: CORRECTNESS, SECURITY, RELIABILITY, PERFORMANCE, MAINTAINABILITY.",
                "Append a proposals block with all actionable findings.",
            ]
        )
        ctx = _coder_context_packet(coder_req_base, coder_history)
        content = chat({**req, "input": (user_text + "\n\n" + bridge + "\n\n" + ctx)})
        return content

    def print_pending() -> None:
        items = [it for it in list_pending() if isinstance(it, dict) and str(it.get("status") or "") == "pending"]
        if not items:
            print(_dim(_t(lang, "no_pending")))
            return
        print(_bold(_t(lang, "pending")) + ":")
        for it in items[:12]:
            eid = str(it.get("id") or "")
            action = str(it.get("action") or "")
            path = str(it.get("path") or "")
            preview = str(it.get("preview") or "").strip()
            if preview:
                preview = preview.replace("\r\n", "\n").split("\n", 1)[0]
                if len(preview) > 100:
                    preview = preview[:100] + "..."
            line = f"- {eid}  {action}  {path}".rstrip()
            if preview:
                line += f"  preview={preview}"
            print(line)

    def print_proposals() -> None:
        nonlocal proposals
        if not proposals:
            return
        print("")
        print(_bold(_t(lang, "proposals")) + ":")
        for i, p in enumerate(proposals, start=1):
            sev = str(p.get("severity") or "info")
            score = int(p.get("score") or 50)
            title = str(p.get("title") or "")
            sev_tag = sev.upper()
            if sev == "crit":
                sev_tag = _fg(91, sev_tag)
            elif sev == "warn":
                sev_tag = _fg(93, sev_tag)
            else:
                sev_tag = _dim(sev_tag)
            print(f"  {i}) [{sev_tag}] {score:3d}pt  {title}")
        print(_dim("  /do <n>  send proposal to coder    /apply <n>  apply META_* to runtime prompt"))

    def apply_meta_from_proposal(idx: int) -> None:
        if idx < 1 or idx > len(proposals):
            return
        p = proposals[idx - 1]
        op = _parse_meta_prompt_op(str(p.get("to_coder") or ""))
        if not op:
            return
        if use_server:
            out = _http_json("POST", _server_join(server, "/api/meta_prompts"), op, timeout=30)
            eid = str(out.get("approval_id") or "") if isinstance(out, dict) else ""
        else:
            cur = serve_lite._load_meta_prompts()
            key = "coder_system_append" if op["target"] == "coder" else "observer_system_append"
            base = str(cur.get(key) or "")
            val = str(op["text"] or "").strip()
            if op["op"] == "append" and base.strip():
                val = (base.rstrip() + "\n" + val).strip()
            cur[key] = val
            out2 = serve_lite._queue_meta_prompts_write(cur)
            eid = str(out2.get("approval_id") or "")
        if eid:
            print(_dim(f"meta prompt queued: {eid} (approve to apply)"))
        else:
            print(_dim("meta prompt queued (approve to apply)"))

    while True:
        try:
            prompt = _t(lang, "prompt_coder") if active_pane == "coder" else _t(lang, "prompt_observer")
            line = input(prompt)
        except EOFError:
            print("")
            break
        except KeyboardInterrupt:
            print("")
            continue

        s = str(line or "").rstrip("\r\n")
        if not s.strip():
            continue

        if s.strip().startswith("/"):
            parts = s.strip().split(" ", 1)
            cmd = parts[0].lower()
            arg = parts[1].strip() if len(parts) > 1 else ""

            if cmd in ("/exit", "/quit"):
                break
            if cmd == "/help":
                print("Commands:")
                print("  /help")
                print("  /exit | /quit")
                print("  /coder            switch input to coder")
                print("  /observer | /obs  switch input to observer (manual)")
                print("  /observe          run auto-observe once")
                print("  /pending          list pending edits")
                print("  /approve <id|all> approve pending edit(s)")
                print("  /reject  <id|all> reject pending edit(s)")
                print("  /do <n>            send proposal #n to coder")
                print("  /apply <n>         apply META_* proposal #n (queues approval)")
                print("  /toolroot <path>   set tool_root under workspace")
                continue
            if cmd == "/coder":
                active_pane = "coder"
                continue
            if cmd in ("/observer", "/obs"):
                active_pane = "observer"
                continue
            if cmd == "/toolroot":
                coder_req_base["tool_root"] = arg
                observer_req_base["tool_root"] = arg
                print(_dim(f"tool_root={arg}"))
                continue
            if cmd == "/pending":
                print_pending()
                continue
            if cmd == "/approve":
                targets: list[str] = []
                if arg == "all":
                    targets = [
                        str(it.get("id") or "")
                        for it in list_pending()
                        if isinstance(it, dict) and str(it.get("status") or "") == "pending"
                    ]
                elif arg:
                    targets = [arg]
                if not targets:
                    print(_dim(_t(lang, "no_pending")))
                    continue
                for eid in targets:
                    resolve_pending(eid, approve=True)
                    print(f"{_t(lang, 'approved')}\t{eid}")
                    # Nudge coder to resume after approvals.
                    send_to_coder(
                        "\n".join(
                            [
                                "[Spiral-Coder] Pending edit approved. Continue without redoing the approved step.",
                                f"id: {eid}",
                            ]
                        )
                    )
                continue
            if cmd == "/reject":
                targets2: list[str] = []
                if arg == "all":
                    targets2 = [
                        str(it.get("id") or "")
                        for it in list_pending()
                        if isinstance(it, dict) and str(it.get("status") or "") == "pending"
                    ]
                elif arg:
                    targets2 = [arg]
                if not targets2:
                    print(_dim(_t(lang, "no_pending")))
                    continue
                for eid in targets2:
                    resolve_pending(eid, approve=False)
                    print(f"{_t(lang, 'rejected')}\t{eid}")
                continue
            if cmd == "/do":
                try:
                    n = int(arg)
                except Exception:
                    n = 0
                if n < 1 or n > len(proposals):
                    continue
                p = proposals[n - 1]
                title = str(p.get("title") or "").strip()
                to_coder = str(p.get("to_coder") or "").strip()
                sev = str(p.get("severity") or "info").strip()
                steer = f"[Observer proposal approved]\nTitle: {title}\nSeverity: {sev}\n\n{to_coder}\n"
                out = send_to_coder(steer)
                print("")
                print(_fg(96, out))
                continue
            if cmd == "/apply":
                try:
                    n2 = int(arg)
                except Exception:
                    n2 = 0
                if n2 < 1 or n2 > len(proposals):
                    continue
                apply_meta_from_proposal(n2)
                continue
            if cmd == "/observe":
                active_pane = "coder"
                obs_out = send_to_observer(_auto_observe_prompt(lang))
                print("")
                print(_fg(95, obs_out))
                proposals = _parse_proposals(obs_out)
                print_proposals()
                continue

            print(_dim(f"(unknown command: {cmd})"))
            continue

        if active_pane == "observer":
            obs_out2 = send_to_observer(s.strip())
            print("")
            print(_fg(95, obs_out2))
            proposals = _parse_proposals(obs_out2)
            print_proposals()
            continue

        # Default: send to coder.
        try:
            out = send_to_coder(s.strip())
        except Exception as e:
            print(f"{_t(lang, 'err')}: {e}", file=sys.stderr)
            continue

        print("")
        print(_fg(96, out))

        # After coder output, show pending edits if any.
        pend2 = [it for it in list_pending() if isinstance(it, dict) and str(it.get("status") or "") == "pending"]
        if pend2:
            print("")
            print_pending()

        if auto_observe:
            obs_out3 = send_to_observer(_auto_observe_prompt(lang))
            print("")
            print(_fg(95, obs_out3))
            proposals = _parse_proposals(obs_out3)
            print_proposals()

    return 0


def _run_doctor(args: argparse.Namespace) -> int:
    req = _common_req(args)
    api_key, _ = _effective_api_key(req["provider"], req.get("api_key"))
    return _run_doctor_from_req({**req, "api_key": api_key}, check_models=bool(args.check_models))


def _run_serve(args: argparse.Namespace) -> int:
    if getattr(args, "workspace", ""):
        serve_lite._set_workspace_root(str(args.workspace))
    host = str(args.host or "127.0.0.1").strip() or "127.0.0.1"
    port = int(args.port or 18080)
    server = serve_lite.ThreadingHTTPServer((host, port), serve_lite.LiteHandler)
    print(f"Spiral-Coder Lite UI: http://{host}:{port}/")
    print(f"Workspace root: {serve_lite.WORKSPACE_ROOT.as_posix()}")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return 0


def _run_pending(args: argparse.Namespace) -> int:
    server = str(args.server or "http://127.0.0.1:18080").strip() or "http://127.0.0.1:18080"
    url = _server_join(server, "/api/pending_edits")
    out = _http_json("GET", url, None, timeout=10)
    if not isinstance(out, dict):
        raise RuntimeError("invalid pending response")
    items = out.get("pending")
    items = items if isinstance(items, list) else []
    if args.json:
        print(json.dumps(out, ensure_ascii=False, indent=2))
        return 0

    if not items:
        print("(no pending edits)")
        return 0

    for it in items:
        if not isinstance(it, dict):
            continue
        eid = str(it.get("id") or "")
        status = str(it.get("status") or "")
        action = str(it.get("action") or "")
        path = str(it.get("path") or "")
        preview = str(it.get("preview") or "").strip()
        if preview:
            preview = preview.replace("\r\n", "\n").replace("\r", "\n")
            preview = preview.split("\n", 1)[0]
            if len(preview) > 120:
                preview = preview[:120] + "..."
        line = f"{status}\t{eid}\t{action}\t{path}".rstrip()
        if preview:
            line += f"\tpreview={preview}"
        print(line)
    return 0


def _run_resolve_pending(args: argparse.Namespace, approve: bool) -> int:
    server = str(args.server or "http://127.0.0.1:18080").strip() or "http://127.0.0.1:18080"

    def resolve_one(edit_id: str) -> Any:
        path = "/api/approve_edit" if approve else "/api/reject_edit"
        url = _server_join(server, path)
        return _http_json("POST", url, {"id": edit_id}, timeout=30)

    if args.all:
        pending = _http_json("GET", _server_join(server, "/api/pending_edits"), None, timeout=10)
        items = pending.get("pending") if isinstance(pending, dict) else []
        items = items if isinstance(items, list) else []
        targets = [
            str(it.get("id") or "")
            for it in items
            if isinstance(it, dict) and str(it.get("status") or "") == "pending"
        ]
        if not targets:
            print("(no pending edits)")
            return 0
        for eid in targets:
            res = resolve_one(eid)
            if args.json:
                print(json.dumps(res, ensure_ascii=False))
            else:
                print(f"{'approved' if approve else 'rejected'}\t{eid}")
        return 0

    edit_id = str(args.id or "").strip()
    if not edit_id:
        raise RuntimeError("id is required (or use --all)")
    res2 = resolve_one(edit_id)
    if args.json:
        print(json.dumps(res2, ensure_ascii=False, indent=2))
    else:
        print(f"{'approved' if approve else 'rejected'}\t{edit_id}")
    return 0


def _add_common_flags(p: argparse.ArgumentParser) -> None:
    p.add_argument("--provider", choices=PROVIDERS, default=CLI_DEFAULT_PROVIDER)
    p.add_argument("--model", default=CLI_DEFAULT_MODEL)
    p.add_argument("--chat-model", default=None)
    p.add_argument("--code-model", default=None)
    p.add_argument("--base-url", default=CLI_DEFAULT_BASE_URL)
    p.add_argument("--api-key", default="")
    p.add_argument("--lang", choices=list(LANGS), default=serve_lite._env("SPIRAL_CODER_LANG", "ja").strip() or "ja")
    p.add_argument(
        "--workspace",
        default=serve_lite._env("SPIRAL_CODER_WORKSPACE_ROOT", "").strip() or str(serve_lite.DEFAULT_WORKSPACE_ROOT),
        help="Workspace root for local tools when running in-process (ignored when --server is set)",
    )
    p.add_argument("--mode", default="VIBE")
    p.add_argument("--persona", default="default")
    p.add_argument("--temperature", type=float, default=0.4)
    p.add_argument("--max-tokens", type=int, default=1024)
    p.add_argument("--timeout-seconds", type=int, default=120)
    p.add_argument("--tool-root", default="", help="Restrict local tools to a subdir under workspace root")
    p.add_argument(
        "--server",
        default="",
        help="If set, call a running Spiral-Coder Lite server (example: http://127.0.0.1:18080)",
    )
    p.add_argument("--diff-file", default=None)
    p.add_argument("--cot", choices=["off", "brief", "structured", "deep"], default="brief")
    p.add_argument("--autonomy", choices=["off", "longrun"], default="longrun")
    p.add_argument(
        "--require-edit-approval",
        action=argparse.BooleanOptionalAction,
        default=True,
    )
    p.add_argument(
        "--require-command-approval",
        action=argparse.BooleanOptionalAction,
        default=True,
    )
    p.add_argument("--mistral-cli-agent", default=None)
    p.add_argument("--mistral-cli-max-turns", type=int, default=None)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Spiral-Coder lite CLI")
    sub = parser.add_subparsers(dest="cmd", required=True)

    chat = sub.add_parser("chat", help="Run one-shot chat")
    _add_common_flags(chat)
    chat.add_argument("prompt")
    chat.add_argument("--stdin", action="store_true", help="Append stdin to prompt")

    repl = sub.add_parser("repl", help="Run interactive REPL")
    _add_common_flags(repl)

    tui = sub.add_parser("tui", help="Dual-brain terminal cockpit (Windows-friendly)")
    _add_common_flags(tui)
    tui.add_argument(
        "--auto-observe",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="Auto-run Observer after each Coder response",
    )
    tui.add_argument("--observer-persona", default="novelist")
    tui.add_argument("--observer-intensity", choices=["polite", "critical", "brutal"], default="critical")

    sub.add_parser("list-providers", help="List providers")
    sub.add_parser("list-presets", help="Show provider default base_url/model")

    doctor = sub.add_parser("doctor", help="Validate CLI/provider configuration")
    _add_common_flags(doctor)
    doctor.add_argument("--check-models", action="store_true", help="Call provider /models endpoint")

    serve = sub.add_parser("serve", help="Run local Spiral-Coder Lite web UI server")
    serve.add_argument("--host", default="127.0.0.1")
    serve.add_argument("--port", type=int, default=18080)
    serve.add_argument(
        "--workspace",
        default=serve_lite._env("SPIRAL_CODER_WORKSPACE_ROOT", "").strip() or str(serve_lite.DEFAULT_WORKSPACE_ROOT),
        help="Override workspace root for local tools (default: $SPIRAL_CODER_WORKSPACE_ROOT or ~/spiral-coder-work)",
    )

    pending = sub.add_parser("pending", help="List pending edits from a running server")
    pending.add_argument("--server", default="http://127.0.0.1:18080")
    pending.add_argument("--json", action="store_true")

    approve = sub.add_parser("approve", help="Approve a pending edit")
    approve.add_argument("id", nargs="?", default="")
    approve.add_argument("--all", action="store_true", help="Approve all pending edits")
    approve.add_argument("--server", default="http://127.0.0.1:18080")
    approve.add_argument("--json", action="store_true")

    reject = sub.add_parser("reject", help="Reject a pending edit")
    reject.add_argument("id", nargs="?", default="")
    reject.add_argument("--all", action="store_true", help="Reject all pending edits")
    reject.add_argument("--server", default="http://127.0.0.1:18080")
    reject.add_argument("--json", action="store_true")

    args = parser.parse_args(argv)

    if args.cmd == "list-providers":
        for p in PROVIDERS:
            print(p)
        return 0
    if args.cmd == "list-presets":
        for p in PROVIDERS:
            base = str(serve_lite.DEFAULT_BASE_URL.get(p, ""))
            model = str(serve_lite.DEFAULT_MODEL.get(p, ""))
            print(f"{p}\tbase_url={base or '(none)'}\tmodel={model or '(none)'}")
        return 0
    if args.cmd == "chat":
        return _run_chat(args)
    if args.cmd == "repl":
        return _run_repl(args)
    if args.cmd == "tui":
        return _run_tui(args)
    if args.cmd == "doctor":
        return _run_doctor(args)
    if args.cmd == "serve":
        return _run_serve(args)
    if args.cmd == "pending":
        return _run_pending(args)
    if args.cmd == "approve":
        return _run_resolve_pending(args, approve=True)
    if args.cmd == "reject":
        return _run_resolve_pending(args, approve=False)
    raise RuntimeError(f"unknown command: {args.cmd}")


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        raise SystemExit(2)
