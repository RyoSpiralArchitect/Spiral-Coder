# Which Rust sources did the check include?

Suppose the Coder writes a new module and the automatic test passes. The
module might still be absent from the crate. After a native Cargo check, the
Coder tool message and the Observer's correlated action can include this
`cargo_inputs` record.

```json
{"target":"unit tests src/main.rs","files":{"src/catalog.rs":"not_listed"}}
```

`listed` means the named target's dependency rule contained the file. Cargo
can also list data used by `include_str!` or `include_bytes!`, so a listing
alone cannot establish that a Rust function was compiled or called.
`not_listed` means the tool read a complete rule for that target and the file
was absent. `unknown` means the tool could not establish membership, including
when the check failed. A test harness and a normal binary have distinct labels.
The existing command, exit status, recovery stage, and completion checks retain
their ordinary meaning. This observation does not measure which code a test
executed.

The first implementation observes native and implied writes which successfully
create a regular `.rs` file in the task root. It watches the eight most recently
created paths, up to 240 bytes each. The native automatic test and explicit
Coder shell action inspect the original command and its outputs. No extra Cargo
command runs. The common root-level `cargo test` and `cargo build` forms support
`--locked`, `--offline`, `--frozen`, `--release`, a simple `--target-dir`, and
a single `--bin` selector. For tests, the first Cargo unit-test runner must
identify `src/main.rs` or `src/lib.rs` and its hashed executable. For builds,
one newly written or refreshed binary depfile must identify the product and
its `src/main.rs` entry. The probe follows the current target directory and
profile. Unsupported layouts, ambiguous outputs, quiet tests without a runner
announcement, unrefreshed sidecars and unsupported Cargo options yield
`unknown`. Other commands do not produce this observation.
