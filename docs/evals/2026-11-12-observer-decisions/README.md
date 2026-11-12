# Observer decision brief pilot

Six Luna Observer calls completed on three frozen S14 transcript prefixes in
56.156 seconds, with no retries or tool execution. Native projection packets
are identical; only the decision-brief prefix differs. The existing native
response schema and validators are unchanged. This is an Observer-only pilot:
no Coder consumed the advice and no candidate was edited or qualified.

| Saved case | Before | After |
| --- | --- | --- |
| Plan admission | Repeats a blocked read without the plan prerequisite | Forwarded reason contains the exact admitted goal and all four acceptance items before the read |
| Missing implementation facts | Requests fixture lines 75–100, missing most of `expected()` | Requests lines 50–87, containing the complete function at 62–78, and explains the next decision |
| Mandatory diagnosis | Correct file, malformed response; no hint forwarded | Correct file with a fuller rationale, malformed response; no hint forwarded |

Four of six replies produce a native soft hint, two in each condition. The
quality observations therefore do not represent a higher acceptance rate.
Both diagnostic responses put prose in the boolean `response_contract.required`
field and fail validation. **This pilot deliberately uses JSON-object transport
for all six calls; production diagnostic mode uses strict JSON schema.** The
two rejections do not demonstrate a production strict-schema regression.

The after plan advice is still imperfect: it calls the issue missing plan
format instead of identifying the changed goal word, and repeats acceptance
text. Neither sampled implementation reply offers a patch, so this pilot does
not demonstrate reduced placeholder patches. The broader diagnostic rationale
never reaches the Coder because its response is rejected.

This is one paired sample per case, with an unblinded descriptive assessment.
Remaining budget is standardized to 12 total / 9 Coder requests, rather than
reconstructed historical allowance; previous-assist outcome is null. Total
provider usage is 57,578 input and 3,611 output tokens (61,189 total). The extra
brief costs input tokens; task success and net request/time savings remain
unmeasured. The prior S14 failure remains unchanged and unqualified.

## Evidence and reproduction

- [Frozen plan](plan.json), pinned before calls at `6acee00`, names source/runtime
  hashes, call order, limits and the manual questions. [Runner](probe.py) refuses
  a started probe; no response was repaired or resampled.
- [Exact inputs](inputs.json.gz), committed at `644e9b6` before calls, are a gzip
  JSON map of filenames to original normalized request/native-output strings.
  Extract these strings into a fresh local directory, preserving bytes, to
  reconstruct the inputs. Do not rerun in the recorded evidence directory.
- [Raw responses](responses.json) preserve the six response-file strings,
  including provider usage. [Results](results.json) distinguish provider receipt
  from native forwarding and include content/file hashes.
- [Native validation](native-validation.json) records parsed envelopes,
  rejection reasons and exactly forwarded hints. These are evaluations of
  recorded replies through `/api/observer_action`, without executing tools.
- [Assessment](assessment.json) answers the frozen quality questions and records
  the fixture hash/ranges, limitations and next probe.
- [Local validation](validation.json): 1,176 Rust main tests pass (two ignored),
  six integration tests, 20 TUI replays and the remaining full-check suites.
  The real native audit suffix first reproduced the plan extraction bug; the
  corrected parser rejects incomplete fields. Final wording-only clarification
  was followed by three focused tests and a successful binary build.

Next, use production response formats in a separately frozen coupled Luna/Luna
run to measure whether Coder follows the plan prerequisite, clears admission
and completes a verified repair under the unchanged gates. This pilot alone
does not establish that improvement.
