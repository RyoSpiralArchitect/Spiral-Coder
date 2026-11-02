# Optional helper reuse in the shared task

Run one episode on the r3 executor, with the same candidate source, shared edit order, probes, driver, reference, roles and budgets. Insert only the opt-in reuse paragraph into the shared task. It names no helper and permits a new implementation. The assessor and its gates are unchanged.

Record the original reproduction, verification after the last mutation, native terminal, independent results and scope. For the reuse observation, inspect edits and the final source for calls to an existing helper, a changed definition, a new helper or a duplicate. Distinguish a source change from a suggestion, a search, an intention in a report or untouched baseline code. Note whether an Observer suggestion points to an observed definition or caller, and whether a matching edit follows. Report a missing opportunity separately. This is one observation per condition, and none of the reuse observations affect qualification.

Independently replay r3's retained malformed diffs to test deterministic format handling. A subsequent episode may use the same r4 prompt with only the executor changed, under a separate predeclared pin. Preserve each attempt and keep candidates outside the product.
