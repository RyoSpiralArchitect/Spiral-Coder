# Current report advice and workspace ownership

PR #36 keeps generated coupled-run state outside the edited workspace and stops
Observer from restarting verified work after a report-only native refusal.
Two fresh three-provider rounds spent **116 requests** (114 Coder, 2 Observer).
The final round has **3/3 clean file scopes** and **2/3 strict qualified tasks**.
Gemini Lite remains incomplete; this pass does not establish better general
model reliability or better artifact correctness across the panel.

## Frozen protocol

Each episode uses the same model for Coder and Observer, 24 shared requests,
600 seconds, the original `pilot-cli` prompt and seven supplied tests. Gemini
`gemini-3.5-flash-lite`, OpenAI `gpt-4.1-mini`, and Mistral `mistral-small-2603`
remain the weak profiles. Both rounds preserve the prior full input inventory,
matrix, driver, prompts and caps. No provider probes, in-place retries or third
round were run. All six processes exited zero without wall timeout or transport
error; exit status alone is not task qualification.

The [plan](plan.json) precedes preparation and calls. R1 freezes runtime
`9bc7351810611310fb7f043219ad90c93ef71c5a`; the [second-round plan](round2-plan.json)
records the automatic-test cache leak before preparing R2 at
`db152f71a286920c35f1cc107049186e88a6347f`. The earlier post-run report-spacing fix
is included in both binaries. The final runtime differs from R1 only by the
extracted automatic-test launcher applying the same child-cache mapping.

Each round retains the frozen-driver assessment (`001`) and current reassessment
(`002`). The current assessor adds lookup of `run/state/tui-work.json` after an
interrupted new runtime; score definitions remain `selection-paraphrases-v4`
and `accepted-omitted-remaining-v1`. Normal completed report receipts use the
same assessment path. No runtime or cache files are exempted from scope checks.

## Results

| Round | Independent artifacts | Behavior and handoff | Scope only | Strict qualification | Requests |
| --- | ---: | ---: | ---: | ---: | ---: |
| Retained preservation R2 | 3/3 | 2/3 | 0/3 | 0/3 | 56 (not new) |
| R1 | 2/3 | 2/3 | 0/3 | 0/3 | 59 |
| R2 | 2/3 | 2/3 | 3/3 | 2/3 | 57 |

| Model | R1 | R2 |
| --- | --- | --- |
| Gemini Lite | 24 requests (23 Coder + 1 Observer); tests pass, docs unchanged, final exact proof/report incomplete | 24 (23 + 1); case-selection test and docs still fail, no completion |
| OpenAI mini | Qualified in 20 Coder requests; Python caches violate scope | Strict qualified in 18 Coder requests |
| Mistral Small | Qualified in 15 Coder requests; Python caches violate scope | Strict qualified in 15 Coder requests |

R1 removed generated sessions, budget, progress, reflection and proposal state
from the workspace, but all three episodes still created two Python bytecode
files there. Explicit `exec` used the run cache; `run_test_cmd`, invoked after
edits, launched its own child without that mapping. R2 routes this second path
through the same child configuration. No files were deleted from retained R1
workspaces. R2 has no changed path outside each task's editable set, including
ignored and untracked files. This does not sandbox arbitrary command writes or
redirect every language's caches; ordinary interactive TUI persistence is
unchanged.

## Observer and remaining failure modes

The [retained-response replay](retained-observer-replay.json) submits the actual
previous Mistral Small native session and original stale `read_file` advice to
the shared Observer endpoint, with zero provider calls. It now identifies the
report-only refusal, withholds the stale hint/response contract and schedules
the existing bounded repair. The previously refused double-spaced summary now
passes current report validation, so resubmitting it remains possible. Neither
that replay nor the runtime writes a replacement final answer.

The two new live Observer calls belong to incomplete Lite artifact work. They
do **not** exercise the new report-only rejection route. That route has native
TUI replay, browser/shared-validator tests and the retained-response replay;
its live effectiveness is not claimed from these six episodes.

R1 Lite spends requests on plan/think repair and repeated observations, repairs
the source, but leaves the requested documentation and fresh exact check pending.
R2 Lite has patch-anchor/recovery failures and leaves the source's original
selection failure. Observer proposes preserving requested order, conflicting
with the task's suite-order requirement. The assist receipt records `override`;
subsequent repairs are blocked or fail to match, and no completed task is
reported. The bad proposal is not evidence that it caused the candidate's final
source behavior. Priority for a later pass is preserving semantic constraints
through advice and repair, followed by allocating enough budget to docs and
closeout. Native failure, parsing success, advice acceptance and task success
remain separate observations.

## Validation and receipts

[Validation](validation.json) records **864 Rust tests plus one integration**
(one ignored), **39 Python tests**, and **7/7 TUI replay cases**. Tests cover
parallel storage ownership, outside-root resolution including symlinks, receipt
reuse refusal, explicit and edit-triggered Python subprocesses, failure/status
preservation, inherited autosaves, interrupted accounting, stale report advice,
new-task/mutation/failure boundaries and no-tool uncertainty. Two read-only
fork reviews found no actionable issue; project context was restored after each.
R1's automatic-test path omission was found by live execution after the first
review and then repaired and reviewed separately.

[Comparison](comparison.json) verifies **1,905 manifest entries** across the
retained baseline and both versions of each new round. Latest inventories contain
**1,016 entries** (111 retained + 456 R1 + 449 R2). Assessment/binary hashes,
plan-before-call ordering and frozen-input equality are checked. Public
receipts are scanned against the three current provider credential values and
opaque continuation fields before publication; full native sessions stay local.

The [first CI infrastructure failure](ci-first-failure.json) records the Intel
macOS runner's DNS failure downloading Rust before compilation. A single-job
retry was refused while the workflow was active; a distinct later source head
runs its own CI. Final merge status is tracked on the PR, separately from this
runtime evidence.
