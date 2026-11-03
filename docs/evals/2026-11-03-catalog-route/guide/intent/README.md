# The same guide task ran as a repair and qualified

D2 used exactly D1's task, baseline, fixtures, oracles, and Luna pair. The
new executor chose existing-file repair. The Coder ran the guide reproducer
before any change. After an overlength read, it inspected the guide using a
bounded range. Two early edits were refused, then an actual native patch
changed only docs/evaluation.md at step 8.

Steps 9 through 13 passed the executable guide, npm routing, native listing,
rustfmt and locked Rust tests on that final document. The Coder also executed
git diff --name-only. Its report named the affected path and commands, put
both outcome fields on standalone lines, and correctly reported one initial
failure event and 1,030 Rust tests passing with two ignored. The separate
assessment passed all 28 gates, including all seven external checks. The
episode took 140 seconds and used 15 Coder and 4 Observer requests.

[First assessed result](results.json) · [Exact guide patch](candidate.patch) ·
[Patch replay](plain-replay.json) · [Receipts](receipts.json) ·
[Before](lane-before.json) and [after](lane-after.json) native routing controls.

The three pair-authored patches also reapplied to the original catalog
baseline. All six original external checks passed and both examples matched
the locally packaged binary. The additional text-preservation check found
a duplicated completion heading, so the unedited three-patch chain missed
that cumulative review criterion. Its source and verdict are retained.
[Exact model chain](combined.patch) · [Combined verification](combined-verification.json).

The separately [reviewed version](reviewed/README.md) moved the authored
catalog section without changing its contents and restored the original
guide bytes as a prefix. The guide episode qualifies on its own input.
The complete feature was delivered across R11, C1 and D2 and a maintainer
placement correction. R13 and interrupted R14 retain their original verdicts.
