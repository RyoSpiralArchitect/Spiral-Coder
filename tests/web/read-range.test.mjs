import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { mountApp, textOf } from './helpers/app.mjs';

test('Web forwards exact ranges and refuses whole-file overwrite after a partial read', async () => {
  const json = data => new Response(JSON.stringify(data), { headers: { 'content-type': 'application/json' } });
  let turn = 0;
  const app = mountApp({ forceAgent: true, coderMaxIters: 2, autonomy: 'off', requireEditApproval: false }, { fetch: async (url, options) => {
    const body = options.body ? JSON.parse(options.body) : {};
    if (url === '/api/status') return json({ ok: true, features: { chat_tools: true } });
    if (url === '/api/governor_contract') return json(JSON.parse(readFileSync(new URL('../../shared/governor_contract.json', import.meta.url), 'utf8')));
    if (url === '/api/observer_action') return json({ native_messages: body.messages, packet: {}, completion_issue: null });
    if (url === '/api/read_file') return json({ ok: true, content: '[note.txt] (1000 lines, 16000 bytes) [range: lines 900-910; partial file]\nold target\n', complete_read: false });
    if (url === '/api/stat_path') return json({ exists: true });
    if (url === '/api/chat_tools_stream') {
      turn++;
      const tool = turn === 1 ? 'read_file' : 'write_file';
      const args = turn === 1 ? { path: 'note.txt', start_line: 900, end_line: 910 } : { path: 'note.txt', content: 'new target\n' };
      const plan = '<plan>\ngoal: Repair note.txt and preserve unrelated content\nsteps:\n1) Read the target\n2) Edit the target\n3) Verify the edit\nacceptance:\n1) Target is corrected\nrisks: preserve surrounding lines\nassumptions: file exists\n</plan>\n';
      const think = `<think>\ngoal: repair note.txt\nstep: ${turn}\ntool: ${tool}\nrisk: data loss\ndoubt: none\nnext: inspect and repair the target\nverify: check unchanged surrounding lines\n</think>`;
      const call = { id: `range-${turn}`, type: 'function', function: { name: tool, arguments: JSON.stringify(args) } };
      return new Response(`event: delta\ndata: ${JSON.stringify({ delta: plan + think })}\n\nevent: finish\ndata: ${JSON.stringify({ finish_reason: 'tool_calls', tool_calls: [call] })}\n\n`, { headers: { 'content-type': 'text/event-stream' } });
    }
    return json({});
  } });
  await app.all(node => node.type === 'button' && textOf(node) === 'Refresh')[0].props.onClick();
  app.render();
  app.all(node => node.type === 'textarea' && node.props['aria-label'] === 'Coder')[0].props.onChange({ target: { value: 'Repair the target near line 900 of note.txt and preserve all surrounding lines.' } });
  app.render();
  await app.all(node => node.type === 'button' && textOf(node) === 'Send')[0].props.onClick();
  app.render();
  const reads = app.requests.filter(r => r.url === '/api/read_file');
  assert.equal(reads.length, 1);
  assert.deepEqual({ start_line: reads[0].body.start_line, end_line: reads[0].body.end_line }, { start_line: 900, end_line: 910 });
  assert.equal(app.requests.some(r => r.url === '/api/write_file' || r.url === '/api/queue_edit'), false);
  const work = JSON.parse(app.storage.get('spiral-coder.work.v1.' + app.threadState().threads[0].id));
  const refusal = work.messages.find(m => m.tool_call_id === 'range-2');
  assert.match(refusal.content, /Refusing to overwrite/);
  assert.notEqual(work.completed, true);
});
