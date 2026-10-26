# Visible recovery regression: first live pair

The protected reproducer exposes the defect during native execution, but neither
Coder–Observer pair produced a qualifying repair. These are **two episodes,
42 provider requests, zero adopted candidates**. The [plan](plan.json) was
committed before calls; [selected receipts](results.json) retain the pins, gates,
test summaries, role accounting and native action evidence.

| Coder → Observer | Requests | Native complete | Visible test | Independent probe | Qualified |
| --- | ---: | --- | --- | --- | --- |
| GPT-5.6 Luna → Gemini 4 Argon | 32 (31 + 1) | No | 0 pass / 1 fail | 5 pass / 1 fail | No |
| Gemini 4 Argon → Mistral Large 4 | 10 (8 + 2) | No | 0 pass / 1 fail | 5 pass / 1 fail | No |

## Frozen conditions

This is the [visible task variant](../../../benchmarks/self-dogfood-v1/recovery-handoff-visible/README.md),
not a runtime-only rerun of the earlier task. The original Git source is
`9982a68a6978058c75a9a700e7407df5a49f010c`. Preparation adds one protected test
file and its protected registration; production behavior is unchanged.
The prompt asks the pair to run that test before editing. The visible test is
derived from the positive independent case and is not another independent
observation. The six-case probe and reference repair retain their original bytes.

The executor remains source `5b9315dbfb4fbfc134b5a98ae59e658837f94d6d`, binary
SHA-256 `5279c0f6b8719863bec83f38d47b192574c4ddb52c34c6ff4080eaacff69165c`.
The frozen Python driver is from `187babe5579c560822c418ec5341156a7852cbbb`.
Both pairs retain the 32-request cap, initial Coder slice of 8, 1,200-second wall
limit, 90-second provider timeout, Coder output cap 32,768 and Observer cap 8,192.
Luna retains the existing adapter's `reasoning_effort=none` behavior.

The first preparation revision made no provider calls. A fresh review found that
an aliased probe target could overlap a visible fixture. The validator was fixed,
regression-tested and independently rechecked before preparing revision 2.
Both live runs are their pair's **first attempt**, in fresh revision-2 directories;
no run or assessment was retried in place. The original preparations have no
`started.json` or native run directory.
The no-rerun policy applies to episodes; the existing runtime's one bounded
Observer action-contract repair is included in the declared shared request cap.

For each pair, preflight verified the expected visible one-case failure and
independent five-pass/one-fail baseline, then one-pass and six-pass reference
results. Original regressions passed 864 tests with one existing ignored test,
and TUI replay passed 7/7 on baseline and reference.

## Luna → Argon: the visible failure prevents completion

The pair spent 32 requests in 228.995 seconds. Luna read the visible test before
editing, but did not request the filtered reproducer command. Its later full
`cargo test --locked --bin spiral-coder -q` executed once and failed at the
protected handoff assertion. That failure remained pending; subsequent attempts
to rerun without the required diagnostic read and the incomplete `done` were
blocked. Native completion stayed false.

The [retained patch](luna-argon.patch) changes `recovery.rs` and the documentation.
Its new `match` preserves the original behavior: an already active stage is left
unchanged, including the incorrectly restored Diagnose stage. It is a semantic
no-op for the target defect, despite changing bytes. The final report correctly
states that the regression still fails, but names `recovery_progress.rs` as a
modified file even though that file does not differ in the retained patch.

There was one Observer request after the initial eight Coder requests. It
proposed a ranged read of `recovery_progress.rs`; the exact suggested action was
executed, and its outcome remains unconfirmed (`null`) because native read
receipts do not claim successful repair. The actual test failure occurred in
the later continuation. No further Observer request followed that new failure
before the shared budget was exhausted.

Protocol/evidence refusals also consumed turns: the first response tried five
tools at once, and an early edit omitted the required evidence fields. These are
observations of this pair, not evidence that the visible test caused the extra
requests or an estimate of model ability.

## Argon → Large 4: complete read arguments, incomplete framing

The pair spent 10 requests in 130.240 seconds. After an initial plan-gate refusal,
Argon read the visible test and executed its filtered command. The
command ran exactly one test and failed at the intended assertion. Argon then
read production code, but made no edit.

Both native-observed Observer answers identified the pending post-failure read
and included `read_file` with the protected test path and lines 140–200. The
initial answer begins with `"summary":`; the one permitted repair begins with
`summary":`. Both lack the opening JSON delimiter, so no assist was forwarded.
The pair stopped with 22 requests unspent. Native completion stayed false.

An offline inspection adding only `{` to the first answer, or `{"` to the second,
produces parseable JSON with the same complete read arguments. This inspection
was **not runtime validation or execution**: it did not check the complete native
action contract, did not modify retained output, and did not forward a tool.
The extra top-level `fallback` field remains visible in the recorded field list.
Unlike the prior episode's path-only patch proposal, no search/replace contents
would have to be invented for this read. Whether a bounded framing recovery is
appropriate remains a separate implementation question. Raw HTTP/SSE bytes were
not captured, so this record does not establish where the missing prefix arose.

## Integrity and next questions

Both candidates preserved all protected files, stayed in the four-path edit
scope, and passed independent build, the original Rust regressions and TUI replay.
Both retained valid native receipts, role/configuration binding and request
accounting. Neither hit the wall/output cap or required secret redaction.
The raw manifests were checked against all 942 Luna files and 881 Argon files.
This remains an auditable evaluator boundary, not an OS sandbox.

The next useful questions are concrete:

1. Can the pair return to Observer after a new real verification failure while
   reserving a bounded continuation budget?
2. Can malformed framing retain useful diagnosis without inventing missing
   arguments, hiding transport loss or broadening tool authority?
3. Can the harness distinguish byte-identical edits from changes, and report
   actual changed files accurately? Semantic no-ops still require behavioral tests.

The visible test supplied native evidence and kept these runs incomplete; it did
not repair the defect. No success-rate, causal-effect or model-ranking claim is
supported by this small sample, and no candidate was promoted.
