# From a working native command to a reviewable self-authored change

R8 demonstrated native command registration, dispatch and real filtered output. Its published candidate missed npm routing and usage documentation. R9 compiled a catalog module but never registered the command, and the runtime steered an edit outside the requested files. In both runs the Coder/Observer pair reported a block. [Runnable R8 example](../../examples/replay-catalog-r8.md) · [R9 traces and controls](../2026-11-03-deferred-edit/README.md)

## What the pair still has to demonstrate

| Step | Work by Spiral-Coder | Evidence for adoption |
| --- | --- | --- |
| Finish the current feature | Implement the catalog, connect the native command and installed npm entry, and document the invocation and selection rules. | A real filtered command returns exactly the expected JSON. All held-out cases, the npm probe and ordinary replay pass. |
| Close out the same patch | Format its own changed files, run both prescribed commands after the last edit, and give a report whose commands, paths and counts match the native record. | An accepted native completion, preserved protected files, passing stock checks and a patch that replays onto the frozen baseline. |
| Change the harness from inside | Give the pair a new, independently frozen runtime defect. Its patch must fix the failing ordinary entry point and preserve a neighboring successful behavior. | A fresh candidate and an untouched control, the independent hidden regression, exact patch replay and human review before integration. |

On the current feature these gates must pass together on one authored candidate. A successful local continuation explains a mechanism. A second untouched task can test whether the pair transfers it. The scored candidate runs in its own copy and the executor stays pinned outside it. [Benchmark contract](../../../benchmarks/self-dogfood-v2/replay-catalog/README.md)

## R10 comparison

One R9 assistant reply contained six native tool calls. All six received refusal results from the single-action governor. In another turn, the progress gate refused a first search for `TuiReplay` after unrelated reads. The R10 executor asks hosted OpenAI to generate one tool action per turn and retains a single scoped search during repair. It also contains the explicit edit list check introduced after R9. [Frozen protocol](protocol.md)
