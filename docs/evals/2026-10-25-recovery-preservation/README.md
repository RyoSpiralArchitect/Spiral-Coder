# Recovery handoffs and source preservation (2026-10-25)

This adaptive pass investigates two retained failures: a successful automatic
check being forgotten on an Observer handoff, and a whole-file rewrite dropping
existing runner helpers. It also checks whether deterministic format recovery
misreads examples as protocol fields. Coder and Observer remain one system with
one model and one shared budget for each episode.

The [retained analysis](retained-analysis.json) records a rewrite that removed or
changed 124/149 distinct nonblank lines and lost `run_bounded` and `preserved`.
The first runtime candidate corrects matching-command recovery memory, routes
high-churn native source overwrites to explicit patches, and protects JSON
examples inside legacy fields. The loss check is a mechanical editing safeguard,
not a semantic preservation oracle; small harmful edits and explicit patches
remain possible.

The [plan](plan.json) bounds this pass to two fresh three-model rounds, at most
144 new requests. Each episode has a 24-request shared cap and 600-second wall
limit. The pilot-cli prompt, original seven tests, complete input inventory,
provider matrix and frozen driver match the
[retained format-recovery pass](../2026-10-25-format-recovery/README.md).
The primary assessor is unchanged: `selection-paraphrases-v4` plus
`accepted-omitted-remaining-v1`. Old frozen-driver assessments are also retained.
No score definition, test or file-scope exemption is changed in this pass.

Round 1 independently passes all three candidates. Lite and Small qualify;
mini exhausts its budget after twelve authored-report refusals, eleven of which
already contain exact outcome rows after an unsupported positive introduction.
The [second-round plan](round2-plan.json) records corrections to this report
boundary and a full-range-read false refusal, plus shared report validation for
Observer done advice and actionable bounded repair feedback. No API retry or
in-place continuation is used to alter earlier outcomes.

## Frozen outcomes

| Profile (same model in both roles) | Retained format R2 | New R1 | New R2 |
|---|---|---|---|
| Gemini 3.5 Flash-Lite | Artifacts pass, final proof missing; 24 requests | Qualified; 23 (22 Coder + 1 Observer) | Qualified; 16 Coder |
| GPT-4.1 mini | Qualified; 23 (21 + 2) | Artifacts pass, report refused; 24 (23 + 1) | Qualified; 16 Coder |
| Mistral Small 2603 | Helpers lost, incomplete; 24 (23 + 1) | Qualified; 15 Coder | Artifacts pass, report refused; 24 (23 + 1) |

The six fresh episodes use **118 requests** (62 + 56), with no transport errors,
wall-limit termination, probe, in-place retry or third round. Independent
artifacts pass **2/3 retained → 3/3 R1 → 3/3 R2**. Qualified native completion is
**1/3 → 2/3 → 2/3**. Strict all-file scope remains **0/3 in every round**: generated
runtime state and Python caches are still counted, without exemptions. All
processes exit zero, which is not used as a substitute for task qualification.
See [comparison.json](comparison.json) and the unchanged per-round assessments.

## What the new traces establish

- Lite R1 passes the final exact check after its Observer handoff and closes.
  The former exec-only memory reconstruction has a deterministic regression test
  as well. This trajectory supports the repair, without isolating its causal
  effect from other changes and generation variation.
- No fresh episode drops `run_bounded` or `preserved`, and all seven original
  tests pass independently. **No new episode attempts a high-churn source
  overwrite**, so none triggers the safeguard. Its rejection, unchanged-file
  behavior and intentional-patch escape are verified offline; its live benefit
  and false-positive cost remain unmeasured.
- Mini R1 has twelve final-report refusals; eleven already contain both exact
  rows after a positive verification introduction. R2 completes with one report
  correction. Its full three-line document range is now represented as a
  complete native read; the following overwrite succeeds. Partial, truncated
  and changed-after-read cases retain negative HTTP/handoff coverage.
- Three original successful Observer responses are retained across the fresh
  rounds, all standalone JSON with object arguments. No wrapper extraction or
  string-argument recovery is observed in these responses. Successful parsing
  alone does not establish the quality of advice.
- Mini R1 exposes the missing authored-report check for Observer done advice.
  The second runtime candidate shares that validator with Coder and prioritizes
  actionable missing-field feedback in its bounded repair prompt. R2's only
  Observer answer is a read suggestion, so this new done-advice rejection path
  is covered offline but not exercised live.
- Small R2 has passed source/docs checks and fresh native verification. Three
  done attempts fail solely because `After:` is followed by two spaces. The
  [post-run audit](post-run-audit.json) corrects presentation whitespace while
  preserving exact command bytes and negative/quoted-field boundaries. That
  final patch has **zero further provider calls** and does not change Small's
  frozen incomplete outcome.

The Small R2 Observer suggests inspecting source before repair even though
repair and verification are already recorded. It is valid read advice with stale
reasoning, not a formatting failure. Prioritizing current pending obligations,
early plan/protocol stalls, generated-state ownership and semantic preservation
remain follow-up work. Small also claims no other files changed; strict inventory
scoring contradicts that broader claim and remains separate from native proof.

## Validation and limits

The final runtime patch passes 845 Rust tests (one ignored), one integration
test; the final integrated catalog build passes 854 plus one integration (one
ignored), with TUI replay 7/7. Coverage includes
matching versus unrelated auto-tests, denied/orphan results, task boundaries,
legacy JSON examples, loss-guard refusal before mutation, explicit deletions,
complete/partial/changed reads through HTTP and Observer continuation, authored
Observer reports, and exact-command whitespace. Python benchmark tests remain
38/38. Exact build identities, test logs and public scan results are in
[validation.json](validation.json).

Raw episodes remain local at `capacity-20261025-preservation-r1` and `-r2` under
the Spiral-Coder cache. Public manifests bind the raw files, frozen binary,
inputs, native transcript, process receipts and assessor snapshots by hashes.
Both assessment versions for each new round and the retained baseline's latest
manifest were verified: **549 file entries** in total, **343** across latest
inventories. Plans precede preparation and first calls; all frozen inventories,
parameters, driver and corrected-assessor hashes agree with the retained run.

These are adaptive development episodes on one task, not a held-out panel,
randomized ablation, model ranking or stable reliability estimate. New report
and source-edit safeguards introduce no synthetic execution/acceptance evidence.
The post-run spacing fix is regression-tested, not live-qualified. No npm
publication or merge is included in this pass.
