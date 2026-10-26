# Recovery handoff with a visible regression

This variant asks whether a Coder–Observer pair can use an executable reproducer
to repair the recovery transition. It uses the same original Git source
`9982a68a6978058c75a9a700e7407df5a49f010c` as the [original task](../README.md),
with two operator-owned `visible_fixtures` added to the prepared baseline:

- `visible-test.rs` becomes `src/tui/agent/self_dogfood_visible_recovery_tests.rs`.
- `visible-registration.rs` appends its test registration to `src/tui/agent.rs`.

Both targets are protected and outside the four editable production/document
paths. No production behavior changes during preparation. The candidate can
read and execute the reproducer before editing:

```sh
cargo test --locked --bin spiral-coder self_dogfood_visible_recovery_tests -- --nocapture
```

The one test covers complete and ranged native source reads, serialized session
restoration, runtime Continue, the edit gate, and the still-pending original
verification command. It must run exactly once and fail on the original source;
the reference repair must pass it. The visible case is derived from the positive
case in the public probe, not an additional independent observation.

The independent `probe.rs` remains byte-for-byte identical to the original
six-case probe (SHA-256
`568e3272447f7b94644b9ecf6fe0b92a9ca4eef13ed7fdba02495885804996e8`).
It still requires five negative controls to pass and the successful handoff case
to fail before repair. `reference/` is unchanged too. Neither is copied into the
live candidate. The old regression command skips only these two named test
modules and still requires at least 864 passing original tests. Final native
verification remains the full `cargo test --locked --bin spiral-coder -q`.

Use either `luna-argon.json` or `argon-large.json` with the normal
[prepare/run/assess workflow](../../../docs/self-dogfood.md). Each pair retains a
32-request shared limit, an initial Coder slice of 8, a 1,200-second wall cap,
90-second provider timeout, Coder output cap of 32,768 and Observer cap of 8,192.
Both roles must make a request; no retry or promotion is automatic.

The `luna-large.json` and `large-luna.json` specs provide a Gemini-free matrix:
Luna 5.6 Coder with Mistral Large 4 Observer, and the reverse pairing. They retain
the exact same task inputs, scope, oracle and limits. The
[failure-routing comparison](../../../docs/evals/2026-10-26-failure-routing/README.md)
uses these pairs with a separate predeclared baseline/candidate comparison;
provider changes are not treated as a runtime improvement over the Gemini runs.

Preparation verifies build, old tests, the visible one-case failure, the
independent six-case failure, and TUI replay on the baseline; every check must
pass on the reference. The declaration freezes both added paths and their bytes.
Candidate changes to the test or registration fail scope even if a command exits
successfully. The independent verifier reconstructs them from the frozen baseline.

This is a changed task presentation and prepared baseline. It is not a
runtime-only comparison with earlier episodes, a hidden benchmark, or evidence
of general self-improvement. Record native completion, source changes, visible
test use, independent qualification, and adoption separately.
