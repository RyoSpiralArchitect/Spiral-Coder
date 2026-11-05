# A qualified native search repair, independently replayed

The same Luna model in separate Coder and Observer contexts repaired recursive
search in Spiral-Coder's full source tree. It made two native edits to one
file: at step 5 it skipped discovered symlinks; at step 10 it fixed the
formatting that rustfmt had actually rejected. The sole schema-guided Observer
read was validated, accepted and executed with matching arguments. The run
took 204 seconds and 19 task requests (15 Coder, 4 Observer, zero transport).

After the last edit the visible public-API test, rustfmt and the exact locked
Rust test all passed in the episode (steps 11, 13 and 14). The author reported
1 visible test failing before the repair and passing afterward, and 1,066
Rust unit tests passing with 2 ignored. These numbers match the actual
commands. Both outcome fields passed the original bounded assertion parser.
One presentation detail remains: the Before field follows the preceding
sentence on its line, despite the prompt requesting separate lines for both.
The [authored report](results.json) is preserved as written.

The independent evaluator passed **28/28 gates**: the separately built candidate
passed the visible file-link probe, two hidden probes, neighboring ordinary
search and the frozen stock suite. No protected or out-of-scope file changed;
the [exact patch](candidate.patch) applied to the untouched original source
and reproduced the candidate [byte for byte](plain-replay.json). The stock
suite verifies the frozen Rust test identities, including integration tests.
The executor never reloaded the candidate. Qualification permits separate
human review; it does not authorize automatic adoption.

For comparison, the same retained H2 rejected plan was sent to both executors
over loopback. Neither executed the original proposal. A second reply used the
same command and pending steps but copied the previously admitted goal and
ordered criteria. The [old executor](control-before.json) manufactured a
diagnosis and refused the check again. The [new executor](control-after.json)
executed rustfmt, which correctly failed and printed the real diff in the H2
source. Neither run modified that workspace. The [fixture](replay_revision.py)
scripts these two replies; it is not evidence that a model independently
authored them.

H3 uses the known H2 search task with a new executor, following the unseen H1
and unsuccessful H2. It is one qualified repair, not a success rate, proof
of a unique cause or a pass for the additive catalog CLI. The candidate's
file-type query falls through on an OS error; the probes cover stable symlinks
and ordinary files, not failure injection, concurrent filesystem changes or a
general sandbox boundary.

[Predeclaration](plan.json) · [First independent outcome](results.json) ·
[Native handoff](trace-handoffs.json) · [Receipt hashes](receipts.json).
