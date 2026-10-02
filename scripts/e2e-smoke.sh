#!/usr/bin/env bash
set -euo pipefail

HOST="127.0.0.1"
PORT="18090"
WORKSPACE_ROOT=""

usage() {
  cat <<'USAGE'
Usage: bash ./scripts/e2e-smoke.sh [--host <host>] [--port <port>] [--workspace <dir>]

Starts the Python Lite server and verifies local UI assets, status and exec offline.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --host|--port|--workspace)
      if [[ $# -lt 2 || -z "$2" ]]; then
        echo "Missing value for $1" >&2
        exit 2
      fi
      case "$1" in
        --host) HOST="$2" ;;
        --port) PORT="$2" ;;
        --workspace) WORKSPACE_ROOT="$2" ;;
      esac
      shift 2
      ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown arg: $1" >&2; usage >&2; exit 2 ;;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"
for tool in python3 curl git; do
  if ! command -v "${tool}" >/dev/null 2>&1; then
    echo "${tool} not found. Install it, then retry." >&2
    exit 1
  fi
done

scratch="$(mktemp -d "${TMPDIR:-/tmp}/spiral-coder-smoke.XXXXXX")"
pid=""
cleanup() {
  result="$?"
  if [[ -n "${pid}" ]]; then
    kill "${pid}" >/dev/null 2>&1 || true
    wait "${pid}" >/dev/null 2>&1 || true
  fi
  if [[ "${result}" -ne 0 && -f "${scratch}/server.log" ]]; then
    cat "${scratch}/server.log" >&2
  fi
  rm -rf "${scratch}"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

ws="${WORKSPACE_ROOT:-${scratch}/workspace}"
mkdir -p "${ws}"
ws="$(cd "${ws}" && pwd -P)"
echo "Starting Lite server: http://${HOST}:${PORT}/  workspace=${ws}"
python3 -S scripts/serve_lite.py --host "${HOST}" --port "${PORT}" --workspace "${ws}" >"${scratch}/server.log" 2>&1 &
pid="$!"

ready=false
for ((attempt = 0; attempt < 50; attempt++)); do
  if ! kill -0 "${pid}" 2>/dev/null; then
    echo "Lite server exited before readiness" >&2
    exit 1
  fi
  if curl --noproxy '*' -fsS --max-time 1 "http://${HOST}:${PORT}/api/status" >"${scratch}/status.json" 2>/dev/null; then
    ready=true
    break
  fi
  sleep 0.1
done
if [[ "${ready}" != true ]]; then
  echo "Lite server did not become ready" >&2
  exit 1
fi

# Program text comes from the heredoc; JSON comes from its own file, never stdin.
python3 -S - "${scratch}/status.json" "${ws}" <<'PY'
import json, pathlib, sys
j = json.loads(pathlib.Path(sys.argv[1]).read_text())
if j.get("ok") is not True:
    raise SystemExit("status.ok is false")
if pathlib.Path(j.get("workspace_root", "")).resolve() != pathlib.Path(sys.argv[2]):
    raise SystemExit("status belongs to a different server/workspace")
PY

# Load every local asset referenced by the page, including split runtime helpers.
python3 -S - "http://${HOST}:${PORT}" <<'PY'
from html.parser import HTMLParser
from urllib.request import build_opener, ProxyHandler
import sys
opener = build_opener(ProxyHandler({}))
base = sys.argv[1]
class Assets(HTMLParser):
    urls = []
    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        url = attrs.get("src") if tag == "script" else attrs.get("href") if tag == "link" else None
        if url and url.startswith("/assets/"):
            self.urls.append(url)
with opener.open(base + "/", timeout=5) as response:
    html = response.read().decode()
assert "app-root" in html and "Spiral-Coder" in html, "missing UI root or branding"
assets = Assets(); assets.feed(html)
assert assets.urls, "index has no local assets"
for url in assets.urls:
    with opener.open(base + url, timeout=5) as response:
        assert response.read(), f"empty asset: {url}"
print(f"Local UI assets OK: {len(assets.urls)}")
PY

tid="thread_e2e_smoke_$$"
cwd=".tmp/${tid}"
body="$(python3 -S - "${cwd}" <<'PY'
import json, sys
print(json.dumps({"command": "mkdir -p demo-repo && cd demo-repo && git init", "cwd": sys.argv[1], "timeout_seconds": 60}))
PY
)"
curl --noproxy '*' -fsS --max-time 15 -X POST -H "Content-Type: application/json" --data "${body}" "http://${HOST}:${PORT}/api/exec" >"${scratch}/exec.json"
python3 -S - "${scratch}/exec.json" <<'PY'
import json, pathlib, sys
j = json.loads(pathlib.Path(sys.argv[1]).read_text())
if j.get("exit_code") != 0:
    raise SystemExit(f"exec failed: {j}")
PY

git_dir="${ws}/${cwd}/demo-repo/.git"
if [[ ! -d "${git_dir}" ]]; then
  echo "expected git dir missing: ${git_dir}" >&2
  exit 1
fi
echo "E2E smoke OK: local assets + status + git exec"
