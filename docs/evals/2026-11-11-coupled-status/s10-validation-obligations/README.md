# S10: rejection rules and final report improve; two independent failures remain

S10 retains the original baseline, visible fixtures, 24 independent cases and
six-round caps. It is a **stronger input condition**, not a matched efficiency
trial: seven original behavior bullets become typed requirement descriptions;
`tests/coupled_status.rs` is a required native artifact; and its exact Cargo test
is the fifth final check. The reference has a separate operator test file that
is not shown to the candidate. All six reference groups pass before calls.
An operator pinning assertion initially used the source spec filename rather
than its frozen `spec.json` name; that preparation error made zero provider calls.

The executor now recognizes the actual unquoted instruction, "Include standalone
lines Before: FAILED and After: PASSED only if supported." Previously this
wording was missed. The regression fails on the old executor and passes after
the correction, including the verbatim S9 summary. Quoted examples, negations
and unsupported outcomes do not gain proof. Shared requirement descriptions
survive Coder/Observer compaction and serialization; runtime messages/model plans
cannot replace the original human contract.

Fresh Luna Coder/Observer execution ran **985.451 seconds**, four rounds,
**103 task requests / 115 wires**: 32/36, 32/35, 32/36 and 7/8. The pair creates
a focused test file, repairs pending accounting, writes both documents, and
executes all five exact checks after the last edit before native done. The
report names all six changed paths and all five commands, gives measured counts,
and authors the required bare Before/After lines. The authored-report gate passes.
The candidate reports 1,148 main Rust passes, two ignored, and two authored tests;
the separate executor's local suite has 1,164 main passes and two ignored.

The independent oracle reaches **23/24**. All six S9 failures now pass: completion
conflict, duplicate task IDs, finalized-ledger symlink, interrupted total conflict,
parent linkage and ledger identity. The remaining failure is **active iteration
with its directory present but the ledger file absent**. `ensure_safe` attempts
`symlink_metadata` before the missing-ledger branch, converting legitimate unknown
usage into an error. Absence, malformed data and a symlink must remain distinct.

The independent stock regression also fails: the original
`config::tests::resolve_api_key_uses_vendor_specific_env_for_gemini` reads None
instead of the test key (1,147 pass / one fail / two ignored). Later stock suites
are not executed after this failure. The same existing test failed during an
in-episode automatic check; a later native full check passed. Neither that
pass nor another diagnostic retry replaces the first independent failure.
The two config tests concurrently mutate the same process environment without
synchronization. This is consistent with interference; 20 separate two-test
diagnostic runs of the original binary did **not** reproduce it, so no frequency
or causal estimate is claimed. [Diagnostic receipts](env-diagnostic-before.json).
A post-run host fix isolates these existing assertions in child processes without
changing provider logic. Its full local suite and 20 two-test diagnostic repetitions
pass ([post-run validation](postrun-validation.json)). It is not part of the frozen
S10 executor or candidate.

Visible, packaged and guide checks pass. Every chain gate except independent
checks passes. **S10 remains unqualified, #82 open, no candidate adopted.** The
remaining authored-test coverage is also limited: its two tests cover a completed
record and duplicate request IDs, not every requested contradiction/path case.
Typed descriptions plus test existence/current execution do not prove semantic
coverage. Independent validation remains necessary.

[Results](results.json) · [Exact candidate patch](candidate.patch) ·
[Context/ledger measurements](context.json) · [Frozen plan](plan.json)

Next work should distinguish missing iteration directory, missing state directory,
missing ledger file, existing malformed ledger and dangling symlink in task-derived
controls. Preserve S10 and the independent oracle; any new visible fixture or
repaired baseline must be declared as a new condition before another live run.
