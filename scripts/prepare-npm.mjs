#!/usr/bin/env node
// Assemble a self-contained release. No install scripts or runtime downloads.
import { promises as fs } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { parseArgs } from 'node:util';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const { targets } = require('../npm/spiral-coder/lib/launcher.cjs');
const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

export async function prepare({ input, out, localTarget }) {
  const template = path.join(repo, 'npm/spiral-coder');
  const info = JSON.parse(await fs.readFile(path.join(template, 'package.json'), 'utf8'));
  const cargo = await fs.readFile(path.join(repo, 'Cargo.toml'), 'utf8');
  if (cargo.match(/^version = "([^"]+)"/m)?.[1] !== info.version) throw new Error('Cargo and npm versions must agree.');
  const supported = Object.values(targets);
  if (localTarget && !supported.includes(localTarget)) throw new Error(`Unsupported local target: ${localTarget}`);
  const selected = localTarget ? [localTarget] : supported;
  // Validate all inputs before creating output; never silently ship a partial release.
  const binaries = [];
  for (const target of selected) {
    const name = `spiral-coder${target.includes('windows') ? '.exe' : ''}`;
    const source = path.join(input, target, name);
    if (!(await fs.lstat(source)).isFile()) throw new Error(`Not a regular binary file: ${source}`);
    const data = await fs.readFile(source);
    if (!data.length) throw new Error(`Empty binary: ${source}`);
    binaries.push({ target, data, file: `vendor/${target}/${name}` });
  }
  await fs.mkdir(out); // Exclusive: preserve existing package artifacts.
  for (const name of ['bin', 'lib']) await fs.cp(path.join(template, name), path.join(out, name), { recursive: true });
  await fs.copyFile(path.join(template, 'README.md'), path.join(out, 'README.md'));
  await fs.copyFile(path.join(repo, 'LICENSE'), path.join(out, 'LICENSE'));
  const manifest = { format: 1, version: info.version, binaries: {} };
  for (const { target, data, file } of binaries) {
    await fs.mkdir(path.dirname(path.join(out, file)), { recursive: true });
    await fs.writeFile(path.join(out, file), data, { mode: 0o755 });
    manifest.binaries[target] = { file, sha256: createHash('sha256').update(data).digest('hex') };
  }
  for (const name of ['spiral.cjs', 'spiral-coder.cjs']) await fs.chmod(path.join(out, 'bin', name), 0o755);
  // Only the complete five-platform bundle may be published. Local smoke packs
  // remain private; neither mode executes npm publish.
  if (!localTarget) delete info.private;
  await fs.writeFile(path.join(out, 'package.json'), JSON.stringify(info, null, 2) + '\n');
  await fs.writeFile(path.join(out, 'native-manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
  return manifest;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const { values } = parseArgs({ options: { input: { type: 'string' }, out: { type: 'string' }, 'local-target': { type: 'string' } } });
    if (!values.input || !values.out) throw new Error('Usage: node scripts/prepare-npm.mjs --input <target-directories> --out <new-package-directory> [--local-target <triple>]');
    const manifest = await prepare({ input: path.resolve(values.input), out: path.resolve(values.out), localTarget: values['local-target'] });
    console.log(JSON.stringify({ package: path.resolve(values.out), version: manifest.version, targets: Object.keys(manifest.binaries), publishable: !values['local-target'] }));
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
