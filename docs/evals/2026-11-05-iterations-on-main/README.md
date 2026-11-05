# Bounded iterations on merged main

Three live `coupled-run` invocations on post-merge main exercise the
`--iterations` chain end to end with `gpt-5.6-luna` in both roles. Each run
used a fresh workspace and external output directory; reports and manifests
are preserved verbatim. These are evidence for the merged chain, not a
promotion gate.

## Conditions

| Run | Task | Budget | Outcome | Stop reason |
| --- | --- | --- | --- | --- |
| [single-iteration](single-iteration/manifest.json) | [task1.txt](task1.txt): one module + one unittest | 3 iterations × 10 requests, 3 observer-reserved | Iteration 1 spent 8 requests, wrote `hello.py` and `test_hello.py`, passed `python3 -m unittest test_hello`, authored an accepted report | `completed` after 1 iteration |
| [no-progress](no-progress/manifest.json) | [task2.txt](task2.txt): two functions + tests | 3 iterations × 4 requests, 1 observer-reserved | Iteration 1 exhausted 4 requests recording no changed file and no newly verified declared command | `no_new_edit_or_verification`; a second budget was correctly refused |
| [two-iteration](two-iteration/manifest.json) | [task3.txt](task3.txt): declared three-artifact package + exact check | 3 iterations × 6 requests, 1 observer-reserved | Iteration 1 spent 6 requests, changed `strutil.py`, `test_strutil.py` and `README.md`, ended `blocked` | Progress unlocked iteration 2, which spent 2 requests, ran `python3 -m unittest test_strutil` as a `newly_verified` declared command and completed |

## Notes

- The two-iteration manifest records the parent `task_id` chain and per-iteration
  ledgers; iteration 2's `newly_verified` contains the declared command only.
- The no-progress condition pins the stop rule: spending the whole allowance
  without an edit or verified check ends the chain instead of buying a second
  budget.
- The completed workspaces were independently re-verified:
  `python3 -S -m unittest test_strutil` exits 0 in the iteration-02 workspace.
- The earlier `gpt-5-mini` probe ended `observer_unavailable` with
  `finish_reason=length` empty Observer envelopes; reasoning-class models can
  exhaust the bounded next-action allowance before emitting content. All
  recorded runs here used `gpt-5.6-luna` for both roles.
- Mistral observer pairing was not exercised: the configured key returned HTTP
  402. Anthropic pairing was attempted but interrupted before evidence.
