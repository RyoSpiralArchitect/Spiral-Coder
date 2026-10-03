# Spiral-Coder Lite CLI

This project includes a Python CLI that avoids `spiral-coder.exe` and works under WDAC.

## App Install (Global Command)

### Option A: Wrapper installer (Windows)

```powershell
cd C:\Users\user\spiral-coder
powershell -ExecutionPolicy Bypass -File .\scripts\install-lite-cli.ps1 -Scope User -Force
spiral-coder-lite list-providers
```

### Option B: Python tool install

```powershell
cd C:\Users\user\spiral-coder
uv tool install -e .
spiral-coder-lite list-providers
```

## Commands

```powershell
cd C:\Users\user\spiral-coder
python .\scripts\spiral_coder_lite_cli.py list-providers
python .\scripts\spiral_coder_lite_cli.py list-presets
python .\scripts\spiral_coder_lite_cli.py doctor --provider mistral --check-models
python .\scripts\spiral_coder_lite_cli.py chat "hello" --provider openai-compatible --api-key "<KEY>"
python .\scripts\spiral_coder_lite_cli.py chat "scaffold game" --provider openai-compatible --tool-root projects/mygame
python .\scripts\spiral_coder_lite_cli.py repl --provider openai-compatible --api-key "<KEY>"
python .\scripts\spiral_coder_lite_cli.py serve --host 127.0.0.1 --port 18080 --workspace .
python .\scripts\spiral_coder_lite_cli.py pending --server http://127.0.0.1:18080
python .\scripts\spiral_coder_lite_cli.py approve --server http://127.0.0.1:18080 edit_xxx
python .\scripts\spiral_coder_lite_cli.py reject --server http://127.0.0.1:18080 edit_xxx
```

PowerShell wrapper:

```powershell
.\scripts\spiral-coder-lite.ps1 chat "hello" --provider openai-compatible --api-key "<KEY>"
```

## CLI UX Improvements

- `doctor` command checks provider/model/base_url/api-key mismatch before runtime.
- `serve` command starts local web UI directly from CLI.
- `chat`/`repl` support `--server http://127.0.0.1:18080` to share pending approvals with the GUI server.
- `pending`/`approve`/`reject` let you manage approval queue from CLI.
- `--tool-root subdir` restricts file/command tools under a project folder (useful for multi-repo workspaces).
- `serve --workspace <path>` sets the workspace root for tools (default: repo root).
- REPL supports `/doctor` for in-session config diagnostics.
- If you change `--provider` and keep default CLI values, base URL/model are auto-switched to provider defaults.
- Coder tools include `run_command` for terminal execution in workspace root.
- Web UI settings include separate `Edit approval` and `Command approval` toggles.

## Codestral Endpoint

Codestral can be used directly with:

- Base URL: `https://codestral.mistral.ai/v1`
- Chat endpoint: `/chat/completion` (fallback to `/chat/completions` is supported)
- FIM endpoint: `/fim/completions` (reference endpoint)

UI/CLI provider name: `codestral`.

## Mistral Official CLI Provider

`mistral-cli` provider calls the official `vibe` command.

```powershell
python .\scripts\spiral_coder_lite_cli.py chat "Implement feature X" --provider mistral-cli
```

If `vibe` is missing:

```powershell
uv tool install mistral-vibe
vibe --setup
```

## Autonomy and CoT

Default behavior:

- `--autonomy longrun`
- `--cot brief`
- `--require-edit-approval` (ON)
- `--require-command-approval` (ON)

Disable if needed:

```powershell
python .\scripts\spiral_coder_lite_cli.py chat "hi" --autonomy off --cot off
```

Stronger CoT formatting:

```powershell
python .\scripts\spiral_coder_lite_cli.py chat "hi" --cot structured
```

Even deeper CoT formatting:

```powershell
python .\scripts\spiral_coder_lite_cli.py chat "hi" --cot deep
```

UI approval endpoints:

- `GET /api/pending_edits`
- `POST /api/approve_edit` with `{"id":"edit_xxx"}`
- `POST /api/reject_edit` with `{"id":"edit_xxx"}`

Env defaults:

- `SPIRAL_CODER_REQUIRE_EDIT_APPROVAL=1|0`
- `SPIRAL_CODER_REQUIRE_COMMAND_APPROVAL=1|0`
