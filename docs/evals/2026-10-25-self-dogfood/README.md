# Cross-family self-dogfood foundation — 2026-10-25

This experiment runs the real Coder and Observer consumers against a complete
Spiral-Coder source snapshot. The runtime binary and evaluator remain outside
the editable candidate. The unit under evaluation is the provider pair; the
operator's reference repair is a feasibility control, never a model outcome.

## Frozen task

Candidate baseline: `9982a68a6978058c75a9a700e7407df5a49f010c`.
The requested repair preserves completed failed-edit diagnosis across session
handoff without clearing a newer failure or the pending original verification.
Allowed artifacts are recovery.rs, edit_failure.rs, recovery_progress.rs and
coupled-work-loop.md. The documentation must actually change. Required native
proof is `cargo test --locked --bin spiral-coder -q` after the final mutation.

Independent checks build the candidate, run at least the original 864 passing
Rust tests (one original ignored case), run a six-case external recovery probe,
and run all seven TUI replay cases with the candidate-built binary. Baseline
qualification requires exactly five probe passes and the named one failure;
reference qualification requires six passes plus every regression check.

The first operator reference passed the six new tests but failed an existing
restoration test. Preflight rejected it before any provider call. The reference
was corrected to preserve the old no-active-recovery branch; 870 tests passed
with one original ignored case. This failed preflight is retained and is not
counted as a model trial.

## Initial live episodes

Executor: `be6b89ad39a816ffe13eda1c26eea0aeaeff5b0e`.
Driver/task source: `9e9fdb1259fd286164ff5eb51137f2c3e68ec0b4`.
Each episode froze a 32-wire-request shared cap, initial Coder slice eight,
1,200-second wall cap, 90-second request timeout and 32,768 Coder output limit.
The native Observer default was 1,200 tokens and temperature 0.2. Luna retains
`reasoning_effort=none`; these are not reasoning-enabled Luna measurements.

| Coder → Observer | Coder / Observer requests | Seconds | Native complete | Independent target probe | Qualified |
| --- | ---: | ---: | --- | --- | --- |
| Luna → Argon | 31 / 1 | 158.169 | No | 5 pass / 1 fail | No |
| Argon → Large 4 | 8 / 2 | 79.649 | No | 5 pass / 1 fail | No |

Both episodes preserved exact role configuration, shared accounting, strict
workspace scope, protected original tests and pinned evidence. Build, existing
Rust tests and TUI replay passed. There were no secret redactions. Raw manifests
were verified locally: 925 files for Luna → Argon and 867 for Argon → Large 4.
[Initial result receipts](initial-results.json) include the original declaration,
run, binary, assessment and raw-manifest hashes.

### Luna → Argon

The candidate made only the [retained one-line source change](initial-luna-argon.patch).
It did not fix the target oracle. Separately, runtime follow-up coercion replaced
a focused model-authored documentation patch with a whole-body anchor containing
the read display's truncation marker. That synthetic patch failed. Subsequent
requested diagnostic reads were rewritten into patches and blocked by reflection,
plan or recovery gates. Argon advised reading the exact documentation range; Luna
accepted, but the requested read did not execute. No explicit verification exec
or accepted done occurred, and documentation remained unchanged.

The inadequate candidate repair and the coercion deadlock are separate findings.
Neither the Observer's appropriate advice nor a passing old regression suite
establishes that the requested source behavior was fixed.

### Argon → Large 4

The initial Coder slice read relevant modules without changing files. Both the
initial Large 4 Observer request and its single permitted format-repair request
returned no usable content with `finish_reason=length`. The runtime retained the
failure and stopped without unbounded retries. This is evidence of an output-cap
problem in this role/configuration, not a provider/model capability verdict.

## Boundaries

No candidate is automatically adopted, committed to the source repository or
merged. Candidate qualification remains distinct from operator review. These
published development tasks are not held-out capability benchmarks. The pair
choices, one attempt per frozen directory, runtime changes and role limits do
not support a causal ranking of models or a numerical estimate of Observer
benefit. The execution boundary is not an OS sandbox; native tool traces are
inspected for accesses outside the candidate before accepting evidence.
