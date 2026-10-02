#!/usr/bin/env bash
set -euo pipefail

FORCE=1
PATH_CONTAINS=""

usage() {
  cat <<'USAGE'
Usage: bash ./scripts/kill-spiral-coder.sh [--force|--no-force] [--path-contains <needle>]

Stops running `spiral-coder` processes.
- --path-contains filters by the process command line substring (best-effort).
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --force)
      FORCE=1
      shift
      ;;
    --no-force)
      FORCE=0
      shift
      ;;
    --path-contains)
      PATH_CONTAINS="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown arg: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

needle=""
if [[ -n "${PATH_CONTAINS}" ]]; then
  if command -v realpath >/dev/null 2>&1; then
    needle="$(realpath "${PATH_CONTAINS}" 2>/dev/null || echo "${PATH_CONTAINS}")"
  else
    if [[ -e "${PATH_CONTAINS}" ]]; then
      needle="$(cd "$(dirname "${PATH_CONTAINS}")" && pwd)/$(basename "${PATH_CONTAINS}")"
    else
      needle="${PATH_CONTAINS}"
    fi
  fi
fi

pids=""
if command -v pgrep >/dev/null 2>&1; then
  pids="$(pgrep -x spiral-coder 2>/dev/null || true)"
else
  pids="$(ps ax -o pid= -o comm= 2>/dev/null | awk '$2=="spiral-coder"{print $1}' || true)"
fi

if [[ -z "${pids}" ]]; then
  if [[ -n "${needle}" ]]; then
    echo "[kill-spiral-coder] no spiral-coder process (filtered)"
  else
    echo "[kill-spiral-coder] no spiral-coder process"
  fi
  exit 0
fi

filtered=()
for pid in ${pids}; do
  if [[ -n "${needle}" ]]; then
    cmd="$(ps -p "${pid}" -o command= 2>/dev/null || true)"
    if [[ -z "${cmd}" ]]; then
      continue
    fi
    if [[ "${cmd}" != *"${needle}"* ]]; then
      continue
    fi
  fi
  filtered+=("${pid}")
done

if [[ ${#filtered[@]} -eq 0 ]]; then
  echo "[kill-spiral-coder] no spiral-coder process (filtered)"
  exit 0
fi

echo "[kill-spiral-coder] stopping ${#filtered[@]} process(es)..."
if [[ "${FORCE}" == "1" ]]; then
  for pid in "${filtered[@]}"; do
    kill -9 "${pid}" 2>/dev/null || true
  done
else
  for pid in "${filtered[@]}"; do
    kill "${pid}" 2>/dev/null || true
  done
fi

echo "[kill-spiral-coder] done"

