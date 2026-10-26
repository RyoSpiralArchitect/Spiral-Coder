# Using Spiral-Coder

[Documentation](README.md) · [Getting started](getting-started.md)

## Terminal workflow

`spiral` starts the TUI in the current directory. Use `spiral -C /path/to/project`
to choose another project. `/root /path/to/project` changes the tool root for
subsequent sends. Start with a provider configured in your shell; `/keys` shows
which environment variables the active panes expect.

Coder reads, edits and runs checks. Observer reviews recorded results and can
propose a follow-up. Chat is for discussion and does not execute tools.
`Ctrl+R` cycles the right-hand tabs; `/tab observer` and `/tab chat` select a tab.

Use `/provider` and `/model` to configure the active pane. Most pane preferences
are stored per project in `.spiral-coder/tui_prefs.json`, without API keys.
Coder and Observer use the same selected model unless separately configured.
See [provider/model compatibility](model-catalog.md) before mixing transports.

Useful controls:

| Control | Effect |
| --- | --- |
| `Ctrl+O` | Review the latest completed Coder response with Observer |
| `/autofix` | Toggle Observer suggestion forwarding |
| `Ctrl+K` | Cancel the focused stream |
| `/find text` | Filter messages in the current pane |
| `/lang ja` / `/lang en` / `/lang fr` | Change interface and prompt language |
| `/help` | Show the full command list |

The [TUI interaction reference](tui-interactions.md) covers input, scrolling,
provider readiness and behavior while a response is streaming.

## Headless coding and resume

The npm launcher forwards explicit subcommands to the native CLI. Put the
subcommand first. This example uses the Gemini key from your shell:

```sh
spiral agent --provider gemini -C . --session .tmp/spiral-session.json \
  "Fix the failing test and report the files changed and checks run."

# Later: omit the prompt to continue the saved session.
spiral agent --provider gemini -C . --session .tmp/spiral-session.json
```

Relative session paths resolve under the selected tool root. `--new-session`
starts fresh. The CLI prompts before commands and file edits by default;
`--yes` skips those prompts when you choose automatic execution. The TUI uses
automatic tool execution, while the Web UI exposes pending approvals in Settings.

`--autofix 1` adds an Observer diff review and a Coder follow-up after the agent
run. Run `spiral agent --help` for iteration limits, session and trace options.
Session files and traces contain project content; choose their storage location
accordingly.

The installed `spiral-coder` command also remains available. Unlike bare
`spiral`, its no-argument behavior starts the Web UI.

## Project configuration

Create a project configuration template with:

```sh
spiral init -C .
```

Review `.spiral-coder.md` and set the project's actual test command and relevant
instructions. Spiral-Coder also detects manifest files, repository layout and
Git state for its initial context. `@file` references give it a specific starting
point; ask for the smallest relevant change and an explicit check.

File tools validate paths against `tool_root`. Shell commands still execute on
your machine; the tool root is not an operating-system sandbox.

## Local Web UI

`spiral serve` serves the interface at `http://127.0.0.1:18080/` by default.
Set `toolRoot`, provider, model and key in Settings before starting work. The
Coder, Observer and Chat panels keep execution, review and discussion visible
together. The server binds to loopback by default.

## Build and contribute

The npm package is the published release. For development changes that have not
been released yet, build the repository with Rust:

```sh
git clone https://github.com/RyoSpiralArchitect/Spiral-Coder.git
cd Spiral-Coder
cargo install --locked --path .
spiral-coder tui --provider gemini -C /path/to/your-project
```

A source install provides `spiral-coder`; it does not replace npm's bundled
`spiral` runtime. To run the current checkout directly, use
`cargo run --locked -- tui --provider gemini -C /path/to/your-project`.

Run the model-free suite before contributing:

```sh
bash scripts/check-local.sh
```

It covers Rust tests, TUI replay, Python helpers, Web/npm tests, repository-map
checks and the end-to-end smoke. Live evaluations are separate and require
explicit provider configuration. Read [AGENTS.md](../AGENTS.md), then use the
[R&D hub](research/README.md) for architecture, state ownership and experiment
contracts.
