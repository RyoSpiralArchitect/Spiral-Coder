# Replay catalog, dependency-order condition

Use a fresh copy of the r1 candidate baseline and one episode. Insert the two paragraphs in [order-guidance.txt](../../../../benchmarks/self-dogfood-v2/replay-catalog/order-guidance.txt) after the task introduction. Both roles see the shared task. The guidance supplies an edit order and asks the Observer to recommend the next action at that stage. It contains no reference code.

Before any provider calls, compare the preparation with the [r1 declaration](../plan.json). Require byte-identical inputs except for that insertion, the same assembled baseline and protected paths, executor binary, frozen driver, role configuration and budget. Repeat baseline and reference preflight and commit the new declaration digest. Use the original r1 driver for this comparison, including its historical skip reporting.

Use the frozen binary qualification decision. To interpret the two attempts, record these additional observations from native receipts:

- The first file changed by a successful native edit, and the first changed edit to each allowed path. Compare whether the catalog implementation was edited before CLI registration. A missing stage stays missing. A file write alone does not show the code builds or implements the feature.
- The primary suggested tool and target for every recorded Observer handoff, its decision, exact-argument execution, and linked native edit outcome. Count an Observer-linked change only if its matching call actually changed a file.
- The number of automatic check failures after changed edits, fresh native checks, accepted final report, and each external build, behavior, package and stock check. Keep skipped, failed and passed checks distinct.
- Scope, patch replay onto the plain Git source and any handoff failure or budget exhaustion. Retain the first result as it stands.

Any behavioral difference is an observation across two individual provider runs under a shared prompt change. The design does not isolate an Observer-only effect. Candidate adoption still requires all frozen gates and human review of a separate patch.
