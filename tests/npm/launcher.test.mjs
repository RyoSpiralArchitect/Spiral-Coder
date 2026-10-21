import test from 'node:test';
import assert from 'node:assert/strict';
import { promises as fs } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createRequire } from 'node:module';
import { spawn, spawnSync } from 'node:child_process';
import { prepare } from '../../scripts/prepare-npm.mjs';
const require = createRequire(import.meta.url);
const { targets, argumentsFor, binaryPath } = require('../../npm/spiral-coder/lib/launcher.cjs');

test('spiral enters the current project while native commands and explicit roots keep their meaning', () => {
  assert.deepEqual(argumentsFor('spiral', [], '/project with spaces'), ['tui', '--tool-root', '/project with spaces']);
  assert.deepEqual(argumentsFor('spiral', ['--provider', 'gemini'], '/repo'), ['tui', '--tool-root', '/repo', '--provider', 'gemini']);
  for (const root of [['-C', '/chosen'], ['--root=/chosen'], ['--tool-root', '/chosen'], ['-C/chosen']]) {
    assert.deepEqual(argumentsFor('spiral', root, '/repo'), ['tui', ...root]);
  }
  for (const args of [['--version'], ['agent', '--provider', 'openai'], ['serve'], ['chat', 'a; $(do not execute)']]) {
    assert.deepEqual(argumentsFor('spiral', args, '/repo'), args);
  }
  assert.deepEqual(argumentsFor('spiral-coder', [], '/repo'), []);
  assert.deepEqual(argumentsFor('spiral-coder', ['--provider', 'openai', 'agent'], '/repo'), ['--provider', 'openai', 'agent']);
});

async function fixture(t) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'spiral npm '));
  t.after(() => fs.rm(root, { recursive: true, force: true }));
  const input = path.join(root, 'input');
  for (const target of Object.values(targets)) {
    await fs.mkdir(path.join(input, target), { recursive: true });
    await fs.writeFile(path.join(input, target, `spiral-coder${target.includes('windows') ? '.exe' : ''}`), `fixture ${target}`);
  }
  return { root, input };
}

test('release assembly requires every platform, binds hashes, and never overwrites an artifact', async t => {
  const { root, input } = await fixture(t);
  const out = path.join(root, 'package');
  const manifest = await prepare({ input, out });
  assert.equal(Object.keys(manifest.binaries).length, 5);
  assert.equal(JSON.parse(await fs.readFile(path.join(out, 'package.json'))).private, undefined);
  for (const [platformArch, target] of Object.entries(targets)) {
    const [platform, arch] = platformArch.split('-');
    assert.equal(binaryPath(out, platform, arch), path.join(out, manifest.binaries[target].file));
  }
  await assert.rejects(prepare({ input, out }), /EEXIST/);
  const native = targets[`${process.platform}-${process.arch}`];
  await fs.appendFile(path.join(out, manifest.binaries[native].file), 'corrupted');
  assert.throws(() => binaryPath(out), /checksum/);
  assert.throws(() => binaryPath(out, 'aix', 'ppc64'), /Unsupported platform/);
  await fs.rm(path.join(input, 'aarch64-apple-darwin'), { recursive: true });
  await assert.rejects(prepare({ input, out: path.join(root, 'incomplete') }), /ENOENT/);
  await assert.rejects(fs.access(path.join(root, 'incomplete')), /ENOENT/);
});

test('a local smoke package stays private and a mismatched manifest cannot launch', async t => {
  const { root, input } = await fixture(t);
  const out = path.join(root, 'local');
  const target = targets[`${process.platform}-${process.arch}`];
  await prepare({ input, out, localTarget: target });
  assert.equal(JSON.parse(await fs.readFile(path.join(out, 'package.json'))).private, true);
  const filename = path.join(out, 'native-manifest.json');
  const manifest = JSON.parse(await fs.readFile(filename));
  manifest.binaries[target].file = '../../elsewhere';
  await fs.writeFile(filename, JSON.stringify(manifest));
  assert.throws(() => binaryPath(out), /manifest/);
});

test('launcher preserves literal arguments, cwd, environment, exit code and signals', { skip: process.platform === 'win32' }, async t => {
  const { root, input } = await fixture(t);
  const target = targets[`${process.platform}-${process.arch}`];
  const fake = `#!${process.execPath}\nconst fs=require('node:fs');\nfs.writeFileSync(process.env.RECEIPT,JSON.stringify({args:process.argv.slice(2),cwd:process.cwd(),marker:process.env.MARKER}));\nif(process.env.WAIT_SIGNAL){process.on('SIGTERM',()=>process.exit(23));setInterval(()=>{},1000);}else{process.exit(17);}\n`;
  await fs.writeFile(path.join(input, target, 'spiral-coder'), fake);
  const out = path.join(root, 'local');
  await prepare({ input, out, localTarget: target });
  const receipt = path.join(root, 'receipt.json');
  const entry = path.join(out, 'bin/spiral.cjs');
  const env = { ...process.env, RECEIPT: receipt, MARKER: 'inherited' };
  const actualRoot = await fs.realpath(root);
  const result = spawnSync(process.execPath, [entry, '--model', 'literal; $(not a shell)'], { cwd: root, env, encoding: 'utf8' });
  assert.equal(result.status, 17, result.stderr);
  assert.deepEqual(JSON.parse(await fs.readFile(receipt)), { args: ['tui', '--tool-root', actualRoot, '--model', 'literal; $(not a shell)'], cwd: actualRoot, marker: 'inherited' });
  await fs.rm(receipt);
  const child = spawn(process.execPath, [entry], { cwd: root, env: { ...env, WAIT_SIGNAL: '1' }, stdio: 'ignore' });
  t.after(() => child.kill('SIGKILL'));
  const exited = new Promise(resolve => child.once('exit', (code, signal) => resolve({ code, signal })));
  for (let i=0; i<100; i++) {
    if (await fs.access(receipt).then(() => true, () => false)) break;
    await new Promise(resolve => setTimeout(resolve, 20));
  }
  await fs.access(receipt);
  child.kill('SIGTERM');
  const timeout = setTimeout(() => child.kill('SIGKILL'), 3000);
  try { assert.deepEqual(await exited, { code: 23, signal: null }); } finally { clearTimeout(timeout); }
});
