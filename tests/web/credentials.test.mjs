import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import vm from 'node:vm';
import { mountApp, textOf } from './helpers/app.mjs';

const context = { window: {} };
vm.runInNewContext(readFileSync(new URL('../../web/core/state.js', import.meta.url), 'utf8'), context);
const { paneApiKey, credentialRoute } = context.window.SpiralCoderState;
const input = (app, name) => app.all(node => node.props.id === `setting-${name}`)[0];
function change(app, name, value) {
  input(app, name).props.onChange({ target: { value } });
  app.render();
}
function fillKeys(app) {
  change(app, 'apiKeyChat', 'fake-chat');
  change(app, 'apiKeyCode', 'fake-code');
  change(app, 'apiKeyObserver', 'fake-observer');
}
function preset(app, name) {
  app.all(node => node.type === 'button' && node.props.className?.includes('preset') && textOf(node).startsWith(name))[0].props.onClick();
  app.render();
}
const independentObserver = { observerProvider: 'anthropic', observerBaseUrl: 'https://observer.test/v1', observerModel: 'qa-observer' };

test('preset keeps workspace and independent Observer key, but drops changed Chat/Coder keys', () => {
  const app = mountApp({ ...independentObserver, toolRoot: '.tmp/custom' });
  fillKeys(app);
  preset(app, 'VIBE');
  assert.equal(input(app, 'apiKeyChat').props.value, '');
  assert.equal(input(app, 'apiKeyCode').props.value, '');
  assert.equal(input(app, 'apiKeyObserver').props.value, 'fake-observer');
  assert.equal(app.config().toolRoot, '.tmp/custom');
  assert.equal(app.config().observerProvider, 'anthropic');
});

test('provider select clears old Codestral overrides and changed keys after VIBE', () => {
  const app = mountApp(independentObserver);
  preset(app, 'VIBE');
  fillKeys(app);
  change(app, 'provider', 'openai-compatible');
  assert.equal(app.config().codeProvider, '');
  assert.equal(app.config().codeBaseUrl, '');
  assert.equal(app.config().codeModel, app.config().chatModel);
  assert.equal(input(app, 'apiKeyChat').props.value, '');
  assert.equal(input(app, 'apiKeyCode').props.value, '');
  assert.equal(input(app, 'apiKeyObserver').props.value, 'fake-observer');
});

test('unchanged credential destinations retain explicit keys', () => {
  const app = mountApp(independentObserver);
  fillKeys(app);
  preset(app, 'OpenAI');
  assert.equal(input(app, 'apiKeyChat').props.value, 'fake-chat');
  assert.equal(input(app, 'apiKeyCode').props.value, 'fake-code');
  assert.equal(input(app, 'apiKeyObserver').props.value, 'fake-observer');
  change(app, 'codeModel', 'another-model');
  assert.equal(input(app, 'apiKeyCode').props.value, 'fake-code');
});

test('endpoint/provider fields clear only credentials whose effective destination changes', () => {
  const app = mountApp({ ...independentObserver, codeProvider: 'codestral', codeBaseUrl: 'https://code.test/v1' });
  fillKeys(app);
  change(app, 'baseUrl', 'https://new-chat.test/v1');
  assert.equal(input(app, 'apiKeyChat').props.value, '');
  assert.equal(input(app, 'apiKeyCode').props.value, 'fake-code');
  assert.equal(input(app, 'apiKeyObserver').props.value, 'fake-observer');
  change(app, 'codeBaseUrl', 'https://new-code.test/v1');
  assert.equal(input(app, 'apiKeyCode').props.value, '');
  change(app, 'observerProvider', 'openai-compatible');
  assert.equal(input(app, 'apiKeyObserver').props.value, '');
});

test('inherited endpoints also invalidate explicit pane credentials when Chat URL changes', () => {
  const app = mountApp({ observerProvider: 'anthropic', observerBaseUrl: '' });
  fillKeys(app);
  change(app, 'baseUrl', 'https://new-inherited.test/v1');
  for (const name of ['apiKeyChat', 'apiKeyCode', 'apiKeyObserver']) assert.equal(input(app, name).props.value, '');
});

test('cross-pane fallback requires both matching provider and matching endpoint', () => {
  const config = {
    provider: 'openai-compatible', baseUrl: 'https://chat.test/v1',
    codeProvider: 'openai-compatible', codeBaseUrl: 'https://code.test/v1',
    observerProvider: 'anthropic', observerBaseUrl: 'https://chat.test/v1',
  };
  const keys = { code: 'fake-code', observer: 'fake-observer' };
  assert.equal(paneApiKey(config, keys, 'chat'), '');
  assert.equal(paneApiKey(config, keys, 'code'), 'fake-code');
  assert.equal(paneApiKey(config, keys, 'observer'), 'fake-observer');
  const inherited = { ...config, codeProvider: '', codeBaseUrl: '' };
  assert.equal(paneApiKey(inherited, keys, 'chat'), 'fake-code');
  assert.equal(credentialRoute(inherited, 'chat'), credentialRoute(inherited, 'code'));
});

test('legacy key fields are neither restored into config nor written back to storage', () => {
  const secrets = { apiKey: 'fake-legacy-chat', chatApiKey: 'fake-chat-alias', codeApiKey: 'fake-legacy-code', observerApiKey: 'fake-legacy-observer' };
  const app = mountApp({ ...secrets, toolRoot: '.tmp/preserved' });
  for (const name of Object.keys(secrets)) assert.equal(app.config()[name], undefined, name);
  for (const name of ['apiKeyChat', 'apiKeyCode', 'apiKeyObserver']) assert.equal(input(app, name).props.value, '');
  fillKeys(app);
  app.effects().find(effect => effect.deps?.length === 1 && effect.deps[0] === app.config()).run();
  const saved = JSON.parse(app.storage.get('spiral-coder.config.v1'));
  for (const name of Object.keys(secrets)) assert.equal(saved[name], undefined, name);
  assert.equal(saved.toolRoot, '.tmp/preserved');
  assert.ok(!JSON.stringify(saved).includes('fake-'));
});

test('Chat requests cannot fall back to the preserved independent Observer key after a preset', async () => {
  const app = mountApp(independentObserver);
  fillKeys(app);
  preset(app, 'VIBE');
  app.all(node => node.type === 'button' && node.props.className?.includes('obs-subtab') && textOf(node).includes('Chat'))[0].props.onClick();
  app.render();
  let field = app.all(node => node.type === 'textarea' && node.props['aria-label'] === 'Chat')[0];
  field.props.onChange({ target: { value: 'Hello' } });
  app.render();
  const composer = app.all(node => node.props.className === 'composer chat-composer')[0];
  await composer.children.find(node => node.type === 'button').props.onClick({ type: 'click' });
  assert.equal(app.requests.length, 1);
  assert.equal(app.requests[0].body.api_key, undefined);
  assert.equal(app.requests[0].body.provider, 'mistral');
});
