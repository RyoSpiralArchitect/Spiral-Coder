import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import vm from 'node:vm';

const context = { window: {} };
vm.runInNewContext(readFileSync(new URL('../../web/core/exec.js', import.meta.url), 'utf8'), context);
const { scaffoldCommand } = context.window.SpiralCoder.exec;

test('scaffold creates a usable Git repository without interpreting names or content as shell', { skip: process.platform === 'win32' }, (t) => {
  const root = mkdtempSync(path.join(tmpdir(), 'spiral-coder-scaffold-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const name = "demo-'$(touch INJECTED)'";
  const outside = mkdtempSync(path.join(tmpdir(), 'spiral-coder-cdpath-'));
  t.after(() => rmSync(outside, { recursive: true, force: true }));
  mkdirSync(path.join(outside, name));
  writeFileSync(path.join(outside, name, 'README.md'), 'Unrelated project\n');
  const files = {
    'README.md': "# UI QA\n\n日本語 / Français\n'quoted' $HOME `date` $(touch INJECTED)\n",
    '.gitignore': 'target/\n',
  };
  const env = { PATH: process.env.PATH, CDPATH: outside, GIT_CONFIG_GLOBAL: '/dev/null', GIT_CONFIG_NOSYSTEM: '1' };
  const command = scaffoldCommand({ name, files, windows: false });
  const first = spawnSync('bash', ['--noprofile', '--norc', '-c', command], { cwd: root, env, encoding: 'utf8' });
  assert.equal(first.status, 0, first.stderr);
  assert.deepEqual(readdirSync(root), [name]);
  const repo = path.join(root, name);
  assert.equal(readFileSync(path.join(repo, 'README.md'), 'utf8'), files['README.md']);
  assert.ok(readdirSync(repo).includes('src'));
  assert.ok(readdirSync(repo).includes('docs'));
  const branch = spawnSync('git', ['branch', '--show-current'], { cwd: repo, env, encoding: 'utf8' });
  assert.equal(branch.stdout.trim(), 'main');
  const status = spawnSync('git', ['status', '--porcelain'], { cwd: repo, env, encoding: 'utf8' });
  assert.equal(status.stdout, '');
  assert.equal(readFileSync(path.join(outside, name, 'README.md'), 'utf8'), 'Unrelated project\n');
  assert.deepEqual(readdirSync(path.join(outside, name)), ['README.md']);

  writeFileSync(path.join(repo, 'README.md'), 'User changes must survive\n');
  const again = spawnSync('bash', ['--noprofile', '--norc', '-c', command], { cwd: root, env, encoding: 'utf8' });
  assert.notEqual(again.status, 0);
  assert.equal(readFileSync(path.join(repo, 'README.md'), 'utf8'), 'User changes must survive\n');
});

test('scaffold rejects traversal and multiline names before generating either shell', () => {
  for (const windows of [true, false]) {
    for (const name of ['', '.', '..', '../outside', 'a/b', 'a\\b', 'a\nb', 'a\0b']) {
      assert.throws(() => scaffoldCommand({ name, files: { 'README.md': 'test' }, windows }), /single directory/);
    }
    assert.throws(() => scaffoldCommand({ name: 'demo', files: { '../outside': 'test' }, windows }), /simple file/);
  }
});
