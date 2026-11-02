# Repair the native recursive search from inside Spiral-Coder

This is a separate transfer task on merged #56 (`527413cc`). The pair receives
a full original checkout, the visible public-tool reproducer, and permission to
change only `src/file_tools.rs`. The scored executor lives outside the candidate.
The prompt asks for a recorded failing check, then a fix, two final exact checks
and an accurate report. Neither the reference nor hidden test files are added
to the candidate. Run on a Unix host with symlink support.

Preparation starts the native server with no credentials and searches only
synthetic temporary directories. The visible check follows a discovered file
link on stock. Two independent checks cover directory aliases and nested file
links; a neighboring control checks ordinary nested and explicit regular-file
search, line numbers, case handling and hidden entries. The original full local
suite must retain the frozen Rust test identities and statuses. All probes use
the real search endpoint. This is about entries found by the walk, not a claim
about general path confinement or concurrent filesystem changes.

Materialize inputs with `python3 -BIS benchmarks/self-dogfood-v2/search-walk-links/materialize.py /absolute/new-inputs`.
Use the [frozen prepare/run/assess procedure](../../../docs/self-dogfood.md#prepare-run-assess)
with the emitted spec. H1 uses the same Luna pair and budgets as R11. Keep its
first run even if the task does not qualify. Review any passing patch before
integrating it separately; a transfer pass would not finish the catalog task.

[H1 results](../../../docs/evals/2026-11-03-search-walk/README.md) separate
independent behavioral success from the failed stock formatting and report
requirements. To materialize the H2 task with an explicit formatting check,
add `--explicit-format` to the materialization command above.
