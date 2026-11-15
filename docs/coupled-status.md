# Inspect a coupled run without spending requests

`spiral coupled-status --run-dir PATH` prints one JSON object describing an
existing `coupled-run --out-dir` directory. It reconciles the top-level
`iterations.json` manifest with each round's durable request ledger
(`iteration-NN/state/tui-work.json`, or the round `report.json` work snapshot
when the durable file is absent). The command is read-only: it never calls a
provider, never writes, and needs no credentials.

```sh
spiral coupled-status --run-dir ../spiral-runs/feature-01
```

A directory without `iterations.json` is not a run; the command exits non-zero
and reports on stderr rather than printing a fake success object.

## What the output means

- `recorded` — the manifest's own claims: `completed`, `stop_reason`,
  `active_iteration`, `interrupted_iteration`, per-run totals.
- `iterations[]` — one entry per manifest row, plus the interrupted round and
  any on-disk `iteration-*` directory the manifest never recorded. Each entry
  pairs `recorded` counts with the independently loaded `ledger`, and lists
  reconciliation `issues`.
- `ledger` — per-round counts derived from receipts, not from the manifest:
  `task_requests`, `transport_requests`, `wire_requests` (every attempt
  exactly once, including in-flight ones), `pending_requests`
  (`in_flight_or_interrupted` attempts whose outcome is not durable),
  `usage_unknown` (receipts without provider usage), and per-role splits.
- `totals` — summed across all recorded rounds. When any round's ledger is
  missing or unreadable, totals are `null`: unknown usage is never reported
  as zero. A ledger file that exists but cannot be parsed is flagged as an
  issue instead of failing the whole report.
- `qualification` — separates the run's recorded claim from an independent
  check. `qualified_complete` is true only when the manifest recorded
  completion *and* every durable receipt reconciles without issues. A
  completion claim with no recorded work receipt in any round — no changed
  file, newly verified check or reproduced baseline — cannot qualify: the
  done report is the run's own assertion, not evidence.

Unrecorded iteration directories on disk are flagged as issues and their
usage is reported per round but excluded from `totals`, so receipts are never
double-counted.

## Reading interrupted runs

While a round is running its ledger includes in-flight attempts, and an
interrupted run may leave `task_requests`/`wire_requests` as `null` in the
manifest. `coupled-status` still loads whatever ledger survived, counts the
in-flight attempt once in `wire_requests`, and keeps `pending_requests` and
`usage_unknown` visible instead of guessing an outcome.

See [One task through Coder and Observer](coupled-work-loop.md) for how the
run records these receipts and [state schema](state-schema.md) for ownership.
