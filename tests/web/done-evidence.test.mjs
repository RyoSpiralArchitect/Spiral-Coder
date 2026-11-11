import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
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
  const authored = [{ criterion: 'acceptance 1', command: '@unit' }];
  const checks = [{ id: 'unit', command }];
  assert.equal(api.resolveDoneCheckReferences(authored, [command], checks).rows[0].command, command);
  assert.equal(authored[0].command, '@unit');
  assert.match(api.resolveDoneCheckReferences(authored, [command], []).error, /no current successful receipt/);
  assert.equal(api.resolveDoneCheckReferences(authored, ['@unit', command], checks).rows[0].command, '@unit');
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

test('actual Gemini App retries a rejected flat done and keeps native check headers through pruning', async () => {
  const json = data => new Response(JSON.stringify(data), { headers: { 'content-type': 'application/json' } });
  let turn = 0;
  const command = 'python3 -m unittest';
  const commands = [command, 'pwd', 'ls', 'git status --short'];
  const app = mountApp({
    forceAgent: true, coderMaxIters: 6, autonomy: 'off', requireEditApproval: false, requireCommandApproval: false,
    baseUrl: google, provider: 'openai', testCmd: command,
  }, { fetch: async (url, options) => {
    const body = options.body ? JSON.parse(options.body) : {};
    if (url === '/api/status') return json({ ok: true, features: { chat_tools: true } });
    if (url === '/api/governor_contract') return json(JSON.parse(readFileSync(new URL('../../shared/governor_contract.json', import.meta.url), 'utf8')));
    if (url === '/api/observer_action') {
      const verified = body.messages.some(m => m.tool_call_id === 'native-1'
        && m.content.startsWith('OK (') && m.content.split('\n').includes('cwd: /workspace'));
      return json({ native_messages: body.messages, packet: { task_obligations: {
        current_commands: verified ? [command] : [],
        citable_checks: verified ? [{ id: 'unit', command }] : [],
      } }, completion_issue: null });
    }
    if (url === '/api/exec') {
      if (turn === 1 && process.platform !== 'win32') {
        const lines = body.command.split('\n');
        assert.equal(lines[0], command);
        assert.equal(lines.at(-1), '(exit "$__spiral_coder_status")');
        lines[0] = 'false';
        const quietFailure = spawnSync('sh', ['-c', lines.join('\n')], { encoding: 'utf8' });
        assert.equal(quietFailure.status, 1);
        assert.match(quietFailure.stdout, /__SPIRAL_CODER_PWD__=/);
      }
      const stdout = turn === 1 ? 'ordinary test output\n'.repeat(24) : '';
      const stderr = turn === 1 ? 'Ran 1 test (test result: ok)\nOK' : '';
      return json({ ok: true, cwd: '/workspace', exit_code: 0, stdout, stderr });
    }
    if (url === '/api/chat_tools_stream') {
      turn++;
      const tool = turn <= commands.length ? 'exec' : 'done';
      const args = tool === 'exec' ? { command: commands[turn - 1] } : {
        summary: 'Verified python3 -m unittest: PASS.',
        completed_acceptance: ['acceptance 1'], remaining_acceptance: [],
        acceptance_evidence: turn === commands.length + 1 ? [] : ['acceptance 1 | @unit'],
      };
      const text = (turn === 1 ? plan : '') + think(tool).replace('next: ' + command, 'next: ' + (args.command || 'done'));
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
  assert.equal(calls.length, 6);
  assert.ok(calls[5].body.messages.some(m => m.tool_call_id === 'native-5' && m.content.includes('evidence_row_shape')));
  assert.ok(work.messages.some(m => m.tool_call_id === 'native-6' && m.content === 'OK: done'));
  assert.ok(work.messages.some(m => m.tool_calls?.some(call => call.id === 'native-6' && call.function.arguments.includes('@unit'))));
  const receipt = work.messages.find(m => m.tool_call_id === 'native-1')?.content;
  assert.match(receipt, /^OK \(exit_code: 0\) \[pruned \d+L\]\nduration_ms: 0\ncwd: \/workspace(?:\ncwd_after: [^\n]+)?$/);
  const recentReceipt = calls[1].body.messages.find(m => m.tool_call_id === 'native-1')?.content;
  assert.match(recentReceipt, /\[history digest — kept \d+\/\d+ lines, \d+ chars\]/);
  assert.match(recentReceipt, /stderr: Ran 1 test/, 'preserve recent successful unittest output in native proof');
  assert.ok(app.requests.filter(r => r.url === '/api/observer_action').every(r => r.body.packet.coder_base_url === google));
});

test('App refuses an edit before the native required baseline without dispatching it', async () => {
  const json = data => new Response(JSON.stringify(data), { headers: { 'content-type': 'application/json' } });
  const app = mountApp({ forceAgent: true, coderMaxIters: 1, autonomy: 'off' }, {
    fetch: async (url, options) => {
      const body = options.body ? JSON.parse(options.body) : {};
      if (url === '/api/status') return json({ ok: true, features: { chat_tools: true } });
      if (url === '/api/governor_contract') return json(JSON.parse(readFileSync(new URL('../../shared/governor_contract.json', import.meta.url), 'utf8')));
      if (url === '/api/observer_action') return json({ packet: { task_obligations: { task_requirements: { pending_baselines: ['python3 acceptance.py'] } } }, native_messages: body.messages });
      if (url === '/api/chat_tools_stream') {
        const call = { id: 'early-edit', type: 'function', function: { name: 'patch_file', arguments: JSON.stringify({ path: 'runner.py', search: 'old', replace: 'new' }) } };
        const text = plan + think('patch_file').replace('next: done', 'next: patch_file runner.py');
        return new Response(`event: delta\ndata: ${JSON.stringify({ delta: text })}\n\nevent: finish\ndata: ${JSON.stringify({ finish_reason: 'tool_calls', tool_calls: [call] })}\n\n`, { headers: { 'content-type': 'text/event-stream' } });
      }
      return json({});
    },
  });
  await app.all(node => node.type === 'button' && textOf(node) === 'Refresh')[0].props.onClick(); app.render();
  app.all(node => node.type === 'textarea' && node.props['aria-label'] === 'Coder')[0].props.onChange({ target: { value: 'Run the tests and fix runner.py.' } }); app.render();
  await app.all(node => node.type === 'button' && textOf(node) === 'Send')[0].props.onClick();
  const work = JSON.parse(app.storage.get('spiral-coder.work.v1.' + app.threadState().threads[0].id));
  assert.ok(work.messages.some(m => m.tool_call_id === 'early-edit' && m.content.includes('[Requirement Gate]')));
  assert.ok(!app.requests.some(r => r.url === '/api/patch_file' || r.url === '/api/pending_edits/queue'));
  assert.equal(work.completed, false);
  assert.equal(api.baselineMutationIssue({ required_exact_checks: ['python3 acceptance.py'], task_requirements: { pending_baselines: ['python3 acceptance.py'] } }, 'exec', { command: 'python3 acceptance.py' }, 'action'), null);
});
