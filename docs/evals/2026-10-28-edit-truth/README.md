# Edit truth and exact check selections

Follow-up to the [budget/rhythm episodes](../2026-10-27-budget-rhythm/README.md).
Native file tools now report identical content as `unchanged`; those operations
cannot clear failures, advance repair, trigger automatic checks or count as edits.
Coder and Observer share this outcome. Declared checks can be selected by ID and
are resolved to the exact command at the task root, through normal approval and
proof gates. Signed-provider calls retain their full-command path.

The [predeclared plan](plan.json) freezes two diagnostic episodes, one for each
Luna/Large direction, with at most 72 wire requests in total. There is no concurrent
old-runtime arm and no causal improvement claim. The inherited independent oracle,
visible reproducer, reference patch, original 864-test floor, scope restrictions,
native completion and authored-report requirements remain unchanged. No candidate
adoption is authorized. Both baseline/reference preflights passed before calls;
their pins and check outcomes are in [preflight.json](preflight.json).

## Deterministic validation

The [validation receipt](validation.json) records 970 Rust tests (one ignored),
one integration test, 79 Python tests, 85 Web/npm tests, 12 TUI replays and 14
repo-map cases. The local UI/server smoke check also passed. Mock-provider tests
exercise the real Coder/TUI loop for unchanged writes, patches and diffs, selected
checks, root-directory normalization, approval and refusal. The new TUI replay
checks that an Observer check-ID suggestion reaches the Coder hint.

Fresh fork review found four regressions, each reproduced before repair:

- A newer direct or aliased check failure did not supersede a selected success.
- Python evaluators counted unchanged successful operations as edits.
- Unchanged operations cleared repeated-observation pressure.
- Observer receipts lost resolved selections when approval was refused or pending.

The broader repo-map check also caught a search regression: several partial
matches in the new tests outranked the complete production identifier. Spaced
queries now receive the existing whole-symbol match weight for snake-case names;
a focused ranking regression and all 14 existing cases pass without changing the
evaluation criteria. An additional strict Clippy attempt reported style warnings;
it is not a required repository gate and is not claimed clean.

## Live outcomes

The two frozen episodes used **26 wire requests** and produced **0/2 qualified
repairs**. Both selected `reproduce` and executed the exact protected failing
test. Neither made a byte-level edit. The executor source is
`e3a6c228a09f01db0a24d06ab0efb6c3afb5aa57`; its binary hash is pinned in
[executor.json](executor.json). Full gates, ledgers and receipt hashes are in
[results.json](results.json).

| Coder → Observer | Wire calls (C + O) | Outcome | Contiguous development stage |
| --- | ---: | --- | ---: |
| Luna 5.6 → Mistral Large 4 | 14 (9 + 5) | One unchanged patch; Observer response body interrupted near the 90-second request limit | 1/6 |
| Mistral Large 4 → Luna 5.6 | 12 (8 + 4) | No edit; Observer's invalid read range repeated after its one repair | 1/6 |

The Luna episode's identical patch is now explicitly `unchanged`, with
`native_edit=false`, `actual_edit=false`, no mutation step and the required check
still pending. A separate attempted patch was refused for missing evidence.
The Large episode selected the same reproducer through plan/diagnostic refusals
before one real execution; selection alone supplied no proof. It also attempted
three simultaneous reads, which were refused. Its final Observer proposal and
repair both requested lines 100–312 (213 lines), exceeding the 200-line ceiling.

Both final artifacts build, pass the original 864 tests and frozen seven-case
candidate TUI replay, but still fail the visible regression (0/1) and independent
oracle (5/6). The required docs change, fresh full native check and accepted
completion report are absent. All **1,789** raw entries match their sealed hashes,
sizes and recorded modes; no secret redaction occurred, and scope/protected files
remain intact. The retained [Luna patch](luna-large.patch) and
[Large patch](large-luna.patch) are empty. No attempt was replaced or rerun.

## Findings repaired after the frozen runs

The Luna episode recorded two transport charges for `finish_reason=length` with
no emitted content. They were **not pure authored-envelope violations**. The
frozen assessor's `budget_accounting.valid=true` verifies structural ledger
arithmetic, but does not catch this semantic misclassification. The supplemental
`transport_eligibility_audit` in the results marks that episode nonconforming;
the original ledger and assessment remain unchanged. These two charges cannot
be presented as successful use of the intended framing-repair policy.

The follow-up preserves `output_limited`, `output_filtered`, `empty_response`
and unknown provider-stop statuses for OpenAI-compatible streams. Their requests
stay in the task budget, even if the resulting text also fails JSON validation.
A terminal UI stream event does not make an authored answer complete. A failing
before-fix coupled test reproduces the extra Observer repair at exhausted task
capacity; the fix preserves the two-call cap for both empty and partial limited
answers. EOF/normal/limited/filtered/unknown-stop cases also exercise native
stream accounting and transport eligibility.

The range failure previously collapsed to a generic schema error. Observer
repair now receives the native range validator's concrete 200-line limit. The
invalid suggestion remains refused; a newly valid 200-line range can be queued.
A TUI/browser regression reproduces the missing diagnostic before the fix, and
the repository replay includes this refusal path.

These follow-ups are **outside the frozen live executable**. They pass 972 Rust
tests plus integration (one ignored), all 13 current TUI replays and all 14
repo-map checks. Their live effect is unmeasured; the two recorded outcomes above
remain the results of the earlier executor.

The remaining work is turning correct reproduction/diagnosis into a real,
complete source repair, making valid Observer actions recover reliably, and
separately investigating provider output exhaustion and response-body timeouts.
These runs demonstrate the new edit/check mechanics in use, not improved repair
success or general self-improvement capability.
