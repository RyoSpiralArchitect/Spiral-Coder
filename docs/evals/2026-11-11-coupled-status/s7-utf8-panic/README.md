# S7: interrupted by an Observer digest panic

S7 changed the child-iteration continuation to state the actual fresh allowance
and ask for a concrete next action. Visible-test failures additionally expose
the current state, input manifest and expected output. Assertions, independent
oracle, prompt, reference, original baseline and six-round caps are unchanged.
The two changes do not support a matched causal comparison with S6.

The fresh Luna pair stopped after **60.326 seconds, nine task requests / nine
wires** (seven Coder, two Observer). Its first source edit introduced the command
enum before its module/dispatch existed. While preparing the resulting compiler
failure for the Observer, the executor panicked at `meta_digest_text` because a
byte-limit truncation cut through a UTF-8 character.

No iteration finalized, so the manifest correctly retains an active first round
and zero finalized totals. The nine requests are read from the durable pending
ledger; they are not zero usage. There is no terminal native report or final
chain proof. The frozen candidate fails to build; dependent binary/oracle checks
are **unrun**, not 0/24. This run is unqualified and no candidate is adopted.

The interrupted assessment verifies the original seal, scope, protected files,
pending ledger budget, and unchanged candidate around the same independent check
runner. It does not use the completed-chain assessor or invent missing reports.
[Results](results.json) · [Sealed patch](candidate.patch) · [Context](context.json)

A native regression reproduces the panic on old code. The correction walks back
to a UTF-8 boundary while preserving the existing byte and line caps. It covers
2-, 3- and 4-byte scalars, the minimum cap, ASCII and the full Observer failure
packet. [Red/green controls](../utf8-recovery.json). S8 is a separately declared
fresh run with S7's exact inputs and this executor correction.
