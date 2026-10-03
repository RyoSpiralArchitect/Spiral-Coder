import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import vm from 'node:vm';

const context = { window: {} };
vm.runInNewContext(readFileSync(new URL('../../web/observer/logic.js', import.meta.url), 'utf8'), context);
const { hasNextActionAttempt } = context.window.SpiralCoder.observer;
const attempt = {
  pane: 'observer', role: 'assistant', metaKind: 'observer_next_action', metaTargetId: 'coder-failure-1',
};

test('completed or failed next-action responses survive reload as prior attempts', () => {
  for (const content of ['Try reading the file.', '[Error] HTTP 401', '[Stop]']) {
    const restored = JSON.parse(JSON.stringify([{ ...attempt, content, streaming: false }]));
    assert.equal(hasNextActionAttempt(restored, 'coder-failure-1'), true);
    assert.equal(hasNextActionAttempt(restored, 'coder-failure-2'), false);
  }
});

test('an interrupted request is not automatically submitted again after reload', () => {
  assert.equal(hasNextActionAttempt([{ ...attempt, content: '', streaming: true }], 'coder-failure-1'), true);
});

test('unrelated messages and user-entered labels do not suppress a new attempt', () => {
  for (const messages of [
    [], [{ ...attempt, role: 'user' }], [{ ...attempt, pane: 'coder' }],
    [{ ...attempt, metaKind: 'meta_diagnose' }],
    [{ pane: 'observer', role: 'assistant', content: '[NEXT-ACTION] target=coder-failure-1' }],
  ]) assert.equal(hasNextActionAttempt(messages, 'coder-failure-1'), false);
  assert.equal(hasNextActionAttempt(null, 'coder-failure-1'), false);
  assert.equal(hasNextActionAttempt([{ ...attempt, metaTargetId: '' }], ''), false);
});
