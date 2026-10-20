import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import { mountApp, textOf } from './helpers/app.mjs';

const context = { window: {} };
vm.runInNewContext(readFileSync(new URL('../../web/core/work-loop.js', import.meta.url), 'utf8'), context);
const api = context.window.SpiralCoderWork;
const google = 'https://generativelanguage.googleapis.com/v1beta/openai';

test('Gemini schema and repair examples agree; both formats preserve exact commands', () => {
  assert.equal(api.doneEvidenceItems(google, 'openai').type, 'string');
  assert.equal(api.doneEvidenceItems('', 'gemini').type, 'string');
  assert.equal(api.doneEvidenceItems('https://api.openai.com/v1', 'openai').type, 'object');
  const command = 'printf "Case  sensitive" | grep "Case  sensitive"';
  const parsed = api.parseDoneEvidence([
    `acceptance 1 | ${command}`,
    { criterion: 'acceptance 2', command },
    'missing separator', { criterion: 3, command }, ' | no criterion',
  ]);
  assert.deepEqual(JSON.parse(JSON.stringify(parsed)), [
    { criterion: 'acceptance 1', command }, { criterion: 'acceptance 2', command },
  ]);
  const hint = api.doneEvidenceHint(google, 'openai', { acceptanceCriteria: ['tests pass', 'docs updated'] }, [command]);
  const data = JSON.parse(hint.slice(hint.indexOf('{"criteria"')));
  assert.equal(typeof data.evidence_row_shape, 'string');
  assert.equal(data.criteria[1].reference, 'acceptance 2');
  assert.equal(data.current_commands[0], command);
});

const plan = `<plan>
goal: Verify the configured tests and report the result.
steps:
1) Run the test suite.
2) Report the verification evidence.
acceptance:
1) The test suite passes.
risks: tests may fail
assumptions: the test runner is installed
</plan>`;
const think = tool => `<think>
goal: Verify the tests.
step: 1
tool: ${tool}
risk: incorrect evidence
doubt: none
next: ${tool === "exec" ? "python3 -m unittest" : "done"}
verify: successful native result
</think>
`;

test('actual Gemini App retries a rejected flat done, then persists successful native evidence', async () => {
  const json = data => new Response(JSON.stringify(data), { headers: { 'content-type': 'application/json' } });
  let turn = 0;
  const command = 'python3 -m unittest';
  const app = mountApp({
    forceAgent: true, coderMaxIters: 4, autonomy: 'off', requireEditApproval: false, requireCommandApproval: false,
    baseUrl: google, provider: 'openai', testCmd: command,
  }, { fetch: async (url, options) => {
    const body = options.body ? JSON.parse(options.body) : {};
    if (url === '/api/status') return json({ ok: true, features: { chat_tools: true } });
    if (url === '/api/governor_contract') return json(JSON.parse(readFileSync(new URL('../../shared/governor_contract.json', import.meta.url), 'utf8')));
    if (url === '/api/observer_action') return json({ native_messages: body.messages, packet: { task_obligations: { current_commands: body.messages.some(m => m.tool_call_id === 'native-1' && m.content.startsWith('OK (')) ? [command] : [] } }, completion_issue: null });
    if (url === '/api/exec') return json({ ok: true, exit_code: 0, stdout: 'Ran 1 test\nOK', stderr: '' });
    if (url === '/api/chat_tools_stream') {
      turn++;
      const tool = turn === 1 ? 'exec' : 'done';
      const args = turn === 1 ? { command } : {
        summary: 'Verified python3 -m unittest: PASS.',
        completed_acceptance: ['acceptance 1'], remaining_acceptance: [],
        acceptance_evidence: turn === 2 ? [] : [`acceptance 1 | ${command}`],
      };
      const text = (turn === 1 ? plan : '') + think(tool);
      const call = { id: `native-${turn}`, type: 'function', function: { name: tool, arguments: JSON.stringify(args) } };
      return new Response(`event: delta\ndata: ${JSON.stringify({ delta: text })}\n\nevent: finish\ndata: ${JSON.stringify({ finish_reason: 'tool_calls', tool_calls: [call] })}\n\n`, { headers: { 'content-type': 'text/event-stream' } });
    }
    return json({});
  } });
  await app.all(node => node.type === 'button' && textOf(node) === 'Refresh')[0].props.onClick();
  app.render();
  app.all(node => node.type === 'textarea' && node.props['aria-label'] === 'Coder')[0].props.onChange({ target: { value: 'Run python3 -m unittest to verify the existing test suite and report the result.' } });
  app.render();
  await app.all(node => node.type === 'button' && textOf(node) === 'Send')[0].props.onClick();
  app.render();
  const calls = app.requests.filter(request => request.url === '/api/chat_tools_stream');
  assert.equal(calls[0].body.tools.find(t => t.function.name === 'done').function.parameters.properties.acceptance_evidence.items.type, 'string');
  const work = JSON.parse(app.storage.get('spiral-coder.work.v1.' + app.threadState().threads[0].id));
  assert.equal(work.completed, true, JSON.stringify(work.messages.filter(m => m.role === 'tool')));
  assert.equal(calls.length, 3);
  assert.ok(calls[2].body.messages.some(m => m.tool_call_id === 'native-2' && m.content.includes('evidence_row_shape')));
  assert.ok(work.messages.some(m => m.tool_call_id === 'native-3' && m.content === 'OK: done'));
  assert.ok(app.requests.filter(r => r.url === '/api/observer_action').every(r => r.body.packet.coder_base_url === google));
});
