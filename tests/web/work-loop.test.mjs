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

test('another tab replacing a task invalidates cached reads and every stale writer', () => {
  const first = setup();
  const second = setup(first.entries);
  const old = first.api.open(first.storage, 'thread', 'old', 4, true);
  const receipt = first.api.reserve(first.storage, 'thread', old, 'coder', []);
  const newer = second.api.open(second.storage, 'thread', 'new', 6, true);
  const saved = first.entries.get('spiral-coder.work.v1.thread');
  assert.equal(first.api.persist(first.storage, 'thread', old), false);
  assert.equal(first.api.isCurrent(first.storage, 'thread', old.taskId, old.revision), false);
  assert.equal(first.api.isActive(first.storage, 'thread', old), false);
  assert.equal(first.api.snapshot(first.storage, 'thread', old, [{ role: 'assistant', content: 'stale' }], true), false);
  assert.equal(first.api.finish(first.storage, 'thread', old, receipt, { tokens: 123 }, 'completed'), false);
  assert.equal(first.api.claimAdvice(first.storage, 'thread', old, 'stale-assist'), false);
  assert.throws(() => first.api.reserve(first.storage, 'thread', old, 'observer', []), /replaced/);
  assert.equal(first.entries.get('spiral-coder.work.v1.thread'), saved);
  assert.equal(first.api.read(first.storage, 'thread').taskId, newer.taskId);
});

test('same-task writes in another tab invalidate stale handles without losing receipts', () => {
  const first = setup();
  const old = first.api.open(first.storage, 'thread', 'same', 4, true);
  const second = setup(first.entries);
  const continued = second.api.open(second.storage, 'thread', 'same', 4, false);
  second.api.reserve(second.storage, 'thread', continued, 'observer', []);
  assert.equal(first.api.persist(first.storage, 'thread', old), false);
  assert.throws(() => first.api.reserve(first.storage, 'thread', old, 'coder', []), /updated/);
  const latest = first.api.read(first.storage, 'thread');
  assert.equal(latest.requests.length, 1);
  assert.equal(latest.requests[0].role, 'observer');
  assert.equal(latest.persistenceRevision, continued.persistenceRevision);
  assert.notEqual(latest, old);
  first.api.reserve(first.storage, 'thread', latest, 'coder', []);
  assert.equal(first.api.read(first.storage, 'thread'), latest, 'same-tab reads preserve the active handle');
});

test('a replaced task cannot start a tool request or consume a late async result', async () => {
  const first = setup();
  const old = first.api.open(first.storage, 'thread', 'old', 4, true);
  let resolve, calls = 0;
  const pending = first.api.runCurrent(first.storage, 'thread', old, () => {
    calls++; return new Promise(r => { resolve = r; });
  });
  const second = setup(first.entries);
  second.api.open(second.storage, 'thread', 'new', 4, true);
  resolve({ tool_calls: ['stale-edit'] });
  await assert.rejects(pending, /replaced/);
  await assert.rejects(first.api.runCurrent(first.storage, 'thread', old, () => { calls++; }), /replaced/);
  assert.equal(calls, 1);
});

test('missing or unavailable durable state cannot keep an old task current', () => {
  for (const removed of ['missing', 'invalid', 'unavailable']) {
    const { api, storage, entries } = setup();
    const work = api.open(storage, 'thread', 'task', 4, true);
    if (removed === 'missing') entries.clear();
    if (removed === 'invalid') entries.set('spiral-coder.work.v1.thread', '{invalid');
    if (removed === 'unavailable') storage.getItem = () => { throw new Error('unavailable'); };
    assert.equal(api.isCurrent(storage, 'thread', work.taskId, work.revision), false);
    assert.throws(() => api.reserve(storage, 'thread', work, 'coder', []), /replaced/);
  }
});

test('Lite supports meta diagnosis but refuses next-action before creating a request', async () => {
  const app = readFileSync(new URL('../../web/app.js', import.meta.url), 'utf8');
  const pick = (name, end) => app.slice(app.indexOf('    const ' + name + ' ='), app.indexOf('    const ' + end + ' ='));
  const meta = pick('runMetaDiagnoseForPacket', 'runMetaDiagnose');
  // Use the function's actual async body until its first state change. Reaching
  // apiKeyForPane proves the capability gate did not reject ordinary diagnosis.
  const prefix = meta.slice(0, meta.indexOf('      const obsKey =')) + '      return "diagnosis-enabled"; };';
  const context = { activeThread: { id: 'thread' }, sendingObserver: false, metaBusy: false,
    status: { features: { chat: true } }, config: {}, showToast: () => { throw new Error('wrong gate'); } };
  vm.createContext(context);
  vm.runInContext(prefix + '\nthis.meta = runMetaDiagnoseForPacket;', context);
  assert.equal(await context.meta({ failure_kind: 'test' }), 'diagnosis-enabled');
  const next = pick('runObserverNextActionAssistForPacket', 'runObserverNextActionAssist');
  const nextPrefix = next.slice(0, next.indexOf('      const threadId =')) + '      throw new Error("unsupported request started"); };';
  let warning = '';
  context.showToast = text => { warning = text; };
  vm.runInContext(nextPrefix + '\nthis.next = runObserverNextActionAssistForPacket;', context);
  await context.next({ failure_kind: 'test' });
  assert.match(warning, /Meta diagnosis is still available/);
});

test('meta diagnosis can compute its save digest outside the Coder execution closure', () => {
  const app = readFileSync(new URL('../../web/app.js', import.meta.url), 'utf8');
  const start = app.indexOf('    const metaConfigDigest =');
  const end = app.indexOf('    const saveMetaDiagnoseArtifact =', start);
  const context = { window: {}, config: { observerMode: 'Observer', requireEditApproval: true } };
  vm.createContext(context);
  vm.runInContext(readFileSync(new URL('../../web/core/state.js', import.meta.url), 'utf8'), context);
  vm.runInContext('const { fnv1a64 } = window.SpiralCoderState;\n' + app.slice(start, end) + '\nthis.digest = metaConfigDigest;', context);
  assert.equal(context.window.SpiralCoderState.fnv1a64('hello').toString(16), 'a430d84680aabd0b');
  const digest = context.digest({ thread_id: 'task', coder_mode: 'VIBE' }, 'test-provider', 'small-model');
  assert.match(digest, /^[0-9a-f]{16}$/);
  assert.equal(context.digest({ thread_id: 'task', coder_mode: 'VIBE' }, 'test-provider', 'small-model'), digest);
  assert.notEqual(context.digest({ thread_id: 'task', coder_mode: 'VIBE' }, 'test-provider', 'different-model'), digest);
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
