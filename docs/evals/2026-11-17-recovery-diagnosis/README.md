# S26: an unlocalized failure keeps its diagnosis across unrelated passes

S25 had 36 governor blocks. 27 of them came from the recovery gate. The sealed
trace shows the cycle behind most of them. The visible check failed, and its
recovery focus was unlocalized: only a last confirmed path was recorded. A
diagnostic read then moved recovery to `fix`. Next, a different exact check
passed. Because an unlocalized focus never becomes `observed`, `pending_stage()`
returned recovery to `diagnose`, and every further rerun or check was blocked
until another read.

#97 keeps a `fix` stage when an unrelated check passes while an `unlocalized`
or `unavailable` focus is pending. A failure still re-opens diagnosis, and an
`awaiting_read` focus still needs its exact read.

S26 is a fresh, unseeded Luna/Luna run with the S20–S25 inputs unchanged byte
for byte. The executor was #97's head `f82f62d`. The [plan](plan.json) was
pushed at `1e60012` before any provider call. Preflight passed, and both
controls behaved as before. [Controls](sensitivity-controls.json)

## First terminal

**Unqualified, not adopted. #82 remains open, but the candidate itself is
complete.**

- **Independent groups:** all seven pass, including held-out 24/24, the
  26-subcase path matrix, visible, guide, npm, build and stock checks.
- **Exact checks:** in round 6, all six passed after the last edit (steps
  133–138).
- **Missing pieces:** only two remain.
  - **No authored report.** The chain stopped `observer_unavailable` before any
    `done`.
  - **Weak tests.** The two authored tests detect one of the four faults
    (missing pending data).

| Outcome | S22 | S25 | S26 |
| --- | ---: | ---: | ---: |
| Stop | completed | iteration_limit | observer_unavailable |
| Task / wire requests | 166 / 182 | 192 / 213 | 168 / 183 |
| Gates passed | 25 / 26 | 18 / 26 | 20 / 26 |
| Independent groups | 7 / 7 | 5 / 7 | **7 / 7** |
| Held-out cases | 24 / 24 | 22 / 24 | **24 / 24** |
| Path subcases | 26 / 26 | 26 / 26 | **26 / 26** |
| Recovery-gate blocks | 8 | 27 | **5** |
| Authored-test sensitivity | 0 / 4 | 0 / 4 | 1 / 4 |

Recovery-gate blocks fell from 27 to 5.

[Assessment and evidence](results.json) · [Candidate patch](candidate.patch)

## Why it stopped one step from completion

1. After the six passing checks, the Coder yielded to the Observer.
2. The Observer's first answer had a duplicate `summary` key. It was a
   malformed envelope, so it was charged as transport, and one repair was
   requested.
3. The repair was a valid envelope proposing `done`. Its report lacked the
   standalone `Before: FAILED` line, so the native gate rejected it, with no
   further repair.
4. Two recovery paths did not apply:
   - The work return needs outstanding native work, and there was none.
   - The verified report-only fallback covered only *structurally* rejected or
     truncated repairs. Its documented rule said "a normal semantic rejection
     … ends this path".

With every native obligation current, only the authored report was left, and
nothing offered the Coder a chance to write it.

## Next condition

- **Semantic rejections reach the report-only attempt.** A completed repair
  rejected semantically now admits the same verified report-only attempt as a
  malformed one. Admission still requires current native proof, no approval
  barrier and remaining Coder allowance, and it is granted once.
- **The closeout review skips report-only turns.** The authored-test review
  (#94) no longer fires inside a report-only closeout. That turn offers only
  `done` within one wire attempt, so a review there would trade the report for
  a request the Coder cannot act on.
