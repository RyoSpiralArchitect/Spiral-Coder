# Reviewed catalog feature

The model-authored guide inserted the catalog block in the middle of the
existing completion section and then repeated its heading. The maintainer
moved the complete catalog block to the end. The original guide is now an
exact byte prefix. The console commands, example outputs, explanatory text,
Rust and npm implementation are identical to the model-authored version.

The combined verifier first reapplied all three original model patches and
matched their sealed source inventories. It then applied this separately
identified document placement change. The reviewed patch reapplied from
the original Git tree, passed all six original external checks, matched the
two packaged CLI examples and preserved the original paragraphs. The
published proposal also has separate permanent regressions for the guide
examples and npm argument forwarding.

[Reviewed patch on the original tree](combined.patch) ·
[Maintainer placement change alone](review-layout.patch) ·
[Independent original-tree checks and actual CLI outputs](combined-verification.json) ·
[Source comparison on the adoption base](current-main-rebase.json).

To exercise the public archive from a checkout of the experiment without model
credentials, run the following with Python 3, Rust and Node on a POSIX host.

```sh
python3 -BIS benchmarks/self-dogfood-v2/replay-catalog/guide/replay.py --stage reviewed
```
