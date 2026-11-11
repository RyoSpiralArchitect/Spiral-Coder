# Read-only replay catalog

A same-model Coder/Observer pair adds a CLI feature to the full Spiral-Coder repository at `55351155d73675605e478e0518cc09279ae02b80`. The executor is separately built from that commit and copied before the run. The assembled baseline adds one protected, visible smoke script. The probe and reference live outside the candidate.

1. Build the executor from the source pin. Keep its binary outside the shared validation cache.
2. Run `python3 -BS benchmarks/self-dogfood-v2/replay-catalog/materialize.py /absolute/new-inputs` from the driver checkout. It expands the compact reference patch and checks its changed paths against the task declaration.
3. Follow the [prepare/run/assess procedure](../../../docs/self-dogfood.md#prepare-run-assess) using `new-inputs/luna-luna.json`, the pin above for both source revisions, and a fresh external output directory. Before spending requests, retain the emitted declaration digest and commit the preflight record.
4. Review the actual native actions, final report, independent checks and patch before any separate adoption PR.

The stock CLI must lack the command and fail one visible and eleven held-out catalog checks for that reason. The packaged spiral entry also has to forward the command. The held-out package test assembles a private host package, checks the bundled binary digest and runs its actual bin entry. The reference must pass all three suites, the original full check and ordinary replay controls. The stock check verifies the original 1,036 Rust test identities and statuses (including the two pre-existing ignores), Python, Web and npm tests, all standard replays, repo-map and server smoke. Its outputs stay in the disposable verifier.

Version 2 additionally requires a native failure of the visible command before the first changed edit, a truthful authored before/after report naming changed files and commands, regular source changes and exact patch replay onto the assembled baseline. Read-only probes inventory the caller, project and designated replay storage including empty directories. All roles, checks, scope, budgets and input bytes are frozen in one declaration. A run can qualify only for this task with this pair and budget. Human review checks the behavioral explanation, numerical claims and tool trace before adoption.

The [first episode](../../../docs/evals/2026-11-01-replay-catalog/README.md) exhausted its declared budget. The retained partial patch reproduces the missing-function build failure.

For the [second condition](../../../docs/evals/2026-11-01-replay-catalog/order-r2/protocol.md), pass `--order-guidance` to `materialize.py`. This inserts two paragraphs into the shared prompt. Prepare with the first episode's frozen driver and executor to keep the original scoring and execution conditions.

For the [helper reuse condition](../../../docs/evals/2026-11-01-replay-catalog/reuse-r4/protocol.md), select both `--order-guidance` and `--reuse-guidance`. The extra paragraph asks Coder and Observer to consider existing definitions and leaves implementation choices open.

The [R8 benchmark example](../../../docs/examples/replay-catalog-r8.md) rebuilds the first passing native listing and reproduces the remaining npm and documentation failures without model credentials.

R9 uses the same task and request budget. Its record separates a scripted recovery of R8's launcher edit, the fresh pair's blocked run, and a native replay of the out-of-scope edit discovered in R9. [Ninth run and follow-up](../../../docs/evals/2026-11-03-deferred-edit/README.md)

R10 records a scoped symbol search, serialized Coder calls and an unresolved catalog import. The record lists the remaining feature, verification and harness-change tasks for a reviewable self-authored patch. [Tenth run and remaining work](../../../docs/evals/2026-11-03-scoped-lookup/README.md)

[R11 diagnostic handoffs](../../../docs/evals/2026-11-03-diagnostic-read/README.md)
record three authored Observer responses, two exact native reads, and distinguish the
working catalog command from the unqualified full candidate.

[R12](../../../docs/evals/2026-11-03-diagnostic-read/r12/README.md) ran the original failed command after two edits and retained its final compile failure.

The [route and handoff experiments](../../../docs/evals/2026-11-03-catalog-route/README.md) retain the original-tree run and the interrupted attempt. The [checkpoint repair](../../../docs/evals/2026-11-03-catalog-route/checkpoint/README.md) composes actual model patches over the original source and runs its original six external checks. The published examples have a separate [executable guide audit](../../../docs/evals/2026-11-03-catalog-route/guide/intent/README.md). [Try the reviewed feature without model credentials](../../../docs/evals/2026-11-03-catalog-route/guide/intent/reviewed/README.md).

[R15's post-edit recheck](../../../docs/evals/2026-11-03-catalog-route/r15/README.md)
and [R16's two unexecuted edits](../../../docs/evals/2026-11-03-catalog-route/r16/README.md)
use the same original source, prompt and budget. R16 includes a key-free
control that builds each intermediate state and exercises the candidate CLI.

The [R16 continuation and fresh R17](../../../docs/evals/2026-11-03-strategy-memory/README.md)
compare returning to a held proposal after restart with building from the
original tree. The final Observer handoff has its own one-request record.

[R18 on the original source](../../../docs/evals/2026-11-03-strategy-memory/r18/README.md)
tracks all ten Observer decisions against the remaining Coder turns and
preserves the unresolved native dispatch and absent final report.

[Bounded iterations after R18](../../../docs/evals/2026-11-03-coupled-iterations/README.md)
track the preserved dispatch repair, the guide-to-report sequence, and a
separate operator acceptance task on the same working source.

[Fresh catalog iterations](../../../docs/evals/2026-11-04-fresh-iterations/README.md) record R19 on the original task and a separate prospective condition with public diagnostic and guide checks. Both start from the original repository copy. The assisted repair has separate data and a frozen checksum for the model-authored test.

[R22 on merged main](../../../docs/evals/2026-11-11-catalog-on-main/README.md)
reached an authored report in iteration three and passed all six independent
groups. Its frozen reporting and transport eligibility gates remain failed.

The [R23 efficiency comparison](../../../docs/evals/2026-11-11-catalog-efficiency/README.md)
retains two saved-state report probes and a fresh original-tree run. Check IDs
reduce evidence retyping, while the fresh run spends 96 task requests and 106
wires and ends before final proof. All six independent groups pass. The trace
separates late artifact delivery, repeated builds and report validation, and
preserves a guide diagnostic control for a future run.

[R24 with context compression](../../../docs/evals/2026-11-11-context-compression/long-run/README.md)
uses the same R23 task, original tree and limits. The Luna pair completes in
62 task requests and 70 wires across two iterations. All final native and
independent checks pass. The frozen transport eligibility gate remains failed.

[R25 on merged main](../../../docs/evals/2026-11-11-context-compression/r25/README.md)
qualifies under the frozen three-round contract with 73 task requests and
80 wires. All native gates, six independent groups and the packaged guide pass.
It also supplies sealed states for the next model-free compression comparison.
