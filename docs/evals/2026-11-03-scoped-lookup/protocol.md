# R10. Give the pair a single scoped symbol lookup during repair

Run the first fresh Issue #49 episode on the R9 task baseline. Keep the exact prompt, imports, reference patch, held-out checks, driver, GPT pair and budgets from R9. Freeze and publish the executor and declaration before the first model request. Keep the initial result even if a provider fails, the action under study never occurs, or the task is blocked.

The executor includes three changes relative to the live R9 binary. It enforces the explicit edit list discovered in R9, permits one identifier lookup in a listed file or its immediate directory when reads would otherwise trigger a forced edit, and asks the hosted OpenAI provider to serialize tool calls. Multiple returned calls are still rejected together. Prerequisites, acceptance and independent qualification stay fixed. These changes form one condition, so a score difference cannot identify their individual effects.

Replay the recorded R9 step 13 tool arguments in disposable copies on the previous scoped executor and this executor. Measure whether the native call executes as authored, whether it returns source locations, and whether files change. The fixture supplies its own tool prelude because the retained assistant message is compacted.

In the fresh run record actual Coder wire attempts with the serialization option, batched responses rejected, scoped searches and native results, out-of-scope changes, first executable CLI behavior, package entry, ordinary replay, both final exact checks and the accepted report. Reapply the sealed candidate patch in a new baseline copy and compare its inventory. Report the first failed gate, and only adopt independently qualified work after a separate human review.
