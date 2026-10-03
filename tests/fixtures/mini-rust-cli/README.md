# mini-rust-cli

Small sandbox repository for stressing `spiral-coder` against an unfamiliar Rust codebase.

## Layout

- `src/config.rs`: project-local config path and profile alias resolution
- `src/render.rs`: output shaping and slug rendering
- `src/main.rs`: thin CLI wiring

## Suggested tasks

### 1. Read-only inspection

Locate where project-local profile aliases are loaded for the `greet` command.
Do not edit anything. Final answer should include the main file path.

### 2. Small edit + verify

Fix the slug rendering bug so repeated separators collapse to a single `-`.

Current failing expectation:

- `"Team   Alpha!!"` should become `"team-alpha"`

Verify with:

```bash
cargo test
```

## Notes

- The repo intentionally has one small failing test.
- The fixture is tracked so runtime eval can probe unknown-repo behavior deterministically.
