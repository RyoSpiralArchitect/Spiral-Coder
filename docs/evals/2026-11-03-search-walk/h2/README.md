# When selecting the right command still does not execute it

The same Luna pair reproduced the visible failure, independently authored
a three-line guard in the real recursive search, and passed the visible
reproducer again. The independent build, both hidden link probes and normal
search control passed. The original source remained otherwise intact, and
the sealed patch [reapplied exactly](plain-replay.json). The guard allowed a
file-type lookup error to fall through; the behavioral probes do not cover
an error or a filesystem race.

H2 spent all 32 task requests (24 Coder, 8 Observer) in 225 s. The Coder
selected the declared rustfmt command eleven times after the visible check
passed, but none ran. Nine attempts were refused for plan drift, two for recovery diagnosis. The fresh native checks ended at 1/3, the original stock check
failed at formatting, and the pair truthfully ended blocked. Making the
formatting and report requirements explicit did not qualify this repair.
Compare [the first run](../README.md), [the complete assessed outcome](results.json),
[the exact candidate](candidate.patch), [the native handoffs](trace-handoffs.json)
and [the raw hashes](receipts.json).
