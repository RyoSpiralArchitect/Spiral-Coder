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

## Runtime repairs between phases

The original episodes and their failures remain unchanged. Follow-up coercion
now preserves recovery diagnosis reads, model-authored required-path edits and
honest incomplete reports. A synthetic whole-file replacement requires a
complete native snapshot with matching byte/line counts and path; displayed
truncation or a partial range is not an edit anchor. Cached absolute/relative
path aliases are rebound only after safe-path identity validation.

The Observer now has an explicit output-token setting independent of the Coder.
The final two episodes use 8,192 Observer tokens instead of 1,200. Their executor
and driver come from `f4f9d812bd63418316ad82f07b03c56ed822c864`, with binary SHA-256
`bca06da9e088ea9c09c6fb5f3af92de4082d87d0402f50f2a854407ba2009935`.
Both pairs retain the original candidate baseline, task, probe, reference,
32-request cap, eight-request initial Coder slice and other role settings.
A prepared intermediate executor was superseded before any model calls after
review found the cached path-alias defect; it is not another live trial.

## Final live episodes

Both baseline/reference preflights passed before the final episodes. These are
fresh attempts in new directories, not retries that overwrite initial evidence.

| Coder → Observer | Coder / Observer requests | Seconds | Native complete | Independent target probe | Qualified |
| --- | ---: | ---: | --- | --- | --- |
| Luna → Argon | 25 / 1 | 225.624 | Yes | 5 pass / 1 fail | No |
| Argon → Large 4 | 28 / 4 | 645.398 | No | 5 pass / 1 fail | No |

Across all four live episodes, 100 provider requests were reserved. No candidate
qualified. Build, existing Rust tests and candidate TUI replay passed in both
final episodes. Raw manifests were verified locally: 934 files for Luna → Argon
and 876 for Argon → Large 4. [Final result receipts](final-results.json) retain
the external pins, effective role settings, gates and hashes. All four episodes
had zero secret redactions and preserved strict scope. Recorded native tool
accesses stayed inside the candidate; that is not proof of OS isolation.

### Luna → Argon: native completion without a correct repair

All qualification gates passed except the independent checks. The candidate
updated the required documentation, ran the exact native test command after its
last mutation and obtained accepted `done` with no pending checks. Observer
advice to read the documentation range was executed. Its `succeeded` field
remains null; this does not establish successful semantic advice.

The initial read-to-patch loop disappeared: native actions changed from six
executed / 25 blocked to 20 executed / five blocked. The final executed count
includes accepted `done`; 19 actions dispatched tools. There were eight reads,
four successful patches, one failed diff and six execs. This is descriptive
within-task evidence, not an improvement rate attributable to one change.

The [retained candidate patch](final-luna-argon.patch) adds 12 source lines and
eight documentation lines. It repeats existing focus restoration but leaves the
cross-file diagnosis loss in place. The external probe still gets Diagnose
where Fix is required. Existing tests, build and candidate TUI replay pass, which
is insufficient to establish the requested repair. The candidate added no new
regression test. This patch is not adopted.

### Argon → Large 4: repeated observation without an edit

The final candidate patch is empty. There were 20 executed file reads, two
searches and two accepted incomplete `done` actions, plus two blocked actions.
Both done reports explicitly retained all four acceptance items, so the runtime
correctly kept `completed=false` and the required check pending. No edit or exec
was attempted. One Coder response-body read timed out after about 90 seconds;
the episode itself stayed within its wall limit and exited normally.

Four Observer requests comprised three next-action attempts and one explicit
format repair. The first two usable suggestions emphasized documentation and
verification despite the absence of a source edit. The third recognized the
missing repair but returned truncated JSON; its repair produced a structurally
valid source patch proposal. That proposal was never independently validated. All three forwarded suggestions were overridden. Their
`executed=true` receipts mean that the subsequent Coder continuation ran, not
that the suggested documentation read or patch ran. Native traces record neither.

Repeated source reads persisted despite 23 progress-policy prompt emissions.
The final authored report also said documentation was inspected, but no native
documentation read supports that claim. These are separate failures in diagnosis,
action selection and report grounding; raising the Observer token cap did not
resolve them. Shared request accounting stopped the episode at 32 calls.

### Remaining harness findings

Two final Luna `cargo test 2>&1` executions returned exit code zero with passing
Rust summaries but were classified as failures by the native harness. The
success-output heuristic in `src/tui/agent.rs` scans broad substrings such as
`exception`, also present in passing test names. A separately selected displayed
diagnostic named a passing `failed_assertion ... ok` case. The full stdout was
not retained, so the exact first triggering substring cannot be established.
The false failure was followed by recovery and one comment-only edit; the
required quiet command later passed.

The next bounded task should fix success-output classification with both real
error controls and passing test-name controls. A separate next repair task
should require a visible behavioral regression that fails on the baseline
before accepting the model's proposed repair, while keeping independent
negative controls. The Argon trace additionally calls for a bounded transition
from repeated observation to a concrete patch, explicit unedited-source/unverified-acceptance
evidence in the Observer packet, and action-level receipts that distinguish
executing a continuation from executing an Observer suggestion. Neither
changing this episode's frozen oracle nor repeatedly spending more requests
can substitute for that evidence.

## Foundation validation and review

The final runtime passed 889 Rust unit tests (one pre-existing ignored case),
one integration smoke test and seven of seven deterministic TUI replay cases.
The new driver has 22 passing offline tests; the existing Python suite has 39.
Linux and macOS quality checks, five native package builds and package assembly
passed for the executor source commit above. [Local validation](runtime-validation.json)
and [source CI receipts](runtime-ci.json) retain the counts, hashes and job
links. These checks validate the harness
implementation, not the unqualified candidate repair.

Fresh fork review identified required-document enforcement, malformed-report
accounting, whitespace-normalized credential redaction and cached path-alias
handling. All four findings were fixed; focused re-review found no remaining
actionable finding. Raw receipts stay local; published results contain selected
accounting/configuration/gate facts and immutable hashes, not provider opaque
continuation fields or credentials.

## Boundaries

No candidate is automatically adopted, committed to the source repository or
merged. Candidate qualification remains distinct from operator review. These
published development tasks are not held-out capability benchmarks. The pair
choices, one attempt per frozen directory, runtime changes and role limits do
not support a causal ranking of models or a numerical estimate of Observer
benefit. The execution boundary is not an OS sandbox; native tool traces are
inspected for accesses outside the candidate before accepting evidence.
