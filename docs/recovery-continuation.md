# Recovery diagnosis across continuation

## r11 failure and restoration contract

The retained [r11 probe](evals/2026-10-31-repo-maintenance/recovery-probe.rs)
exposes two independent obligations: the failed check points at the test, while
the failed edit targets the source. Reading both diagnoses both; it repairs
neither. A generic `Diagnose` reconstructed from failed-check history must not
erase that completed source diagnosis.

`RecoveryGovernor::restore_focus` now reconciles outstanding edits before the
existing focused-check restoration. `recovery_edit_diagnosis.rs`, under the
recovery-progress module, derives a temporary ledger from correlated native
call/result pairs in the current task. No persistent fields or snapshot versions
are added. The ledger retains the latest failed call ID and read status per edit
target, separately tracks failed reads, and checks failed command ownership.

Restoration permits `Fix` only when the current edit's native diagnosis is
retained, every outstanding edit target has a subsequent matching native read,
failed reads have their own successful retries, and the active check has no
outstanding diagnosis. New failures reset only their corresponding evidence.
Malformed arguments, uncorrelated results, stale reads, unrelated headers,
refusals, unknown failures and wrong-root execution cannot supply that proof.

The existing edit memory still owns actual-edit reset boundaries. The existing
focused-check ledger still owns newest-check routing, deferred failures,
creation-time automatic-check identity and exact successful verification.
Diagnosing a newer check does not resolve a deferred one. An outstanding failed
edit restores to `Fix`, never `Verify`, even if the check ledger remembers an
older repair attempt. No read creates repair, verification or completion proof.
The existing live transitions, independent assessors and archived r11 evidence
are not changed.

## Model-free regression coverage

`src/tui/agent/recovery_resume_tests.rs` adds ordinary tests for the cross-file
handoff, missing/stale/unrelated/uncorrelated reads, a newer same or different
check, a newer edit, an older unread edit, failed reads on each type of target,
overlapping read/edit/check failures, old repair state, actual verification,
unlocalized/wrong-root/diagnostic failures, edit-only recovery and task isolation.
The newer-check regression asserts the current newest-focus behavior rather
than the obsolete assumption in the external historical probe.

Run the new tests and the existing recovery controls without model calls:

```sh
cargo fmt --all -- --check
cargo test --locked --bin spiral-coder tui::agent::recovery::
cargo test --locked --bin spiral-coder tui::agent::recovery_focus::
cargo test --locked --bin spiral-coder tui::agent::edit_failure::
cargo test --locked --bin spiral-coder tui::agent::tests::
bash scripts/check-local.sh
```

## Actual retained-session regression

The original full session/workspace are local evidence, not repository fixtures.
Their manifest and hashes are published in r11; they must not be replaced by a
synthetic session and then reported as a real-session pass. The native regression
is explicitly ignored by default, not silently passed when inputs are absent.
It consumes the original session unchanged and asserts the three actual read
call IDs, `Fix`, a still-pending check and absence of repair/completion evidence.

Set `SPIRAL_DIAGNOSTIC_SESSION` to the retained pilot-cli session JSON and
`SPIRAL_DIAGNOSTIC_WORKSPACE` to its original workspace. Validate the published
session hash, then explicitly run the ignored test:

```sh
python3 - <<'PY'
import hashlib
import os
from pathlib import Path
session = Path(os.environ["SPIRAL_DIAGNOSTIC_SESSION"])
expected = "276a76d6552ba5c3dd02f82c94f6b8675fccd052ac6e5c60744cc6421aa5172f"
assert hashlib.sha256(session.read_bytes()).hexdigest() == expected
PY
cargo test --locked --bin spiral-coder \
  tui::agent::recovery::resume_tests::native_source_diagnosis_survives_a_restored_continuation \
  -- --ignored --exact --nocapture
```

The original workspace is necessary for native `cwd` and location validation.
No provider requests or candidate edits are made by this regression. The frozen
0/2 and corrected 1/2 r11 outcomes remain historical measurements; this repair
does not retroactively change them or claim a new live maintenance result.

## Integration of the parallel CLI follow-up

PR #48 retains the r12 live CLI repair evidence and nine additional regressions
in `recovery_cli_continuity_tests.rs`. Its parallel latest-edit provenance state
is superseded by the per-target ledger described above, so there is one final
restoration owner. Both regression suites remain executable. Their distinct
names avoid the original add/add test-file conflict.

On 2026-11-01, the original retained r11 session was available locally. Its
published SHA-256 was verified and the explicit ignored native-session test
passed on the combined source. This closes the previously unexecuted replay
check without changing either historical live result. The
[integration receipt](evals/2026-11-01-pr-integration/validation.json) records
the exact tested source hashes and local check logs.
