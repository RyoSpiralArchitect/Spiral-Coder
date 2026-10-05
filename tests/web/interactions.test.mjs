import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mountApp, textOf } from './helpers/app.mjs';

function composer(app, label) {
  return app.all(node => node.type === 'div' && node.props.className?.split(' ').includes('composer'))
    .find(node => node.children.some(child => child?.props?.['aria-label'] === label));
}

test('Observer button sends its draft instead of the click event and clears the composer', async () => {
  const app = mountApp();
  const draft = 'UI QA: この文章がそのまま送信されることを確認';
  composer(app, 'Observer').children.find(node => node.type === 'textarea').props.onChange({ target: { value: draft } });
  app.render();
  const button = composer(app, 'Observer').children.find(node => node.type === 'button');
  await button.props.onClick({ type: 'click', currentTarget: {} });
  app.render();
  assert.equal(composer(app, 'Observer').children.find(node => node.type === 'textarea').props.value, '');
  assert.equal(app.threadState().threads[0].messages.find(message => message.role === 'user').content, draft);
  assert.equal(app.requests.length, 1);
  assert.ok(app.requests[0].body.input.startsWith(draft + '\n\n'));
});

test('empty Observer Send click makes no request or conversation entry', async () => {
  const app = mountApp();
  await composer(app, 'Observer').children.find(node => node.type === 'button').props.onClick({ type: 'click' });
  assert.equal(app.requests.length, 0);
  assert.equal(app.threadState().threads[0].messages.length, 0);
});

for (const label of ['Coder', 'Observer', 'Chat']) {
  test(`${label} composer leaves IME confirmation and Shift+Enter in the input`, () => {
    const app = mountApp();
    if (label === 'Chat') {
      app.all(node => node.type === 'button' && node.props.className?.includes('obs-subtab') && textOf(node).includes('Chat'))[0].props.onClick();
      app.render();
    }
    let field = composer(app, label).children.find(node => node.type === 'textarea');
    field.props.onChange({ target: { value: '未確定の日本語' } });
    app.render();
    field = composer(app, label).children.find(node => node.type === 'textarea');
    for (const properties of [{ isComposing: true }, { nativeEvent: { isComposing: true } }, { keyCode: 229 }, { nativeEvent: { keyCode: 229 } }, { shiftKey: true }]) {
      field.props.onKeyDown({ key: 'Enter', ...properties, preventDefault() { assert.fail('must preserve native input behavior'); } });
    }
    assert.equal(app.requests.length, 0);
    assert.equal(app.threadState().threads[0].messages.length, 0);
    app.render();
    assert.equal(composer(app, label).children.find(node => node.type === 'textarea').props.value, '未確定の日本語');
  });
}

test('first Coder submission gives the default Thread 1 its actual title', async () => {
  const app = mountApp();
  const draft = 'Hello  from\nfirst thread';
  composer(app, 'Coder').children.find(node => node.type === 'textarea').props.onChange({ target: { value: draft } });
  app.render();
  const send = app.all(node => node.type === 'button' && textOf(node) === 'Send')[0];
  await send.props.onClick({ type: 'click' });
  assert.equal(app.threadState().threads[0].title, 'Hello from first thread');
});

test('thread rename waits for IME confirmation before Enter commits the title', () => {
  const app = mountApp();
  app.all(node => node.type === 'button' && textOf(node) === '✎')[0].props.onClick();
  app.render();
  let field = app.all(node => node.type === 'input' && node.props.autoFocus)[0];
  field.props.onChange({ target: { value: '日本語の名前' } });
  app.render();
  field = app.all(node => node.type === 'input' && node.props.autoFocus)[0];
  field.props.onKeyDown({ key: 'Enter', nativeEvent: { isComposing: true } });
  assert.equal(app.threadState().threads[0].title, 'Thread 1');
  field.props.onKeyDown({ key: 'Enter' });
  assert.equal(app.threadState().threads[0].title, '日本語の名前');
});

