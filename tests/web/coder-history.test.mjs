import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { mountApp, textOf } from './helpers/app.mjs';

async function send(app, text) {
  app.all(node => node.type === 'textarea' && node.props['aria-label'] === 'Coder')[0]
    .props.onChange({ target: { value: text } });
  app.render();
  await app.all(node => node.type === 'button' && textOf(node) === 'Send')[0].props.onClick();
  app.render();
}

for (const provider of ['openai-compatible', 'anthropic', 'hf']) {
  test(`${provider} ordinary Coder follow-up retains conversation without inheriting native proof`, async () => {
    const app = mountApp({ provider, codeProvider: provider });
    const first = 'Explain how a rainbow forms.';
    await send(app, first);
    const thread = app.threadState().threads[0];
    const workKey = 'spiral-coder.work.v1.' + thread.id;
    const oldWork = JSON.parse(app.storage.get(workKey));
    // An earlier completed native run is not evidence for the next human task.
    oldWork.messages = [{ role: 'tool', tool_call_id: 'old-check', content: 'OK (exit_code: 0)' }];
    oldWork.completed = true;
    app.storage.set(workKey, JSON.stringify(oldWork));
    await send(app, 'Explain that in more detail.');
    const calls = app.requests.filter(request => request.url === '/api/chat');
    assert.equal(calls.length, 2);
    assert.deepEqual(calls[1].body.history, [
      { role: 'user', content: first },
      { role: 'assistant', content: '[Observer] QA response' },
    ]);
    const newWork = JSON.parse(app.storage.get(workKey));
    assert.notEqual(newWork.taskId, oldWork.taskId);
    assert.deepEqual(newWork.messages, []);
    assert.equal(newWork.completed, null);
  });
}

test('streaming and its HTTP fallback receive the same preceding conversation', async () => {
  const app = mountApp({ stream: true }, { fetch: async url => url === '/api/chat_stream'
    ? { ok: false, status: 503, headers: { get: () => 'application/json' }, json: async () => ({ error: 'stream unavailable' }) }
    : { ok: true, headers: { get: () => 'application/json' }, json: async () => ({ content: 'A rainbow refracts light.' }) } });
  await send(app, 'Explain a rainbow.');
  await send(app, 'Explain that in more detail.');
  const calls = app.requests.filter(request => ['/api/chat_stream', '/api/chat'].includes(request.url));
  assert.equal(calls.length, 4);
  const expected = [
    { role: 'user', content: 'Explain a rainbow.' },
    { role: 'assistant', content: 'A rainbow refracts light.' },
  ];
  assert.deepEqual(calls[2].body.history, expected);
  assert.deepEqual(calls[3].body.history, expected);
});

test('native Coder starts new evidence and resumes only its correlated native exchanges', async () => {
  const json = data => ({ ok: true, headers: { get: () => 'application/json' }, json: async () => data });
  const app = mountApp({ forceAgent: true, coderMaxIters: 4, autonomy: 'off' }, { fetch: async (url, options) => {
    const body = options.body ? JSON.parse(options.body) : {};
    if (url === '/api/status') return json({ ok: true, features: { chat_tools: true } });
    if (url === '/assets/governor_contract.json') return json(JSON.parse(readFileSync(new URL('../../shared/governor_contract.json', import.meta.url), 'utf8')));
    if (url === '/api/observer_action') return json({ native_messages: body.messages, packet: {} });
    if (url === '/api/chat_tools_stream') throw new Error('Provider intentionally stopped after capturing the native request.');
    return json({ content: 'Previous conversation marker.' });
  } });
  await send(app, 'Explain a rainbow.');
  await app.all(node => node.type === 'button' && textOf(node) === 'Refresh')[0].props.onClick();
  app.render();
  await send(app, 'Read only: inspect note.txt. Do not run tests.');
  let nativeCalls = app.requests.filter(request => request.url === '/api/chat_tools_stream');
  assert.equal(nativeCalls.length, 1);
  assert.equal(JSON.stringify(nativeCalls[0].body.messages).includes('Previous conversation marker.'), false);
  assert.equal(JSON.stringify(nativeCalls[0].body.messages).includes('Explain a rainbow.'), false);
  const key = 'spiral-coder.work.v1.' + app.threadState().threads[0].id;
  const work = JSON.parse(app.storage.get(key));
  const exchange = [
    { role: 'assistant', tool_calls: [{ id: 'current-read', function: { name: 'read_file', arguments: '{"path":"note.txt"}' } }] },
    { role: 'tool', tool_call_id: 'current-read', content: '[note.txt] Current native result.' },
  ];
  work.messages.push(...exchange);
  app.storage.set(key, JSON.stringify(work));
  await send(app, 'Continue.');
  nativeCalls = app.requests.filter(request => request.url === '/api/chat_tools_stream');
  assert.equal(nativeCalls.length, 2);
  const resumed = nativeCalls[1].body.messages;
  assert.deepEqual(resumed.filter(message => message.tool_call_id === 'current-read' || message.tool_calls?.[0]?.id === 'current-read'), exchange);
  assert.equal(JSON.parse(app.storage.get(key)).taskId, work.taskId);
});
