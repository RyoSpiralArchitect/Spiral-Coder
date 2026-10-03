import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { test } from 'node:test';
import { buildScenarioResult, collectBrowserDiagnostics } from '../../scripts/ui-smoke-result.mjs';

function scenario() {
  const page = new EventEmitter();
  const diagnostics = collectBrowserDiagnostics(page);
  return {
    page,
    finish(result = { ok: true, checks: { rendered: true } }) {
      return buildScenarioResult({
        name: 'baseline',
        workspaceRoot: 'fixture',
        result,
        diagnostics,
      });
    },
  };
}

test('successful scenario requires no browser errors; warnings and successful HTTP responses are allowed', () => {
  const { page, finish } = scenario();
  for (const type of ['log', 'info', 'warning']) {
    page.emit('console', { type: () => type, text: () => 'diagnostic' });
  }
  for (const status of [200, 204, 304, 399]) {
    page.emit('response', { status: () => status, url: () => '/api/status' });
  }
  assert.deepEqual(finish(), {
    ok: true,
    checks: { rendered: true },
    name: 'baseline',
    workspaceRoot: 'fixture',
    pageErrors: [],
    consoleErrors: [],
    badResponses: [],
  });
});

test('page exception fails a scenario even when all scenario checks passed', () => {
  const { page, finish } = scenario();
  page.emit('pageerror', new Error('render crashed'));
  const report = finish();
  assert.equal(report.ok, false);
  assert.deepEqual(report.pageErrors, ['render crashed']);
  assert.deepEqual(report.checks, { rendered: true });
});

test('console error fails a scenario even without a page exception or failed HTTP response', () => {
  const { page, finish } = scenario();
  page.emit('console', { type: () => 'error', text: () => 'state update failed' });
  const report = finish();
  assert.equal(report.ok, false);
  assert.deepEqual(report.consoleErrors, ['state update failed']);
  assert.deepEqual(report.pageErrors, []);
  assert.deepEqual(report.badResponses, []);
});

for (const status of [400, 404, 500, 503]) {
  test(`HTTP ${status} fails a scenario even when its UI assertions passed`, () => {
    const { page, finish } = scenario();
    page.emit('response', { status: () => status, url: () => '/api/status' });
    const report = finish();
    assert.equal(report.ok, false);
    assert.deepEqual(report.badResponses, [{ url: '/api/status', status }]);
  });
}

test('failed or missing scenario assertion cannot pass just because the browser was error-free', () => {
  const { finish } = scenario();
  for (const result of [{ ok: false }, {}, { ok: 'true' }]) {
    assert.equal(finish(result).ok, false);
  }
});

test('scenario data cannot replace authoritative browser diagnostics or identity', () => {
  const { page, finish } = scenario();
  page.emit('pageerror', new Error('render crashed'));
  page.emit('console', { type: () => 'error', text: () => 'state update failed' });
  page.emit('response', { status: () => 500, url: () => '/api/status' });
  assert.deepEqual(finish({
    ok: true,
    name: 'other scenario',
    workspaceRoot: 'other root',
    pageErrors: [],
    consoleErrors: [],
    badResponses: [],
  }), {
    ok: false,
    name: 'baseline',
    workspaceRoot: 'fixture',
    pageErrors: ['render crashed'],
    consoleErrors: ['state update failed'],
    badResponses: [{ url: '/api/status', status: 500 }],
  });
});