test('every visible Settings input and select has an associated label', () => {
  for (const config of [{}, { provider: 'mistral-cli' }]) {
    const app = mountApp(config);
    const labels = app.all(node => node.type === 'label' && node.props.htmlFor);
    const controls = app.all(node => ['input', 'select'].includes(node.type) && node.props.id?.startsWith('setting-'));
    assert.ok(controls.length >= 23);
    assert.equal(controls.length, labels.length);
    assert.equal(new Set(controls.map(node => node.props.id)).size, controls.length);
    for (const control of controls) {
      const label = labels.find(node => node.props.htmlFor === control.props.id);
      assert.ok(textOf(label).trim(), `missing label for ${control.props.id}`);
    }
    for (const group of app.all(node => node.props.role === 'group')) {
      assert.ok(app.all(node => node.props.id === group.props['aria-labelledby']).length === 1);
    }
  }
});

test('shortcuts open a named modal dialog with a focusable fallback', () => {
  const app = mountApp();
  app.all(node => node.type === 'button' && textOf(node) === '⌨')[0].props.onClick();
  app.render();
  const dialogs = app.all(node => node.props.role === 'dialog');
  assert.equal(dialogs.length, 1);
  assert.equal(dialogs[0].props['aria-modal'], true);
  assert.equal(dialogs[0].props.tabIndex, -1);
  assert.equal(app.all(node => node.props.id === dialogs[0].props['aria-labelledby']).length, 1);
});

test('provider presets preserve the workspace, approvals, Observer, and runtime preferences', () => {
  const kept = {
    toolRoot: '.tmp/ui-qa-custom', requireEditApproval: false, requireCommandApproval: false,
    observerProvider: 'anthropic', observerBaseUrl: 'http://localhost:9999', observerModel: 'qa-observer',
    observerIntensity: 'polite', observerLang: 'fr', observerPersona: 'duck',
    includeCoderContext: false, chatAutoTasks: false, chatAttachRuntime: false,
    coderMaxIters: '9', timeoutSeconds: '40', temperature: '0.2', maxTokens: '500',
  };
  const app = mountApp(kept);
  const preset = name => app.all(node => node.type === 'button' && node.props.className?.includes('preset') && textOf(node).startsWith(name))[0];
  preset('VIBE').props.onClick();
  app.render();
  assert.equal(app.config().codeProvider, 'codestral');
  preset('OpenAI').props.onClick();
  app.render();
  for (const [key, value] of Object.entries(kept)) assert.equal(app.config()[key], value, key);
  assert.equal(app.config().provider, 'openai-compatible');
  assert.equal(app.config().codeProvider, '', 'previous preset Coder routing must be cleared');
  assert.equal(app.config().codeBaseUrl, '');
});

test('status refresh reports HTTP failures and recovers after the server returns', async () => {
  let available = false;
  const app = mountApp({}, { fetch: async () => available
    ? { ok: true, json: async () => ({ ok: true, version: 'qa', features: {}, host_os: 'linux' }) }
    : { ok: false, status: 503 } });
  const refresh = () => app.all(node => node.type === 'button' && textOf(node) === 'Refresh')[0].props.onClick();
  await refresh();
  app.render();
  let indicator = app.all(node => node.props.role === 'status')[0];
  assert.equal(textOf(indicator), 'Disconnected · Refresh to retry');
  assert.ok(indicator.props.title.includes('HTTP 503'));
  available = true;
  await refresh();
  app.render();
  indicator = app.all(node => node.props.role === 'status')[0];
  assert.equal(textOf(indicator), 'Connected');
  assert.equal(indicator.props.title, undefined);
});

test('a stale failed status request cannot replace a newer successful refresh', async () => {
  let failOld;
  let calls = 0;
  const app = mountApp({}, { fetch: () => ++calls === 1
    ? new Promise((_, reject) => { failOld = reject; })
    : Promise.resolve({ ok: true, json: async () => ({ ok: true, features: {} }) }) });
  const refresh = () => app.all(node => node.type === 'button' && textOf(node) === 'Refresh')[0].props.onClick();
  const old = refresh();
  await refresh();
  failOld(new Error('Failed to fetch'));
  await old;
  app.render();
  assert.equal(textOf(app.all(node => node.props.role === 'status')[0]), 'Connected');
});
