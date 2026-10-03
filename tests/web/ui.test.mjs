import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import vm from 'node:vm';

const context = { window: {} };
vm.runInNewContext(readFileSync(new URL('../../web/core/ui.js', import.meta.url), 'utf8'), context);
const { submitComposer, isAutomaticThreadTitle, focusDialog, transcriptMd } = context.window.SpiralCoderUI;

test('plain and modified Enter send once, while other keys remain native', () => {
  for (const modifiers of [{}, { ctrlKey: true }, { metaKey: true }, { ctrlKey: true, shiftKey: true }]) {
    let sent = 0;
    let prevented = 0;
    submitComposer({ key: 'Enter', ...modifiers, preventDefault: () => prevented++ }, () => sent++);
    assert.deepEqual({ sent, prevented }, { sent: 1, prevented: 1 });
  }
  submitComposer({ key: 'a', preventDefault: () => assert.fail('ordinary key consumed') }, () => assert.fail('ordinary key sent'));
});

test('only automatic thread titles are replaced; explicit titles stay intact', () => {
  for (const title of ['Untitled', 'Thread 1', 'Thread 23', 'Threads #2']) {
    assert.equal(isAutomaticThreadTitle(title, 'Threads #'), true);
  }
  for (const title of ['My review', 'Thread 1 custom name', 'Thread notes']) {
    assert.equal(isAutomaticThreadTitle(title, 'Threads #'), false);
  }
});

test('Markdown transcript uses real newlines and preserves message code escapes', () => {
  const thread = { messages: [
    { pane: 'coder', role: 'user', content: 'Review this\nplease' },
    { pane: 'observer', role: 'assistant', content: '```js\nconst newline = "\\n";\n```\n' },
    { pane: 'chat', role: 'assistant', content: 'Done' },
  ] };
  assert.equal(transcriptMd(thread, { thread: 'QA' }), [
    '# Spiral-Coder transcript', '', '```', 'thread: QA', '```', '',
    '## coder / user', '', 'Review this\nplease', '',
    '## observer / assistant', '', '```js\nconst newline = "\\n";\n```', '',
    '## chat / assistant', '', 'Done', '',
  ].join('\n'));
});

function dialogFixture({ empty = false } = {}) {
  const listeners = new Map();
  const doc = {
    body: { style: { overflow: 'auto' } },
    addEventListener: (name, callback) => listeners.set(name, callback),
    removeEventListener: name => listeners.delete(name),
  };
  const node = (name, options = {}) => ({
    name, tabIndex: 0, isConnected: true,
    focus() { doc.activeElement = this; },
    getClientRects: () => [1],
    ...options,
  });
  const trigger = node('trigger');
  const first = node('close');
  const disabled = node('disabled', { disabled: true });
  const hidden = node('hidden', { getClientRects: () => [] });
  const last = node('confirm');
  const background = [{ inert: false }, { inert: true }];
  const overlay = { parentElement: { children: [...background] } };
  overlay.parentElement.children.push(overlay);
  const controls = empty ? [disabled, hidden] : [first, disabled, hidden, last];
  const dialog = node('dialog', {
    ownerDocument: doc, parentElement: overlay,
    querySelectorAll: () => controls,
    contains: target => target === dialog || controls.includes(target),
  });
  trigger.focus();
  return { doc, listeners, trigger, first, last, dialog, background };
}

test('dialog moves focus in, wraps Tab, excludes background, and restores previous state', () => {
  const { doc, listeners, trigger, first, last, dialog, background } = dialogFixture();
  const close = focusDialog(dialog);
  assert.equal(doc.activeElement, first);
  assert.ok(background.every(node => node.inert));
  assert.equal(doc.body.style.overflow, 'hidden');
  let prevented = 0;
  const tab = (shiftKey = false) => listeners.get('keydown')({ key: 'Tab', shiftKey, preventDefault: () => prevented++ });
  tab(true);
  assert.equal(doc.activeElement, last);
  tab();
  assert.equal(doc.activeElement, first);
  assert.equal(prevented, 2);
  // Intermediate navigation stays native; disabled/hidden nodes are not endpoints.
  tab();
  assert.equal(prevented, 2);
  trigger.focus();
  listeners.get('focusin')({ target: trigger });
  assert.equal(doc.activeElement, first);
  close();
  assert.equal(doc.activeElement, trigger);
  assert.deepEqual(background.map(node => node.inert), [false, true]);
  assert.equal(doc.body.style.overflow, 'auto');
  assert.equal(listeners.size, 0);
});

test('dialog with no enabled controls keeps focus on its fallback', () => {
  const { doc, listeners, dialog } = dialogFixture({ empty: true });
  const close = focusDialog(dialog);
  assert.equal(doc.activeElement, dialog);
  let prevented = false;
  listeners.get('keydown')({ key: 'Tab', preventDefault: () => { prevented = true; } });
  assert.equal(prevented, true);
  assert.equal(doc.activeElement, dialog);
  close();
});
