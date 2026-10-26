# Cross-family self-dogfood

`scripts/run_self_dogfood.py` runs one real repository repair with a Coder and an
Observer from different provider families. The execution unit is the pair. A
Coder-only success cannot qualify this lane. The existing same-model
`run_coupled_bench.py` lane and its fixtures remain separate.

The first task catalog is [self-dogfood-v1](../benchmarks/self-dogfood-v1/README.md).
It targets a real recovery-handoff defect from a pinned baseline. New task contracts
must be authored and reviewed before spending requests. Model-generated checks,
acceptance changes, automatic retries and automatic promotion are not supported.
The first live runs also exposed a runtime coercion conflict; the
[follow-up edit safety contract](followup-edit-safety.md) preserves diagnostic
reads and authored local edits while retaining completion and verification gates.

## Prepare, run, assess

Prepare an execution binary from a known source commit first. The executor's
commit can differ from the defective candidate baseline: the executor needs the
mixed-role harness, while the candidate starts from the same original defect in
every comparison. `--runtime-commit` records the operator's build provenance;
the independent binary hash binds the bytes actually executed. Neither alone
proves compiler reproducibility. The running executor never reloads candidate
source.

```sh
python3 -S scripts/run_self_dogfood.py prepare \
  --repo "$PWD" \
  --baseline-ref BASELINE_COMMIT \
  --runtime-commit EXECUTOR_SOURCE_COMMIT \
  --binary /absolute/executor-target/debug/spiral-coder \
  --spec benchmarks/self-dogfood-v1/recovery-handoff/luna-argon.json \
  --target-dir /absolute/self-dogfood-validation-cache \
  --out /absolute/new-episode
```

Preparation makes no provider calls. It copies every regular tracked file from
the exact Git tree, independent of dirty/untracked source files and archive
export attributes. Symlinks, submodules and oversized snapshots are rejected.
The candidate receives a full copy with its own local Git history. Original
standalone tests and explicitly protected files cannot be editable.

A task may also declare `visible_fixtures`: operator-owned test and registration
overlays applied before the prepared baseline is inventoried and copied into the
candidate. Their targets are protected automatically. `baseline_commit` still
identifies the original Git source; `baseline_inventory` binds the assembled
baseline, and `visible_fixture_paths` records which paths received overlays.
The frozen spec/input inventory binds their source bytes and application modes.
This is a separately labeled task variant, not an unchanged-baseline comparison.

The external episode directory contains the frozen binary, driver, task contract,
prompt, probes, reference repair and baseline. Preparation runs the declared
checks on disposable baseline and reference copies. The original defect must
fail for the specified reason; the independently supplied reference repair must
pass every check. A missing assertion, timeout, infrastructure failure or wrong
test count cannot substitute for the expected baseline failure.

Save the emitted `declaration_sha256` independently of the episode tree. Run the
**frozen driver** with that digest and only the required provider credentials
already exported in the calling environment:

```sh
python3 -S /absolute/new-episode/driver/run_self_dogfood.py run \
  --out /absolute/new-episode \
  --declaration-sha256 PREPARE_DIGEST

python3 -S /absolute/new-episode/driver/run_self_dogfood.py assess \
  --out /absolute/new-episode \
  --declaration-sha256 PREPARE_DIGEST \
  --run-sha256 RUN_RECEIPT_DIGEST
```

`run` emits the `run_receipt_sha256` used by the second command. Preserve that
value independently too. The run receipt binds the whole candidate inventory,
native report/ledger/trace, prompt, patch and process record. Modified evidence
cannot reuse its prior pin. Declaration/input/binary/driver integrity is checked
before and after execution; saving a rewritten adjacent checksum cannot replace
the operator's pinned digest.

Each output directory permits one run and one assessment attempt. A timeout or
interruption is retained, never retried in place. For an interrupted episode
without a finished run receipt, omit `--run-sha256` to inspect the durable request
ledger; this can report spent and in-flight requests but **cannot qualify** the
candidate. A malformed or truncated native report falls back to a valid durable ledger for
request accounting. Parse/type errors are retained in the assessment and prevent
qualification; missing or unusable ledger counts remain unknown rather than
becoming zero. A partial report does not consume the assessment attempt without
producing an error receipt.
Start any later attempt in a fresh output directory with a new preparation.

