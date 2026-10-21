const fs = require('node:fs');
const path = require('node:path');
const { createHash } = require('node:crypto');
const { spawn } = require('node:child_process');
const { constants } = require('node:os');

const targets = Object.freeze({
  'darwin-arm64': 'aarch64-apple-darwin',
  'darwin-x64': 'x86_64-apple-darwin',
  'linux-arm64': 'aarch64-unknown-linux-gnu',
  'linux-x64': 'x86_64-unknown-linux-gnu',
  'win32-x64': 'x86_64-pc-windows-msvc',
});
const commands = new Set(['chat', 'agent', 'eval', 'merge-gate', 'tui-replay', 'coupled-run', 'inventory', 'promote-harness', 'review', 'init', 'repl', 'serve', 'tui', 'list', 'help']);

function argumentsFor(alias, args, cwd) {
  if (alias === 'spiral-coder' || ['--version', '-V'].includes(args[0])) return args;
  if (commands.has(args[0]) && args[0] !== 'tui') return args;
  const tuiArgs = args[0] === 'tui' ? args.slice(1) : args;
  const hasRoot = tuiArgs.some(a => ['-C', '--root', '--tool-root'].includes(a)
    || a.startsWith('--root=') || a.startsWith('--tool-root=') || (a.startsWith('-C') && a.length > 2));
  return ['tui', ...(hasRoot ? [] : ['--tool-root', cwd]), ...tuiArgs];
}

function binaryPath(root, platform = process.platform, arch = process.arch) {
  const target = targets[`${platform}-${arch}`];
  if (!target) throw new Error(`Unsupported platform ${platform}/${arch}. See the supported platforms in the package README.`);
  if (platform === 'linux' && platform === process.platform && !process.report.getReport().header.glibcVersionRuntime) {
    throw new Error('This release requires Linux with glibc; musl/Alpine is not supported. Use a source build.');
  }
  const info = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));
  let manifest;
  try { manifest = JSON.parse(fs.readFileSync(path.join(root, 'native-manifest.json'), 'utf8')); }
  catch { throw new Error('Native binaries are missing. Install a prepared Spiral-Coder npm tarball or registry release; the source package is not an installable release.'); }
  const entry = manifest.binaries?.[target];
  const filename = `vendor/${target}/spiral-coder${platform === 'win32' ? '.exe' : ''}`;
  if (manifest.format !== 1 || manifest.version !== info.version || entry?.file !== filename
      || !/^[a-f0-9]{64}$/.test(entry.sha256 || '')) {
    throw new Error(`No valid native binary manifest for ${target}. Reinstall the prepared package.`);
  }
  const binary = path.join(root, filename);
  let digest;
  try { digest = createHash('sha256').update(fs.readFileSync(binary)).digest('hex'); }
  catch { throw new Error(`The bundled ${target} executable is missing. Reinstall the package.`); }
  if (digest !== entry.sha256) throw new Error('The bundled executable failed its checksum. Reinstall the package.');
  return binary;
}

function main(alias) {
  const args = process.argv.slice(2);
  if (alias === 'spiral' && ['--help', '-h'].includes(args[0])) {
    console.log(`Spiral-Coder

Usage: spiral [TUI options]
       spiral <command> [options]

With no command, start the Coder + Observer TUI in the current directory.
  spiral                          Start here
  spiral -C /path/to/repo          Choose a workspace
  spiral --provider gemini         Choose a provider for the TUI
  spiral tui --help                List TUI options
  spiral serve                    Open the local Web UI
  spiral agent --help              Headless agent options
  spiral --version                Print the native runtime version

spiral-coder forwards all arguments to the original CLI unchanged.
Configure provider credentials with environment variables or the runtime UI.`);
    return;
  }
  try {
    const binary = binaryPath(path.resolve(__dirname, '..'));
    const child = spawn(binary, argumentsFor(alias, args, process.cwd()), { stdio: 'inherit', shell: false });
    const handlers = new Map(['SIGINT', 'SIGTERM', 'SIGHUP'].map(signal => [signal, () => child.kill(signal)]));
    for (const [signal, handler] of handlers) process.on(signal, handler);
    const cleanup = () => { for (const [signal, handler] of handlers) process.removeListener(signal, handler); };
    child.once('error', error => { cleanup(); console.error(`spiral: Could not start the native runtime: ${error.message}`); process.exitCode = 1; });
    child.once('exit', (code, signal) => { cleanup(); process.exitCode = code ?? (128 + (constants.signals[signal] || 1)); });
  } catch (error) {
    console.error(`spiral: ${error.message}`);
    process.exitCode = 1;
  }
}

module.exports = { targets, argumentsFor, binaryPath, main };
