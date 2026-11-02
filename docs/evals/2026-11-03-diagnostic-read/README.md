# From an Observer diagnosis to an actual Coder read

On the eleventh full [catalog task](../../../benchmarks/self-dogfood-v2/replay-catalog),
three hosted Observer requests used the diagnostic schema. All three returned
one response contract and one suggestion, passed native validation, and received
an authored accept. Two exact suggestions have a matching executed native read:
the initially failing visible probe and the newly written catalog source.
The other suggestion does not have an exact argument-matched successful read.
A successful schema response alone is not evidence that the Coder acted.
[Request and action receipts](trace-handoffs.json).

No reply needed transport credit. In 320 seconds the pair spent 24 Coder and
8 Observer requests. It introduced its own selection helper, implemented the
catalog, and registered and dispatched the CLI. The independent build, visible
filtered listing, ordinary replay and 10 of 11 hidden catalog checks passed.
The missing usage guide failed the remaining hidden check; the installed npm
entry still forwarded to the TUI. Stock checks stopped at rustfmt. The ordinary
runner also retained its inline selector, so the new helper did not establish
a shared implementation. Both required final native checks were missing. No
protected file changed, the patch reapplied exactly, and the run ended blocked
and unqualified. [First result](results.json) · [Patch](candidate.patch) ·
[Plain replay](plain-replay.json) · [Raw hashes](receipts.json).

On the frozen baseline, a real file-scoped search for load_spec now shows
declaration line 582 and call-like locations 153 and 568. A search for
select_cases reports no exact spelling in that file. Both executors returned
identical literal search rows. This controlled lookup does not attribute the
live helper implementation to the new hint. [Before](helper-before.json) ·
[After](helper-after.json). The original malformed and duplicate R10 replies,
conflicting or nested duplicates, wrong targets, and low-confidence controls
remain unforwarded as tested. [Native controls](controls.json).

After its dispatch edit passed an automatic Cargo test, the pair proposed the
configured Cargo command twice while the recovery gate still required the
original visible Python command. Two launcher edits were also refused. Finally
the pair selected the right check but attached a replacement plan that did not
pass alignment. No refused command or edit counts. [R11 predeclaration](plan.json)
· [Follow-up protocol](r12/protocol.md).
