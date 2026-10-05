# TUI interaction contract

The TUI draws and hit-tests through `src/tui/layout.rs`. `ScreenLayout` owns the
focused pane split, tab row, composer, and footer rectangles. Tab labels and their
pointer targets are built together, including review counts. A click in the
full-width composer keeps the active recipient; it does not select a pane based
on which half of the composer was clicked.

Message scrolling counts Ratatui's rendered, wrapped rows at the actual pane
width. This requires `unstable-rendered-line-info` in the already locked Ratatui
0.29 dependency; it does not upgrade the dependency. Keep this count aligned with
the `Paragraph` wrapping options when changing message rendering.

Provider/model pickers expand within the terminal height and keep the selected
item in the visible window. Compact tab labels keep all five destinations visible
at normal 80-column sizes. Very small terminals remain renderable, but do not
provide the same amount of readable content as an 80×24 terminal.

## Input and provider readiness

- `Ctrl+O` asks Observer to review the latest completed Coder response. It opens
  the Observer pane and keeps any unsent Observer draft. If there is no completed
  Coder response, the pane explains that no output is available yet.
- Enter while a pane is streaming leaves its next draft intact. `Ctrl+K` cancels
  the focused stream; entering a new draft does not cancel or replace it.
- Home moves message history to its actual first rendered row. PageDown can then
  move toward the latest output. PageUp and wheel movement stay within the same
  rendered bounds, including after a resize or history filter change.
- Bracketed paste inserts one text payload into the focused editable composer.
  Japanese characters and line breaks are retained; CRLF is normalized to LF.
  Newlines and slash commands inside a paste never dispatch messages or commands.
  Read-only Tasks/Review/Merge panes ignore pasted text. The terminal's bracketed
  paste mode is disabled again on exit along with mouse capture and raw mode.
- TUI setup may open before a Mistral or Anthropic key is configured. `/keys`
  remains available to show the required environment variable. Normal sends,
  manual reviews, and Observer diagnostics validate their target pane before
  starting a provider request. Missing credentials leave the draft intact.
  Shared noninteractive config resolution retains its existing authentication
  requirement; setup still validates URLs, models, and numeric options.
- Restoring a different provider or endpoint from preferences, or changing
  `/base_url`, resolves credentials for the new target from its environment.
  A launch-time explicit key is retained only when its provider and normalized
  endpoint stay the same. Preferences still contain no credentials. An invalid
  restored target leaves the original runtime configuration intact.

## Regression contract

`src/tui/ui_tests.rs` renders the actual widgets through Ratatui `TestBackend`.
`src/tui/interaction_tests.rs` drives the production pointer handler, terminal
input mapping, and send validation, using temporary preferences roots.

The deterministic checks must preserve these outcomes:

| Interaction | Required behavior |
| --- | --- |
| Long Japanese/English message at 80×24 | Latest wrapped content is visible at the bottom |
| Later provider/model options in a short terminal | Selected option remains visible |
| Click or scroll inside the focused right pane | Event reaches the visible pane |
| Click every displayed right tab at 80, 120, and 200 columns in both focus modes | Label selects that tab, including count badges |
| Click either half of the full-width composer | Active recipient stays the same |
| Submit with missing target credentials | Warning appears, no provider task starts, draft remains |
| Ctrl+O with an empty Observer draft | Latest completed Coder output becomes a review request |
| Enter while Observer is streaming | Next draft remains intact |
| Home followed by PageDown | Rendered message history moves down from the first row |
| Bracketed Japanese/multiline paste containing a slash command | Text is inserted without dispatching anything |
| Restore another provider/endpoint or change `/base_url` | Old explicit credential is not inherited by the new target |
| Render at 1×1, 20×8, 80×24, and 120×40, with and without a picker | No rendering panic |

Run the focused suites and existing orchestration replay:

```bash
cargo test --locked --bin spiral-coder tui::events::tests::
cargo test --locked --bin spiral-coder tui::ui::tests::
cargo test --locked --bin spiral-coder interactive_setup_defers_only_missing_credentials
cargo run --locked -- tui-replay --spec .spiral-coder/tui_replay.json
```

The existing `tui-replay` cases cover Observer suggestion and handoff plumbing.
They do not exercise terminal pixels, pointer coordinates, or native terminal
keyboard encodings. The `TestBackend` and production input-handler checks cover
rendering and routing contracts without asserting desktop computer-use coverage.
A PTY smoke can additionally verify process startup, key input, terminal resizing,
and terminal-mode restoration on exit.
