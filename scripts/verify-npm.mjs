#!/usr/bin/env node
// Validate an extracted release before publication. This command never publishes.
import { promises as fs } from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import assert from 'node:assert/strict';
const require = createRequire(import.meta.url);
const { targets, binaryPath } = require('../npm/spiral-coder/lib/launcher.cjs');
const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

export async function verifyRelease(root) {
  const template = path.join(repo, 'npm/spiral-coder');
  const expected = JSON.parse(await fs.readFile(path.join(template, 'package.json'), 'utf8'));
  delete expected.private;
  const info = JSON.parse(await fs.readFile(path.join(root, 'package.json'), 'utf8'));
  assert.deepEqual(info, expected, 'Release metadata must match the source template without private; no hooks or dependencies may be added.');
  const cargo = await fs.readFile(path.join(repo, 'Cargo.toml'), 'utf8');
  assert.equal(info.version, cargo.match(/^version = "([^"]+)"/m)?.[1], 'Cargo/npm version mismatch');
  assert.match(info.version, /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/, 'Invalid release version');
  const manifest = JSON.parse(await fs.readFile(path.join(root, 'native-manifest.json'), 'utf8'));
  assert.deepEqual(Object.keys(manifest.binaries).sort(), Object.values(targets).sort(), 'A release must include all five targets');
  const expectedFiles = ['package.json', 'native-manifest.json', 'README.md', 'LICENSE',
    'bin/spiral.cjs', 'bin/spiral-coder.cjs', 'lib/launcher.cjs'];
  for (const [platformArch, target] of Object.entries(targets)) {
    const [platform, arch] = platformArch.split('-');
    binaryPath(root, platform, arch); // Exact filename, version and SHA-256.
    expectedFiles.push(manifest.binaries[target].file);
  }
  const files = [];
  async function visit(dir) {
    for (const entry of await fs.readdir(path.join(root, dir), { withFileTypes: true })) {
      const file = path.posix.join(dir, entry.name);
      assert(!entry.isSymbolicLink(), `Symlink in release: ${file}`);
      if (entry.isDirectory()) await visit(file);
      else { assert(entry.isFile(), `Non-regular release file: ${file}`); files.push(file); }
    }
  }
  await visit('');
  assert.deepEqual(files.sort(), expectedFiles.sort(), 'Unexpected or missing files in release');
  for (const name of ['README.md', 'bin/spiral.cjs', 'bin/spiral-coder.cjs', 'lib/launcher.cjs', 'LICENSE']) {
    const source = name === 'LICENSE' ? path.join(repo, name) : path.join(template, name);
    assert.deepEqual(await fs.readFile(path.join(root, name)), await fs.readFile(source), `Source mismatch: ${name}`);
  }
  return { name: info.name, version: info.version, targets: Object.keys(manifest.binaries) };
}

export function verifyRegistryIntegrity(tarball, record) {
  const expected = `sha512-${createHash('sha512').update(tarball).digest('base64')}`;
  assert.equal(record?.dist?.integrity, expected, 'Published registry tarball differs from the verified artifact');
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const { values } = parseArgs({ options: {
      package: { type: 'string' }, tarball: { type: 'string' }, registry: { type: 'boolean', default: false },
    } });
    if (!values.package) throw new Error('Usage: node scripts/verify-npm.mjs --package <extracted-package> [--registry --tarball <published.tgz>]');
    const info = await verifyRelease(path.resolve(values.package));
    if (values.registry) {
      if (!values.tarball) throw new Error('--registry requires --tarball');
      const bytes = await fs.readFile(values.tarball);
      // Allow bounded registry propagation, never retry npm publish itself.
      for (let attempt = 0; ; attempt++) {
        try {
          const response = await fetch(`https://registry.npmjs.org/${encodeURIComponent(info.name)}/${encodeURIComponent(info.version)}`, { signal: AbortSignal.timeout(10000) });
          if (!response.ok) throw new Error(`Registry lookup returned HTTP ${response.status}`);
          const record = await response.json();
          assert.equal(record.name, info.name);
          assert.equal(record.version, info.version);
          verifyRegistryIntegrity(bytes, record);
          break;
        } catch (error) {
          if (attempt === 4) throw error;
          await new Promise(resolve => setTimeout(resolve, 2000));
        }
      }
    }
    console.log(JSON.stringify({ ...info, registry_verified: values.registry }));
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
