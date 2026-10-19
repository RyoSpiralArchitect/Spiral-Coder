import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mountApp, textOf } from './helpers/app.mjs';

test('leaving before the debounce expires saves the latest visible conversation', async () => {
  const app = mountApp();
  const saveEffect = () => app.effects().find(effect => Array.isArray(effect.deps?.[0]?.threads));
  const cleanupOld = saveEffect().run();
  app.all(node => node.type === 'textarea' && node.props['aria-label'] === 'Coder')[0].props.onChange({ target: { value: 'Keep this conversation after reload.' } });
  app.render();
  await app.all(node => node.type === 'button' && textOf(node) === 'Send')[0].props.onClick();
  app.render();
  cleanupOld();
  const cleanup = saveEffect().run();
  // The harness deliberately never fires timeouts: pagehide must flush now.
  app.dispatchWindowEvent('pagehide');
  assert.deepEqual(JSON.parse(app.storage.get('spiral-coder.threads.v1')), JSON.parse(JSON.stringify(app.threadState().threads)));
  assert.equal(app.storage.get('spiral-coder.active.v1'), app.threadState().activeId);
  const saved = JSON.parse(app.storage.get('spiral-coder.threads.v1'));
  assert.ok(saved[0].messages.some(message => message.content === 'Keep this conversation after reload.'));
  assert.ok(saved[0].messages.some(message => message.content === '[Observer] QA response'));
  cleanup();
  app.storage.set('spiral-coder.threads.v1', 'new-owner');
  app.dispatchWindowEvent('pagehide');
  assert.equal(app.storage.get('spiral-coder.threads.v1'), 'new-owner');
});
