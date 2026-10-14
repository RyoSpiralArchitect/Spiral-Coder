import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('../../web/core/work-loop.js', import.meta.url), 'utf8');
function setup(entries = new Map()) {
  const storage = { getItem: k => entries.get(k), setItem: (k, v) => entries.set(k, v) };
  const context = { window: {} };
  vm.runInNewContext(source, context);
  return { api: context.window.SpiralCoderWork, storage, entries };
}

test('Coder, Observer and repair share a durable cap across browser reload', () => {
  const { api, storage, entries } = setup();
  const work = api.open(storage, 'thread', 'original task', 3, true);
  for (const role of ['coder', 'observer', 'observer_repair']) api.reserve(storage, 'thread', work, role, [{ role: 'user', content: 'task' }]);
  const reloaded = setup(entries);
  const continued = reloaded.api.open(reloaded.storage, 'thread', 'original task', 64, false);
  assert.equal(continued.limit, 3);
  assert.throws(() => reloaded.api.reserve(reloaded.storage, 'thread', continued, 'coder', []), /exhausted/);
  assert.equal(continued.requests[0].providerUsage, null);
});

test('native tool IDs survive reload and stale advice cannot be claimed twice', () => {
  const { api, storage, entries } = setup();
  const work = api.open(storage, 'thread', 'inspect', 4, true);
  const messages = [{ role: 'assistant', tool_calls: [{ id: 'real-call', function: { name: 'read_file', arguments: '{"path":"a"}' } }] }, { role: 'tool', tool_call_id: 'real-call', content: '[a] actual result' }];
  api.snapshot(storage, 'thread', work, messages);
  const revision = work.revision;
  assert.equal(api.isCurrent(storage, 'thread', work.taskId, revision), true);
  assert.equal(api.claimAdvice(storage, 'thread', work, 'assist-A'), true);
  assert.equal(api.claimAdvice(storage, 'thread', work, 'assist-A'), false);
  assert.equal(setup(entries).api.read(storage, 'thread').messages[1].tool_call_id, 'real-call');
  api.snapshot(storage, 'thread', work, [...messages, { role: 'user', origin: 'runtime', content: 'Continue.' }]);
  assert.equal(api.isCurrent(storage, 'thread', work.taskId, revision), false);
});

test('a late old-task snapshot cannot overwrite a new human task', () => {
  const { api, storage } = setup();
  const old = api.open(storage, 'thread', 'old task', 3, true);
  const current = api.open(storage, 'thread', 'new review finding', 4, true);
  api.snapshot(storage, 'thread', old, [{ role: 'assistant', content: 'old completion' }], true);
  assert.equal(api.read(storage, 'thread'), current);
  assert.equal(current.messages.length, 0);
  assert.equal(current.completed, null);
  assert.throws(() => api.reserve(storage, 'thread', old, 'coder', []), /replaced/);
});

test('native stop calls and a later usage trailer survive fragmented SSE', async () => {
  const stream = 'event: delta\ndata: {"delta":"hello"}\n\nevent: finish\ndata: {"finish_reason":"stop","tool_calls":[{"id":"c1","function":{"name":"read_file","arguments":"{}"}}]}\n\nevent: usage\ndata: {"prompt_tokens":7,"completion_tokens":2}\n\n';
  const chunks = [stream.slice(0,17), stream.slice(17,73), stream.slice(73)];
  const context = { window: {}, TextDecoder, fetch: async () => ({ok:true, body: new ReadableStream({start(controller) { for (const chunk of chunks) controller.enqueue(new TextEncoder().encode(chunk)); controller.close(); }})}) };
  vm.runInNewContext(source, context);
  let text = '';
  const result = await context.window.SpiralCoderWork.streamChatTools({}, undefined, delta => {text += delta;});
  assert.equal(text, 'hello');
  assert.equal(result.finishReason, 'stop');
  assert.equal(result.toolCalls[0].id, 'c1');
  assert.equal(result.usage.prompt_tokens, 7);
});

test('an explicit test command is not rejected because the requested report says inspection', () => {
  const app = readFileSync(new URL('../../web/app.js', import.meta.url), 'utf8');
  const start = app.indexOf('  function isRootReadOnlyObservationTask(');
  const end = app.indexOf('  function inferTaskVerificationFloor(', start);
  const context = {};
  vm.runInNewContext(app.slice(start, end), context);
  assert.equal(context.isRootReadOnlyObservationTask('Read note.txt, run node --test note.test.mjs. Final answer must include inspection complete.'), false);
  assert.equal(context.isRootReadOnlyObservationTask('Read only: inspect note.txt. Do not run tests.'), true);
});