## Frozen task contract

Version 1 JSON contains these fields:

| Field | Meaning |
| --- | --- |
| `id`, `prompt` | Stable task ID and relative prompt file beside the spec. |
| `editable` | Exact production/document paths the candidate may change; up to 16. |
| `required_changed_paths` | Optional unique subset of `editable` whose files must actually differ from baseline, such as required behavior documentation. |
| `protected` | Additional original paths that must exist and remain unchanged. Standalone test paths are also protected automatically. |
| `visible_fixtures` | Optional 1–8 `{source, target, mode}` overlays applied to the baseline before candidate creation. Every target is protected, unique, and disjoint from editable, probe and reference paths. |
| `native_checks` | Exact shell command strings that must succeed in native execution after the last mutation. |
| `roles` | `coder` and `observer`, each with exact `provider`, `model`, optional standard `key_env`. Providers must differ. |
| `max_requests` | Shared durable cap, including retries and both roles; 6–64. |
| `coder_chunk_requests` | Initial Coder wire slice, including compatibility retries; 4 through `max_requests - 2`. It reserves capacity but does not guarantee Observer participation. |
| `wall_seconds` | Live process-tree wall cap, at most 3,600 seconds. |
| `evaluation_wall_seconds` | Combined check cap for each baseline, reference or candidate assessment, at most 3,600 seconds. |
| `timeout_seconds`, `max_tokens`, `temperature` | Coder request configuration. Observer next-action configuration retains the timeout and uses temperature 0.2. |
| `observer_max_tokens` | Optional independent Observer output-token budget, integer 1–65,536. If omitted, the default remains `min(max_tokens, 1200)`. |
| `checks` | Ordered independent argv checks; build, regression and baseline-failing improvement checks are required, at most eight. |
| `probes`, `reference` | Operator-owned `{source, target, mode}` overlays; `mode` is `append` or `replace`. Reference targets must be editable paths. |

Overlay source paths are relative to the spec's directory. The runner copies the
exact bytes and adds no separators, so appended Rust probes should start with a
newline. Probes are applied only to disposable verifier copies. Reference edits
are applied only to the reference verifier, never to the live candidate.

Visible fixtures are different: the candidate can read and run them. Their test
registration must be protected too. Every verifier uses the same prepared
baseline, so a candidate cannot weaken a visible test in the independent copy.
Changing either the test or its registration also fails candidate scope and
protected-file checks. Visible checks should declare exact test counts and an
expected failure identity; an empty test selection must not count as a pass.
Keep independent controls in `probes` even when a visible reproducer is supplied.

A check has `id`, `kind` (`build`, `regression`, `improvement`), `baseline`
(`pass`, `fail`), `argv`, and `timeout_seconds`. A failing baseline also requires
`baseline_failure_contains`, a nonempty list of nonblank diagnostic strings.
Each whole string must occur verbatim in the failing log; a bare string is
rejected rather than interpreted as individual characters. The same type
validation applies when this optional field is supplied on a passing check.
Optional `expected_test_count` requires a completed Rust test summary with exactly that
many passed tests and zero failed/ignored tests for a pass. The baseline failure
must have exactly one failed test and the remaining tests passed, with none
ignored; `expected_baseline_failures` can explicitly freeze a different count.
The startup banner `running N tests` does not count as completion.
`minimum_passed_tests` requires a completed successful summary with at least that
many passed and zero failed tests; it permits existing ignored tests. This guards
against silently disconnecting a preserved test module.

Checks run as explicit argument arrays, with no evaluator shell. Allowed
executables are `cargo`, `python3`, `node`, and
`{target_dir}/debug/spiral-coder`. A `build` check must invoke `cargo build`.
Supported placeholders are `{workspace}`, `{target_dir}`, `{receipts}` and
`{runtime}`. Placeholders are substitutions inside argv entries, not shell
variables. Literal braces must be doubled. Candidate runtime tests must follow a
successful build and execute `{target_dir}/debug/spiral-coder`; `{runtime}` is the
frozen executor and must not be substituted for candidate behavior. Candidate
binary hashes are retained. Put replay output under `{receipts}`, which is
outside the verifier workspace, to preserve repository scope.

