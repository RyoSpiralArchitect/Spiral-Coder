# When R6 repeated its Observer summary

R6 stopped on a conflicting answer followed by an explicit repair that repeated the same display string. Replaying the exact two replies against R6's sealed final native session separates framing from action eligibility. The prior executor rejected both. With the new rule, the conflicting first reply still fails, and the second produces exactly the same envelope, Coder hint and response contract as a one-summary control. The ordinary TUI replay queues the same check ID and retains both original byte strings. Neither replay executes any action or makes any provider calls.

| Sealed R6 reply | Prior | With summary recovery |
| --- | --- | --- |
| Different values for blocker and quickest check | Rejected | Rejected; no hint |
| Two byte-identical summary strings | Rejected | Advisory to run `exec(check_id=required-1)` |
| One-summary control derived from the second reply | Advisory | Same advisory |

In this session `required-1` names the pending visible CLI test. The current recovery still asks for the exact Cargo retest (`required-2`); a successful visible check would not discharge it. The recorded proposal was eligible as advice, but that does not make its ordering optimal, waive an outstanding check or supply a result.

[Old native evaluator](before.json) · [New native evaluator](after.json) · [TUI raw-byte checks](tui-replay.json) · [Local replay script](replay_retained.py) · [Frozen live protocol](protocol.md)

To replay the two public response fixtures, run `cargo run --locked -- tui-replay --spec .spiral-coder/tui_replay.json --filter r6-duplicate`. No provider credentials are needed. The retained-session script additionally needs the original sealed R6 directory.

A separate [native loop test](../../../src/tui/events/coupled_run/tests/observer_summary.rs) delivers a conflicting answer and its identical-summary repair in the same assist. The ledger charges the first Observer request to transport and the second to the task. Its scripted Coder sees the advisory on the wire; the fixture stops at delivery, with no executed tool and no completion.

## R7 live: the exact Rust retest passed, the CLI did not

The same-model GPT pair used 32 task requests (24 Coder, eight Observer), zero transport requests, and exhausted its allowance. No identical summary was observed. Both the full native episode and the independent qualification remained incomplete. This fresh episode is not evidence that summary recovery advanced the task.

| Gate | First R7 outcome |
| --- | --- |
| Reproduction and edits | The protected visible test reproduced the originally missing command. The pair registered and wrote the catalog module. Two in-scope files changed. |
| Fresh native verification | After the write, the automatic unit test listed the new source at step 16. The required exact Cargo test passed at step 18; its own dependency observation was unknown. The post-edit visible check was selected, but the plan gate blocked execution. |
| User-facing behavior | The independent product build and ordinary replay control passed. Public help still lacked the command. The visible case, all 11 held-out cases and the npm entry test failed. The stock suite stopped at formatting. |
| Terminal report | The Coder tried to report blocked with no completed criteria but cited the Cargo test against two remaining criteria; the runtime rejected it. The last Observer proposed `patch_file` with only a path. That action was not forwarded and no task slot remained for an explicit repair. |
| Reproducibility | The exact patch recreated all candidate files on the original plain snapshot without the injected visible test. No protected file changed, no fixture was adopted, and the listing feature remains open. |

One native routing rewrite also redirected a read of `scripts/catalog_visible.py` to `src/agent_session.rs`; a later call read the intended file. A proposed edit to the CLI inventory was blocked before mutation because its evidence block lacked `target_files`. None of these events supplies a passing public command.

[Declaration](plan.json) · [Preflight](preflight.json) · [Independent and native results](results.json) · [Observer responses and gate outcomes](observer-observations.json) · [Candidate patch](candidate.patch) · [Plain-source replay](plain-replay.json) · [Sealed digests](receipts.json)

## Does adding the advertised name expose the command?

After qualification, an operator rebuilt two disposable copies: the sealed R7 candidate, then the same source plus only the inventory row proposed at step 22. Both built. The second binary listed the command in `inventory commands --json`, but neither binary listed it in top-level help. On the second binary, `tui-replay-list --help` exited zero and printed the **exact same bytes as global help**. A real filtered listing exited 2 on `--filter` with empty stdout. The original model edit was never executed; this control tests its intended row, not whether its diff framing would be accepted.

The next useful handoff is to trace a working sibling command from argument parsing to dispatch, then try the requested invocation. This gives the two roles a way to distinguish descriptive names from executable behavior without requiring a particular helper or implementation.

[Two rebuilt binaries](inventory-control.json) · [Actual invocation on the inventory-only binary](inventory-invocation.json) · [Rebuild script](probe_inventory.py) · [Invocation script](probe_invocation.py)
