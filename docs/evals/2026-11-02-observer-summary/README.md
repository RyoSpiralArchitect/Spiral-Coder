# When R6 repeated its Observer summary

R6 stopped on a conflicting answer followed by an explicit repair that repeated the same display string. Replaying the exact two replies against R6's sealed final native session separates framing from action eligibility. The prior executor rejected both. With the new rule, the conflicting first reply still fails, and the second produces exactly the same envelope, Coder hint and response contract as a one-summary control. The ordinary TUI replay queues the same check ID and retains both original byte strings. Neither replay executes any action or makes any provider calls.

| Sealed R6 reply | Prior | With summary recovery |
| --- | --- | --- |
| Different values for blocker and quickest check | Rejected | Rejected; no hint |
| Two byte-identical summary strings | Rejected | Advisory to run `exec(check_id=required-1)` |
| One-summary control derived from the second reply | Advisory | Same advisory |

In this session `required-1` names the pending visible CLI test. The current recovery still asks for the exact Cargo retest (`required-2`); a successful visible check would not discharge it. The recorded proposal was eligible as advice, but that does not make its ordering optimal, waive an outstanding check or supply a result.

[Old native evaluator](before.json) · [New native evaluator](after.json) · [TUI raw-byte checks](tui-replay.json) · [Local replay script](replay_retained.py) · [Frozen live protocol](protocol.md)