## Assessment and evidence

Assessment assembles a fresh verifier from the frozen baseline plus **only**
allowed candidate artifacts, then overlays the frozen probes. Candidate edits to
an evaluator, manifest, original standalone test or other forbidden path cannot
change what the independent checks execute. Such edits also fail strict scope,
even when the independent checks pass. Every `required_changed_paths` entry must
actually change; native completion cannot waive a required documentation update.

The assessment preserves separate gates for native completion and accepted
`done`, no remaining acceptance or pending checks, actual edits and successful
native edit receipts, exact fresh native commands, full candidate path scope,
original protected files, independent build/regression/improvement checks, and
bounded process/request execution. Both roles must have actual wire reservations
in the durable ledger. The report must match the declared provider/model,
endpoint origin, transport and effective role configuration, and agree with the
ledger's task, workspace, limit and request counts. Role configuration receipts
are not captures of every transformed HTTP field: provider adapters can further
normalize or omit sampling parameters. An explicit `observer_max_tokens` is
passed as `coupled-run --observer-max-tokens` for both the initial assist and
repair; it may exceed the Coder token budget. The exact-role gate checks the
explicit limit, or the original default when absent. These are configured output
limits, not measured token use or a guarantee that a provider supplies that many
tokens; provider-specific transformations and reported usage remain separate.

The raw manifest hashes frozen authority, native receipts, candidate artifacts,
check outputs and the assessment. All candidate files, including ignored and
untracked files, participate in scope inventory; only `.git` internals are
excluded. Patches compare against the frozen candidate base commit, so a
candidate commit cannot hide its changes. Operational build caches and temporary
verifier copies are explicitly excluded from receipt hashing.

A qualified candidate is evidence for this one frozen task, pair and budget. It
is not proof of general model capability, a causal model comparison or approval
to adopt the patch. `promotion_authorized` stays false. Review the patch and
receipts before creating a normal follow-up implementation or promotion PR.

## Resource and trust boundary

The runner uses POSIX process groups, a wall deadline, an 8 MiB cap per process
log, and an exclusive lock for the external Cargo cache. It kills the owned
process group on timeout or exit, including descendants left by a shell. Native
wire reservations remain in the external durable ledger if a request is killed.
Cargo build artifacts never belong in the candidate: every execution uses the
external `CARGO_TARGET_DIR`, `CARGO_INCREMENTAL=0`, and disabled dev/test debug
info. Separate episodes can reuse one validation cache sequentially.

Execution receives a small environment allowlist and only the two declared
provider credential variables: `OPENAI_API_KEY`, `GEMINI_API_KEY`,
`MISTRAL_API_KEY` or `ANTHROPIC_API_KEY` as applicable. No key value enters CLI
arguments or the frozen contract. Leading/trailing whitespace is stripped once;
blank keys are rejected before execution, and identical normalized bytes are used
for provider environment values and redaction. Console secrets are redacted and any detected
credential output prevents qualification. Checks receive no provider keys. The
runtime also removes provider credentials from tool subprocess environments.

These are provenance and evaluation boundaries, **not an OS security sandbox**.
The candidate's shell runs as the same operating-system user. It must not read
external probes, reference repairs, other checkouts or credentials, and must not
change external evidence. The reference and probes are held out by the task
contract and are never supplied in the prompt; same-user filesystem access does
not make them inaccessible or confidential. Pinned hashes detect relevant
artifact changes, not arbitrary reads or every hostile side effect. Inspect
native tool traces for contamination and apply an actual sandbox before claiming
adversarial isolation. Existing inline tests inside editable production files
are not independently preserved by the standalone-file rule; frozen probes,
regression counts and review remain necessary.

## Offline validation

```sh
python3 -S -m unittest discover -s tests -p test_self_dogfood.py -v
```

The tests use tiny temporary Git repositories and synthetic native receipts.
They exercise orchestration, provenance, qualification failure modes and bounded
subprocess handling without building Rust or spending provider requests.
