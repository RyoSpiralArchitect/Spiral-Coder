# Migrating to Spiral-Coder

Spiral-Coder is the new product name. Mistral remains a supported provider;
provider identifiers and vendor credentials such as `MISTRAL_API_KEY` are unchanged.

| Surface | Previous name | Current name |
|---|---|---|
| Rust package / CLI | `obstral` | `spiral-coder` |
| Python package / CLI | `obstral-lite` | `spiral-coder-lite` |
| Python entry module | `scripts.obstral_lite_cli` | `scripts.spiral_coder_lite_cli` |
| Project configuration / state | `.obstral/` | `.spiral-coder/` |
| Project instructions | `.obstral.md` | `.spiral-coder.md` |
| Repo-map ignore file | `.obstralignore` | `.spiral-coderignore` |
| REPL history | `.obstral_history` | `.spiral_coder_history` |
| Generic configuration environment | `OBS_*` | `SPIRAL_CODER_*` |
| Web asset override | `OBSTRAL_ASSETS_DIR` | `SPIRAL_CODER_ASSETS_DIR` |
| Default session | `.tmp/obstral_session.json` | `.tmp/spiral_coder_session.json` |

Install the renamed commands from the checkout:

```sh
cargo install --locked --path .
python3 -m pip install .
spiral-coder --version
spiral-coder-lite --help
```

Update shell exports, launchers, automation commands, and custom session paths to
the new names. No API key needs to be regenerated. Vendor variables such as
`OPENAI_API_KEY`, `GEMINI_API_KEY`, and `MISTRAL_API_KEY` keep their existing names.
Installing the new executable does not uninstall an older executable.
The Lite Python runtime also accepts old `OBS_*` variables during migration;
a present `SPIRAL_CODER_*` variable takes precedence, even when empty.

## Copy existing project data

Run the helper from this checkout, pointing it at each project with old state:

```sh
python3 -S scripts/migrate_legacy_state.py --root /path/to/project
python3 -S scripts/migrate_legacy_state.py --root /path/to/project --apply
```

The default is a dry run. `--apply` copies the old state directory, project
instructions, ignore file, REPL history, and default session/trace/export files.
It preserves originals and never replaces an existing destination. Symlinks are
skipped. Inspect `keep-existing` and `skip-*` entries before removing any old files.

The helper preserves file contents byte for byte, including historical transcripts
and benchmark receipts. It does not rewrite old paths in recorded evidence.
Custom session files can be opened with `spiral-coder agent --session /path/to/file`.
Old paths in active project instructions or custom eval specs should be updated
manually after reviewing the copied files.

The Web UI copies legacy browser storage into the new namespace on first load
when no new value exists. This requires the same browser origin (scheme, host,
and port). Existing new values win; old entries are preserved.

Historical benchmark documents describe earlier runs, not a fresh measurement
of the renamed runtime. Rerun the current replay/eval commands for current evidence.
