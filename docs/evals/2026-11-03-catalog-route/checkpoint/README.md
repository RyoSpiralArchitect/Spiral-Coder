# The checkout gained a working packaged catalog

Starting with the R11 native code, the Luna Coder and Observer repaired the
npm launcher, made ordinary replay and catalog listing call the same
selection helper, and added a guide section. Both callers use the existing
selection order, cap and tag behavior. The independent build, visible
routing and native listing, all 11 hidden cases, packaged binary, ordinary
replay and stock suite passed. A separate replay applied the two patches
to the untouched original Git commit. The file inventory matched and all
six original external checks passed. [Independent combined
verification](combined-verification.json) · [Combined patch](combined.patch).

The native session ended blocked in 285 seconds with 24 Coder and 8 Observer
requests. Rustfmt found the module declarations in the wrong order. The pair
fixed their order at step 22, then passed the alias test. That was the only
one of four native checks with fresh proof after the last edit. The first
visible command really exited with a failure, but the retained action text
also lacked the predeclared diagnostic substring for the native reproduction
gate. A later verifier pass cannot supply either missing native receipt.
[First assessed outcome](results.json) · [Exact incremental patch](candidate.patch)
· [Patch replay](plain-replay.json) · [Run and check hashes](receipts.json).

The old hidden documentation check looked for a command and a few keywords.
Its passing result hid a real discrepancy. The published positive example
omitted version and total and named two nonexistent cases. The empty filter
was described as returning an array, but the packaged command actually exits
with status 1, an empty stdout and a diagnostic on stderr. The added heading
also absorbed older completion text. [Measured examples and findings](../guide/c1-audit.json).
These are release blockers for the guide even though the functional command
passed every original external check.

## Run the archived command locally

From a Git checkout containing this experiment and its original base commit,
use Python 3, Rust and Node on a POSIX host. The script creates a temporary
source tree, builds the exact combined patch, packages its binary and runs
both console commands without model credentials.

```sh
python3 -BIS benchmarks/self-dogfood-v2/replay-catalog/guide/replay.py --stage delivery
```

The output includes the real two cases, the empty-selection exit and stderr,
and the four discrepancies in the delivery-stage guide. Pass `--target-dir`
with a dedicated external Cargo cache to reuse dependencies. The script
cleans the root crate before rebuilding in that cache.
