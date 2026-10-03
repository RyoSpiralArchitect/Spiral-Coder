import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import vm from 'node:vm';

const context = { window: {} };
vm.runInNewContext(readFileSync(new URL('../../web/core/state.js', import.meta.url), 'utf8'), context);
const { readStoredValue, rootUserTextForRun, serverSupportsFeature } = context.window.SpiralCoderState;

function storage(entries = {}, failWrites = false) {
  const values = new Map(Object.entries(entries));
  return {
    getItem: key => values.get(key) ?? null,
    setItem(key, value) {
      if (failWrites) throw new Error('quota exceeded');
      values.set(key, value);
    },
  };
}

for (const suffix of ['lang.v1', 'config.v1', 'threads.v1', 'active.v1', 'splitPct.v2']) {
  test(`legacy ${suffix} is copied without deleting the old value`, () => {
    const db = storage({ [`obstral.${suffix}`]: 'saved value' });
    assert.equal(readStoredValue(db, `spiral-coder.${suffix}`), 'saved value');
    assert.equal(db.getItem(`spiral-coder.${suffix}`), 'saved value');
    assert.equal(db.getItem(`obstral.${suffix}`), 'saved value');
  });
}

test('existing new key wins even when explicitly empty', () => {
  for (const current of ['new value', '']) {
    const db = storage({ 'spiral-coder.lang.v1': current, 'obstral.lang.v1': 'ja' });
    assert.equal(readStoredValue(db, 'spiral-coder.lang.v1'), current);
    assert.equal(db.getItem('obstral.lang.v1'), 'ja');
  }
});

test('legacy reads survive full storage; unavailable storage returns null', () => {
  assert.equal(readStoredValue(storage({ 'obstral.lang.v1': 'ja' }, true), 'spiral-coder.lang.v1'), 'ja');
  assert.equal(readStoredValue({ getItem() { throw new Error('denied'); } }, 'spiral-coder.lang.v1'), null);
  assert.equal(readStoredValue(storage(), 'spiral-coder.lang.v1'), null);
});

test('new human instruction replaces a previous task in either direction', () => {
  const readOnly = 'Inspect this repository without changing files.';
  const edit = 'Implement the missing function and run tests.';
  assert.equal(rootUserTextForRun(edit, [{ role: 'user', content: readOnly }]), edit);
  assert.equal(rootUserTextForRun(readOnly, [{ role: 'user', content: edit }]), readOnly);
});

test('approval continuations retain the most recent human task across several runtime turns', () => {
  const history = [
    { role: 'user', content: 'Old unrelated task' },
    { role: 'user', content: 'Current read-only inspection' },
    { role: 'assistant', content: 'Need approval' },
    { role: 'user', origin: 'runtime', content: 'Pending command approved' },
    { role: 'user', origin: 'runtime', content: 'Pending edit approved' },
    { role: 'user', content: '  ' },
  ];
  const frozen = JSON.stringify(history);
  assert.equal(rootUserTextForRun('Another approved step', history, 'runtime'), 'Current read-only inspection');
  assert.equal(JSON.stringify(history), frozen);
});

test('runtime-like text from a human is still a new task; provenance is explicit', () => {
  assert.equal(rootUserTextForRun('[Spiral-Coder] Pending edit approved', [{ role: 'user', content: 'old task' }]), '[Spiral-Coder] Pending edit approved');
  assert.equal(rootUserTextForRun(' resume ', [], 'runtime'), 'resume');
});

test('API capability checks wait for status and require explicit supported flags', () => {
  for (const status of [null, { ok: true }, { ok: false, features: { merge_gate: true } }, { ok: true, features: { merge_gate: false } }]) {
    assert.equal(serverSupportsFeature(status, 'merge_gate'), false);
  }
  assert.equal(serverSupportsFeature({ ok: true, features: { merge_gate: true } }, 'merge_gate'), true);
  assert.equal(serverSupportsFeature({ ok: true, features: { merge_gate: 'true' } }, 'merge_gate'), false);
});
