# npm distribution and the `spiral` terminal command

The npm package is `spiral-coder`; its short command is `spiral`. An installable
release bundles the native runtimes and a dependency-free Node launcher. The
private root `package.json` remains the browser-development project.

## Install and use

Requires **Node.js 20+**, with npm available in your terminal.

```sh
npm install -g spiral-coder@latest
cd your-project
spiral
```

The [npm package](https://www.npmjs.com/package/spiral-coder) bundles the native
executables. Rerun the install command to update. To pin this release, use
`npm install -g spiral-coder@0.2.0`.

For a preview, download and unzip the `spiral-coder-npm` artifact from a successful
[npm native package workflow run](https://github.com/RyoSpiralArchitect/Spiral-Coder/actions/workflows/npm-package.yml)
and install the enclosed archive with `npm install -g ./spiral-coder-0.2.0.tgz`.

No Rust compiler, install hook, or additional executable download is required.
Both methods support `--ignore-scripts`. Preview updates use the newer `.tgz`.
`npm uninstall -g spiral-coder` removes the npm installation; it leaves your
projects and Spiral-Coder configuration/session data in place.

| Command | Behavior |
| --- | --- |
| `spiral` | Open the TUI with the current directory as tool root |
| `spiral -C /path/to/project` | Choose the project explicitly (`--root` and `--tool-root` also work) |
| `spiral --provider gemini` | Select a provider for the coding loop |
| `spiral --lang ja` | Select the UI language (`en`, `ja`, or `fr`) |
| `spiral tui --help` | List TUI options |
| `spiral serve` | Start the local Web UI |
| `spiral agent --help` | List headless agent options |
| `spiral --version` | Show the installed native version |

Flag-only invocations select the TUI. Explicit native subcommands go first, for
example `spiral agent --provider openai`. `spiral-coder` is installed too and keeps
the native CLI's original argument semantics, including its no-argument Web UI.
Terminal streams, working directory, environment and exit status are inherited;
signals are forwarded to the native child. Ctrl+C exits the TUI.

Configure your provider before making model requests. Common environment
variables are `MISTRAL_API_KEY`, `OPENAI_API_KEY`, and `GEMINI_API_KEY` (or
`GOOGLE_API_KEY`). Set them in the shell that launches `spiral`; the TUI's `/keys`
command shows setup help and key status. `/provider` and `/model` change the
selection. **Coder + Observer form one coding loop** and use the same selected
model unless explicitly overridden. See the [Quickstart](../README.md#quickstart)
for provider configuration. Never put credentials in a package.

### Installation troubleshooting

- **`spiral` is not found:** inspect `npm prefix -g`. On macOS/Linux, its `bin/`
  directory must be on `PATH`; on Windows, the prefix directory itself must be
  on `PATH`. Open a new terminal after changing Node/npm or PATH. Check with
  `command -v spiral` on macOS/Linux or `Get-Command spiral` in PowerShell.
- **Global install permission error:** use a Node version manager or a
  user-owned npm prefix, following [npm's permissions guide](https://docs.npmjs.com/resolving-eacces-permissions-errors-when-installing-packages-globally/).
  A one-off Unix install can use `npm install -g --prefix "$HOME/.local" spiral-coder@latest`,
  with `$HOME/.local/bin` on PATH. Use that same prefix for updates/removal.
- **PowerShell blocks script execution:** run `spiral.cmd` (the npm-generated
  command wrapper). There is no need to change the machine's execution policy.
- **Unsupported platform or checksum error:** check the table below. Reinstall
  a complete package for a damaged install; do not disable checksum verification.
  Alpine/musl and Windows arm64 need a source build in this release.
- **Missing API key:** run `/keys`, set the indicated environment variable and
  restart `spiral`. Installing the package does not configure provider accounts.

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

## Build and verify a release

`npm native package` builds each target on its native GitHub runner, installs a
local tarball with scripts disabled, checks both commands, then assembles the
complete package. PR runs only build and test. A manual run also defaults to
build-only (`publish: false`). Its `spiral-coder-npm` artifact contains the `.tgz`
and `SHA256SUMS`, retained for 14 days. Download it from the run page or use
`gh run download RUN_ID -R RyoSpiralArchitect/Spiral-Coder -n spiral-coder-npm -D .tmp/release-download`
with the successful run's numeric ID.

Use a checkout of the **same revision** as the workflow run. Verify the archive
checksum in its download directory (`sha256sum -c SHA256SUMS` on Linux;
`shasum -a 256 -c SHA256SUMS` on macOS). Then, from the checkout:

```sh
mkdir -p .tmp/release-unpacked
tar -xzf .tmp/release-download/spiral-coder-0.2.0.tgz -C .tmp/release-unpacked
node scripts/verify-npm.mjs --package .tmp/release-unpacked/package
npm publish .tmp/release-download/spiral-coder-0.2.0.tgz --dry-run --ignore-scripts --access public
```

The verifier checks matching Cargo/npm versions, exact package metadata, all
five binary hashes, the file allowlist, and the bundled launcher/README/license
against the checkout. It rejects private local packages and added install hooks
or dependencies. CI runs it both before packing and on the extracted tarball;
the publish job repeats verification on the artifact it will send to npm.

For a local assembly, place the five binaries at
`.tmp/native/<rust-target>/spiral-coder` (`spiral-coder.exe` for Windows):

```sh
node scripts/prepare-npm.mjs --input .tmp/native --out .tmp/npm-release
node scripts/verify-npm.mjs --package .tmp/npm-release
mkdir -p .tmp/tarballs
npm pack ./.tmp/npm-release --pack-destination .tmp/tarballs
```

The builder requires all five nonempty regular files and matching Cargo/npm
versions. It refuses to overwrite an output directory. Its source template stays
private; only a complete assembly removes that flag. To smoke-test one native
build on Apple Silicon instead:

```sh
cargo build --locked --release
mkdir -p .tmp/native/aarch64-apple-darwin .tmp/local-tarballs
cp target/release/spiral-coder .tmp/native/aarch64-apple-darwin/
node scripts/prepare-npm.mjs --input .tmp/native --out .tmp/npm-local --local-target aarch64-apple-darwin
npm pack ./.tmp/npm-local --pack-destination .tmp/local-tarballs
npm install -g ./.tmp/local-tarballs/spiral-coder-0.2.0.tgz
```

Use your host's target and executable name in that example. A local-target
package stays private and fails release verification. Run `npm run test:npm` for
launcher, packaging and integrity tests without provider requests.

## Manual publication

An authorized maintainer can authenticate with npm and publish a verified
artifact from a green, merged revision. The first publication establishes the
package and its maintainer access. The repository contains no npm credentials.

Before uploading, create and verify an npm account and
[enable two-factor authentication](https://docs.npmjs.com/configuring-two-factor-authentication/)
for direct publication. Completing the email login challenge alone does not
enable account 2FA. Register a passkey or security key in npm's account settings
and keep the recovery codes in your own secure storage.

After the verification steps above, from that same checkout:

```sh
npm login --registry=https://registry.npmjs.org
npm whoami --registry=https://registry.npmjs.org
npm profile get "two-factor auth" --registry=https://registry.npmjs.org
npm publish .tmp/release-download/spiral-coder-0.2.0.tgz --ignore-scripts --access public --registry=https://registry.npmjs.org --fetch-retries=0 --fetch-timeout=1200000
node scripts/verify-npm.mjs --package .tmp/release-unpacked/package --registry --tarball .tmp/release-download/spiral-coder-0.2.0.tgz
```

Complete account setup if the 2FA check prints `disabled`. Complete any browser
authentication requested by the publish command using that account.

The upload allows up to 20 minutes for the bundled native archive and disables
automatic HTTP retries. A transport error does not establish whether npm accepted
the version: reconcile registry integrity before deciding whether to try again.
If a local upload needs more time, a subsequent manual attempt can increase
`--fetch-timeout` (for example, `3600000` for one hour) after checking registry
state. Keep automatic retries disabled.

The last command compares the published version's SHA-512 integrity with the
exact uploaded `.tgz`, with bounded retries for registry propagation. If publish
returns an uncertain result, inspect the registry before taking any further
publication action; npm versions cannot be overwritten. Confirm a clean
`npm install -g spiral-coder@0.2.0 --ignore-scripts` and `spiral --version` with a
fresh prefix/cache and no npm authentication. Record the published source,
archive hash, registry integrity and clean-install result before announcing it.

### Upload first, then authenticate publication

For a slow connection, [npm staged publishing](https://docs.npmjs.com/staged-publishing/)
separates the upload from the final 2FA challenge. It requires Node.js 22.14+ and
npm 11.15+ for publication; consumer installation still requires only Node.js 20+.
Use the same verified archive and keep the returned stage ID:

```sh
npm stage publish .tmp/release-download/spiral-coder-0.2.0.tgz --ignore-scripts --access public --tag latest --fetch-retries=0 --fetch-timeout=3600000
npm stage list spiral-coder --json
npm stage view STAGE_ID --json
mkdir -p .tmp/staged-download
(cd .tmp/staged-download && npm stage download STAGE_ID)
cmp .tmp/release-download/spiral-coder-0.2.0.tgz .tmp/staged-download/spiral-coder-0.2.0-STAGE_ID.tgz
npm stage approve STAGE_ID --fetch-retries=0
node scripts/verify-npm.mjs --package .tmp/release-unpacked/package --registry --tarball .tmp/release-download/spiral-coder-0.2.0.tgz
```

Replace `STAGE_ID` with the returned ID. Check the name, version and `latest` tag,
and require an identical downloaded archive before approving. Approval prompts
for 2FA without reuploading the archive. If a challenge expires, reconcile the
stage and registry state before requesting approval again.

For a new package, npm creates a public `0.0.0-stage` placeholder; the actual
staged release becomes public only after approval. If staging returns an
uncertain result, inspect the stage list before another upload: staged and
published versions share the same version uniqueness constraint.

## Subsequent releases with a trusted publisher

The workflow includes an opt-in publish job using npm's GitHub Actions OIDC
integration. After the package exists, configure **Settings → Trusted Publisher**
on npm with these exact values:

| Setting | Value |
| --- | --- |
| Provider | GitHub Actions |
| Organization or user | `RyoSpiralArchitect` |
| Repository | `Spiral-Coder` |
| Workflow filename | `npm-package.yml` |
| Environment | `npm` |
| Allowed publish action | `npm publish` |

Use the filename only, without `.github/workflows/`. Enable `npm publish` in npm's
publisher settings; a staging-only configuration will not authorize this job.
Configure the matching GitHub environment `npm` if it needs release protections.
The publish job uses a GitHub-hosted runner, Node 24, npm 11.9.0 and
`id-token: write`, with no long-lived npm token. Follow npm's
[trusted publishing guide](https://docs.npmjs.com/trusted-publishers/) for account
setup and current activation requirements.

For each release, bump `Cargo.toml`, the root package entry in `Cargo.lock`, and
`npm/spiral-coder/package.json` together; update versioned documentation examples.
Merge the tested change, then dispatch **npm native package** on `main` with
**publish = true**. The workflow rejects publication from another branch or
repository, rebuilds/tests all five targets, verifies the resulting archive,
publishes once with provenance, and checks registry integrity. A normal PR or
build-only dispatch cannot enter the publish job. The workflow does not create
Git tags or GitHub releases.

The `bin` mappings follow [npm's package manifest contract](https://docs.npmjs.com/cli/v11/configuring-npm/package-json/#bin).
Publication follows [npm publish](https://docs.npmjs.com/cli/v11/commands/npm-publish/).
Native build labels follow [GitHub's hosted-runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).
