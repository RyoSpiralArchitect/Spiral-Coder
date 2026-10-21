# npm distribution and the `spiral` terminal command

The npm package is `spiral-coder`; its short command is `spiral`. An installable
release bundles the native runtimes and a dependency-free Node launcher. The
private root `package.json` remains the browser-development project.

The first registry publication is a separate release step. Until it happens,
install a prepared `.tgz` from CI or a local build, not the source package:

```sh
npm install -g ./spiral-coder-0.2.0.tgz
cd your-project
spiral
```

After publication, replace the tarball with `npm install -g spiral-coder`.
No Rust compiler or install hook is required. `--ignore-scripts` is supported.

`spiral` and flag-only invocations select the TUI, defaulting its tool root to the
current directory. `-C`, `--root` and `--tool-root` override that location. Explicit
native subcommands go first (`spiral agent --provider openai`). `spiral-coder`
keeps native argument semantics, including its original no-argument Web UI.
Terminal streams, environment and exit status are inherited; signals are forwarded
to the native child. Configure API keys through the existing provider environment
variables or runtime settings; never bake them into a package.

## Supported first-release targets

| OS | Architecture | Rust target |
| --- | --- | --- |
| macOS | Apple Silicon | aarch64-apple-darwin |
| macOS | Intel | x86_64-apple-darwin |
| Linux, glibc 2.35+ | arm64 | aarch64-unknown-linux-gnu |
| Linux, glibc 2.35+ | x64 | x86_64-unknown-linux-gnu |
| Windows | x64 | x86_64-pc-windows-msvc |

The initial tarball includes all five binaries. This costs download/disk size but
avoids postinstall downloads and a family of platform packages to publish. The
launcher checks the selected executable against the manifest packaged alongside
it. This detects missing/corrupted files; it is not an independent publisher
signature. npm's package integrity remains the distribution trust boundary.

## Build and release

`npm native package` builds each target on its native GitHub runner, installs a
local tarball with scripts disabled, checks both commands, then assembles the
complete package. A PR produces downloadable artifacts; it never publishes.
For a release, run the workflow on the intended merged revision and obtain the
`spiral-coder-npm` artifact. Review its workflow revision and verify the extracted
files with `sha256sum -c SHA256SUMS` (macOS: `shasum -a 256 -c SHA256SUMS`), then an
authorized maintainer can run `npm publish ./spiral-coder-0.2.0.tgz --access public`.
Registry ownership/authentication and public publication are not automated here.

The builder requires all five nonempty binaries and matching Cargo/npm versions.
It refuses to overwrite an output directory. Its source template stays private;
only a complete release assembly removes that flag. To smoke-test just one native
build, use `--local-target`; that package stays private and cannot be published.

```sh
cargo build --locked --release
mkdir -p .tmp/native/aarch64-apple-darwin
cp target/release/spiral-coder .tmp/native/aarch64-apple-darwin/
node scripts/prepare-npm.mjs --input .tmp/native --out .tmp/npm-local --local-target aarch64-apple-darwin
npm pack ./.tmp/npm-local
npm install -g ./spiral-coder-0.2.0.tgz
```

Use the target matching your host in that example. Test the launcher without
provider requests using `node --test tests/npm/*.test.mjs`.

The `bin` mappings follow [npm's package manifest contract](https://docs.npmjs.com/cli/v11/configuring-npm/package-json/#bin).
Native build labels follow [GitHub's hosted-runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).
