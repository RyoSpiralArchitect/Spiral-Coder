# Fixed expectations for a first interruption

The [previous observation run](../2026-11-18-observed-fixture-input/README.md) derived the expected pending directory from the number of finalized rows in the observed manifest. A candidate added a finalized row and made the comparison accept the stale pointer and ledger as matches while the real test still failed.

The protected check now compares against a fixed first-interruption case. Zero finalized rows require `iteration-01`, valid JSON at that directory's ledger path, an empty request list, and zero pending and aggregate task and wire totals. Generated and validator-input snapshots still record the actual values. Each probe starts with a clean test-package build in the forwarding profile. This prevents a temporary checkout from using test code previously compiled for another case. [Protected observation change](fixed-observation.patch)

## Model-free calibration

Each replay ran the original named integration test through the same unchanged candidate validator.

| Input to the test | Exit | Fixed-field differences | Diagnostic |
| --- | ---: | ---: | --- |
| Original fixture | 101 | 7 | Invalid pending directory |
| Earlier control terminal patch | 101 | 6 | Invalid pending directory |
| Earlier observed-input terminal patch | 101 | 7 | Invalid finalized iteration order or directory |
| Coherent test-only control | 0 | — | Passed |

Both earlier patches still show `iteration-02` where `iteration-01` is expected. The second patch also shows one finalized row where zero is expected. The coherent control exits successfully and emits no failure sample. All five exact checks pass in that separate control. [Original sample](replays/baseline.json) · [Earlier control](replays/former-control.json) · [Earlier observed input](replays/former-observed.json) · [Coherent control checks](coherent-checks.json)

An initial coherent calibration reused a stale forwarding-profile test executable. That attempt was invalidated before any provider requests; the table records a fresh compilation for each input after the cache fix.

## Luna pair

The Coder and Observer used the same `gpt-5.6-luna` profile and runtime binary as the previous observation run. The task text, starting Rust files, public checks, and overall caps remained fixed. The starting inventory matches all 1,673 entries from the prior run after the observation patch. The first terminal and the later provider recovery have separate receipts. [Original plan](plan.json) · [Recovery plan](continuation-plan.json)

The first corrective Observer advice still asked Coder to write the empty ledger under `iteration-02`. The retained failure sample expected `iteration-01`. The durable assist receipt identifies the accepted suggestion and the exact call that made the first change. The first patch edited only the test and used `iteration-02`. Later edits corrected the manifest pointer and its pending counts before a Coder response timed out in transit. The first terminal spent 17 task requests and 20 total requests. [First terminal evidence](first-terminal.json) · [Patch at the timeout](first-terminal.patch)

The native CLI reused the saved conversation and files in a new request ledger with only the remaining allowance. The Coder finished the test-only repair and reformatted it. The validator and protected checks kept their starting bytes. The combined run spent 32 task requests and 35 total requests. After the final formatting change, it recorded three current passes. The full Rust integration suite and the public skipped-directory check were still due. The runtime accepted a blocked report. [Recovery evidence](recovered-terminal.json)

The recovered patch replayed to the exact terminal file inventory. All five checks passed on that separate copy with zero model calls, including the seven Rust integration tests and skipped-directory control. The live completion gate remains unmet. [Recovered patch](recovered-terminal.patch) · [Independent exits](recovered-terminal.json)

## Reproduction

Use the [previous starting tree and setup](../2026-11-18-observed-fixture-input/README.md#reproduce-the-boundary-check) and apply [this observation patch](fixed-observation.patch). Start a disposable copy for each candidate. Apply the [earlier control patch](../2026-11-18-observed-fixture-input/control-terminal.patch), the [earlier observed-input patch](../2026-11-18-observed-fixture-input/observed-terminal.patch), or the [coherent control patch](../2026-11-18-observed-fixture-input/coherent-control.patch) to that copy. Reset the Spiral-Coder package in the separate Cargo target and run the original five checks listed in [the plan](plan.json).