test('approval result is bound to the native call and cannot be reused for a replaced task', () => {
  const {api, storage} = setup();
  const work = api.open(storage, 'thread', 'verify', 8, true);
  api.snapshot(storage, 'thread', work, [{role:'assistant',tool_calls:[{id:'call-A',function:{name:'exec',arguments:'{"command":"node --test note.test.mjs"}'}}]}, {role:'tool',tool_call_id:'call-A',content:'Awaiting approval via /api/approve_command\napproval_id: pc_1'}]);
  const item = {id:'pc_1',command:'node --test note.test.mjs',cwd:'.',result:{exit_code:0,stdout:'passed',stderr:''}};
  assert.equal(api.resolveApproval(storage, 'thread', 'pc_1', {...item, command:'different command'}, true), false);
  assert.equal(api.resolveApproval(storage, 'thread', 'pc_1', item, true), true);
  assert.match(work.messages[1].content, /^OK \(exit_code: 0\)/);
  assert.equal(api.resolveApproval(storage, 'thread', 'pc_1', item, true), false);
  api.open(storage, 'thread', 'new task', 8, true);
  assert.equal(api.resolveApproval(storage, 'thread', 'pc_1', item, true), false);
});

test('the shared verification catalog recognizes Node native tests', () => {
  const contract = JSON.parse(readFileSync(new URL('../../shared/governor_contract.json', import.meta.url), 'utf8'));
  const app = readFileSync(new URL('../../web/app.js', import.meta.url), 'utf8');
  const pick = name => { const start=app.indexOf('  function ' + name + '('); const end=app.indexOf('\n  function ',start+10); return app.slice(start,end); };
  const context={DEFAULT_GOVERNOR_CONTRACT:contract, normalizeScratchEntry:s=>String(s||'').trim().toLowerCase().replace(/\s+/g,' ')};
  vm.runInNewContext(['contractVerification','verificationTerms','signatureMatchesVerificationTerms','verificationLevelForCommand','toolContentSucceeded','isVerificationCommand','restoreKnownVerificationCommandsFromMessages'].map(pick).join('\n'),context);
  const history=[{role:'assistant',tool_calls:[{id:'verified',function:{name:'exec',arguments:'{"command":"node --test note.test.mjs"}'}}]},{role:'tool',tool_call_id:'verified',content:'OK (exit_code: 0)\npassed'}];
  assert.equal(context.restoreKnownVerificationCommandsFromMessages(history,contract)[0], 'node --test note.test.mjs');
  history[1].content='ERROR (exit_code: 1)';
  assert.equal(context.restoreKnownVerificationCommandsFromMessages(history,contract).length, 0);
});

test('file approvals bind the native edit and reject a different path or unexecuted response', () => {
  for (const action of ['write_file', 'patch_file', 'apply_diff']) {
    const {api, storage} = setup();
    const work = api.open(storage, 'edit-thread', 'edit source', 8, true);
    api.snapshot(storage, 'edit-thread', work, [
      {role:'assistant',tool_calls:[{id:'edit-A',function:{name:action,arguments:'{"path":"src/a.rs"}'}}]},
      {role:'tool',tool_call_id:'edit-A',content:'Awaiting approval via /api/approve_edit\napproval_id: pe_1\napproval_path: workspace/src/a.rs'},
    ]);
    const item={id:'pe_1',action,path:'workspace/src/a.rs',result:{ok:true,message:"OK: patched 'workspace/src/a.rs'"}};
    assert.equal(api.resolveApproval(storage, 'edit-thread', 'pe_1', {...item,path:'elsewhere.rs'}, true), false);
    assert.equal(api.resolveApproval(storage, 'edit-thread', 'pe_1', {...item,result:null}, true), false);
    assert.equal(api.resolveApproval(storage, 'edit-thread', 'pe_1', item, true), true);
    assert.equal(work.messages[1].content, item.result.message);
    assert.equal(api.resolveApproval(storage, 'edit-thread', 'pe_1', item, true), false);
  }
});
