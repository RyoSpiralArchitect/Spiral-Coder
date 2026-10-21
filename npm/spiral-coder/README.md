# Spiral-Coder

A terminal coding agent with coupled **Coder + Observer** contexts, plus a local
Web UI and a headless CLI.

## Install and start

Requires **Node.js 20+**. The first registry publication is pending; until then,
install the `.tgz` inside a successful CI run's `spiral-coder-npm` artifact:

```sh
npm install -g ./spiral-coder-0.2.0.tgz
cd your-project
spiral
```

After the first registry release, install or update with
`npm install -g spiral-coder@latest`. Both methods support `--ignore-scripts`.
No Rust compiler, postinstall script, or additional executable download is needed.

`spiral` starts the TUI in your current directory. Coder and Observer use the same
selected model unless explicitly overridden. Before making model requests, set
your provider's environment variable: `MISTRAL_API_KEY`, `OPENAI_API_KEY`, or
`GEMINI_API_KEY` (also accepts `GOOGLE_API_KEY`). In the TUI, `/keys` shows setup
help and key status; `/provider` and `/model` change the selection. Ctrl+C exits.

| Command | Use |
| --- | --- |
| `spiral -C /path/to/repo` | Choose a workspace |
| `spiral --provider gemini` | Select a provider |
| `spiral --lang ja` | Select UI language (`en`, `ja`, `fr`) |
| `spiral tui --help` | Show TUI options |
| `spiral serve` | Start the local Web UI |
| `spiral agent --help` | Show headless CLI options |
| `spiral --version` | Show native version |

For explicit native commands, put the subcommand first, for example
`spiral agent --provider openai`. The additional `spiral-coder` command preserves
the original CLI behavior and argument order, including its no-argument Web UI.

## Platforms and troubleshooting

The package contains native runtimes for macOS Apple Silicon and Intel, Linux
glibc 2.35+ on x64 and arm64, and Windows x64. Alpine/musl and Windows arm64 are
not supported by this bundle. All five binaries are included in one tarball.
The launcher checks its selected binary against the bundled SHA-256 manifest,
inherits the working directory/environment/terminal, and forwards process signals
and exit status.

If `spiral` is not found, check `npm prefix -g`: add its `bin/` directory to PATH
on macOS/Linux, or the prefix itself on Windows, then open a new terminal.
If PowerShell blocks the generated script, use `spiral.cmd`. See
[installation troubleshooting](https://github.com/RyoSpiralArchitect/Spiral-Coder/blob/main/docs/npm-distribution.md#installation-troubleshooting)
for permissions and platform issues.

Update a preview with its newer `.tgz`. Remove the commands with
`npm uninstall -g spiral-coder`; project files and Spiral-Coder session data stay
in place.

## Source and releases

The source template is private and intentionally lacks release binaries.
Maintainers assemble and verify the complete tarball before publishing; see
[release instructions](https://github.com/RyoSpiralArchitect/Spiral-Coder/blob/main/docs/npm-distribution.md).
The PR/build-only workflow produces downloadable artifacts; publication requires
an explicit release and npm authentication.

[Source and full documentation](https://github.com/RyoSpiralArchitect/Spiral-Coder)
· Apache-2.0.
