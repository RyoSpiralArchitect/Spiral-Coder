import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { mountApp, textOf } from './helpers/app.mjs';

for (const [name, args] of [
  ['patch_file', '{"path":"note.txt","replace":"unfinished'],
  ['exec', 'printf unsafe > note.txt'],
  ['read_file', 'null'],
]) {
  test(`${name} invalid arguments stay unexecuted, correlated and budgeted`, async () => {
    const json = data => new Response(JSON.stringify(data), { headers: { 'content-type': 'application/json' } });
    const call = { id: 'bad-native', type: 'function', function: { name, arguments: args }, extra_content: { google: { thought_signature: 'opaque-test-marker' } } };
    const app = mountApp({ forceAgent: true, coderMaxIters: 2, autonomy: 'off' }, { fetch: async (url, options) => {
      const body = options.body ? JSON.parse(options.body) : {};
      if (url === '/api/status') return json({ ok: true, features: { chat_tools: true } });
      if (url === '/assets/governor_contract.json') return json(JSON.parse(readFileSync(new URL('../../shared/governor_contract.json', import.meta.url), 'utf8')));
      if (url === '/api/observer_action') return json({ native_messages: body.messages, packet: {} });
      if (url === '/api/chat_tools_stream') return new Response(`event: finish\ndata: ${JSON.stringify({ finish_reason: 'stop', tool_calls: [call] })}\n\n`, { headers: { 'content-type': 'text/event-stream' } });
      return json({});
    } });
    await app.all(node => node.type === 'button' && textOf(node) === 'Refresh')[0].props.onClick();
    app.render();
    app.all(node => node.type === 'textarea' && node.props['aria-label'] === 'Coder')[0].props.onChange({ target: { value: 'Inspect note.txt and report what remains.' } });
    app.render();
    await app.all(node => node.type === 'button' && textOf(node) === 'Send')[0].props.onClick();
    app.render();
    const calls = app.requests.filter(request => request.url === '/api/chat_tools_stream');
    assert.equal(calls.length, 2);
    assert.equal(app.requests.some(request => /\/api\/(exec|read_file|write_file|patch_file|apply_diff)$/.test(request.url)), false);
    const work = JSON.parse(app.storage.get('spiral-coder.work.v1.' + app.threadState().threads[0].id));
    assert.equal(work.requests.length, 2);
    assert.equal(work.completed, false);
    assert.deepEqual(work.messages.find(message => message.tool_calls)?.tool_calls, [call]);
    const refusal = work.messages.find(message => message.tool_call_id === call.id);
    assert.match(refusal.content, /^GOVERNOR BLOCKED\n\n\[Tool arguments\]/);
    assert.match(refusal.content, /No tool was executed/);
    assert.ok(calls[1].body.messages.some(message => message.tool_call_id === call.id && message.content.startsWith('GOVERNOR BLOCKED')));
  });
}
