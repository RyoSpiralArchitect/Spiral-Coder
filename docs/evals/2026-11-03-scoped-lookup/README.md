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

On the prior scoped executor the original R9 step 13 search was still refused. On the R10 executor the same arguments returned matching locations in `src/tui_replay.rs`. The retained excerpt did not include the CLI dispatch. Both disposable workspaces finished with their original contents. [Before](lookup-before.json) · [After](lookup-after.json)

## First full R10 episode

The two `gpt-5.6-luna` roles made 20 wire requests in 179 seconds. Coder made 13, all with serialization requested. Observer made seven. The ledger charged 19 task requests and one transport repair. There were zero multi-call refusals. The pair ran one literal search for `tui_replay` in `src` and changed two permitted files. No out-of-scope file changed. This search occurred early in the run and does not establish that the late progress-gate exception was needed. [Declaration](plan.json) · [Trace measurements](trace-actions.json)

The candidate did not compile. Its new catalog imported `super::select_cases`, which the parent module did not define. It never registered the CLI command or edited the launcher or the usage guide. Independent build and visible checks failed, the stock suite stopped at formatting, and the held-out, package and ordinary-replay probes had no process result. The pair left both required native checks without a passing final receipt. The independent inventory matched when the retained patch was replayed. Qualification remained false. [First result](results.json) · [Exact patch](candidate.patch) · [Plain replay](plain-replay.json) · [Raw receipt hashes](receipts.json)

The native terminal reason was `observer_unavailable`. Both final Observer HTTP streams completed. The first proposed reading 220 lines of the parent module, beyond the 200-line limit. Its single repair returned malformed JSON and still proposed the parent module. The pending native diagnostic required `src/tui_replay/catalog.rs`. Model availability, a valid action and repair of the import are distinct outcomes.

## Isolated Observer repair

The follow-up supplies the current diagnostic path alongside the validation error when the Observer proposes invalid `read_file` arguments. Both original invalid replies remain rejected. The native evaluator rebuilt the repair request from the sealed session. One fresh non-streamed Observer call was declared in advance, with a separate budget and no Coder continuation. Its success criterion requires exactly one eligible, forwardable read of the actual diagnostic path. [Original feedback](observer-feedback-before.json) · [Corrected feedback](observer-feedback-after.json) · [One-call declaration](observer-probe-plan.json)

That first call returned HTTP 200 and named the correct file in its text. A stray quoted field at the end of `response_contract.note` left the envelope invalid. The validator rejected it and forwarded nothing. [First authored reply](observer-probe-reply.txt) · [Wire receipt](observer-probe-wire.json) · [Native verdict](observer-probe-result.json)

The second condition changes one request property, `response_format={"type":"json_object"}`. It has its own one-call cap and the same acceptance criterion. Its message bytes, model, output limit and sampling options match the first request. The runtime now asks for this format on native next-action Observer requests to the hosted OpenAI endpoint. Every reply still goes through the ordinary schema, confidence and diagnostic checks.

The format-enabled call returned a parseable JSON object with the correct single read. It repeated the top-level `response_contract` twice with identical bytes. The strict native parser rejected the duplicate and forwarded nothing. On a disposable control, removing only the final identical member made that same authored read forwardable. Neither live reply passed the criterion or executed a file action. [One-field declaration](json-mode-plan.json) · [Authored reply](json-mode-reply.txt) · [Wire receipt](json-mode-wire.json) · [Native verdict](json-mode-result.json) · [Single-member control](single-contract-control.json)

The next Observer task is to emit exactly one response contract on a fresh held-out repair while continuing to reject conflicting, nested and action-bearing duplicates. The next Coder task is to locate an actual selection definition and caller before importing it, build the module, and return to the native and npm checks. A qualified full episode and a separate self-authored runtime patch are still outstanding.

## Merge review and verification

The first independent reviewer found three paths where ordinary callers could violate the proposed behavior. A buffered Web retry could spend an unrecorded request. A text-only Coder could create an excluded file. An ordinary cached reread could retire the deferred launcher edit despite identical contents. A second review of the fixes identified automatic HTTP redirects and a named shell fence after six files as additional cases.

The corrected Web proxy records each attempted send, including parameter retries and endpoint fallback. In a one-request redirect test the upstream previously received two POSTs for one ledger entry. It now receives one and the ledger records the 307. A named shell fence supplies file data, and the entire batch is checked before any write or command approval. A real native replay rejected an excluded seventh target before creating any of the six allowed files. A permitted named script was written without executing its contents. An unnamed shell command still executed in its own control.

In one native Coder session, an unchanged full reread after the verified R8 launcher edit kept the offer available. Changing the file on disk between the verification and reread retired it. These tests used loopback providers and disposable workspaces. The full R10 run and the two external Observer probes kept their original outcomes. [Control results](review-controls.json)
