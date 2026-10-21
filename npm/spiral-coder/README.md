# Spiral-Coder

A terminal coding agent with coupled **Coder + Observer** contexts.

After the first npm release is published:

```sh
npm install -g spiral-coder
cd your-project
spiral
```

`spiral` starts the TUI in your current directory. Use `spiral -C /path/to/repo`
to choose a different workspace, `spiral --provider gemini` to choose a provider,
and `spiral tui --help` for TUI options. Coder and Observer use the same selected
model unless you explicitly configure an override. Provider credentials follow
the existing runtime's environment/configuration flow.

`spiral serve` opens the local Web UI. `spiral agent --help` lists headless options.
`spiral-coder` preserves the original CLI behavior and argument order. For native
commands, put the subcommand first (for example `spiral agent --provider openai`).

Node.js 20+ is required. Prepared packages contain the native runtimes for macOS
Apple Silicon and Intel, Linux glibc 2.35+ on x64 and arm64, and Windows x64.
Alpine/musl and Windows arm64 are not supported by this initial bundle. No Rust
toolchain, postinstall script, or additional executable download is needed.
The launcher verifies the bundled binary against its packaged SHA-256 manifest,
inherits the working directory/environment/terminal, and forwards process signals
and exit status. The initial package bundles all supported platforms in one
tarball; that makes it larger but also usable with `--ignore-scripts` and offline
after the tarball is obtained.

This source directory is private and intentionally lacks release binaries.
Maintainers assemble the publishable tarball with `scripts/prepare-npm.mjs`; see
[release instructions](https://github.com/RyoSpiralArchitect/Spiral-Coder/blob/main/docs/npm-distribution.md).
Until the first public release, install a prepared CI/local `.tgz` instead of the
registry package. Example: `npm install -g ./spiral-coder-0.2.0.tgz`.

Source: https://github.com/RyoSpiralArchitect/Spiral-Coder — Apache-2.0.
