import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import vm from 'node:vm';

function bootstrap(overrides = {}) {
  let rendered = null;
  const context = {
    window: {},
    document: { getElementById: id => id === 'app-root' ? {} : null },
    React: { createElement: (type, props, ...children) => ({ type, props, children }) },
    ReactDOM: { createRoot: () => ({ render: tree => { rendered = tree; } }) },
    ...overrides,
  };
  const html = readFileSync(new URL('../../web/index.html', import.meta.url), 'utf8');
  const assets = [...html.matchAll(/<script src="\/assets\/([^"?]+)(?:\?[^" ]*)?"/g)].map(match => match[1]);
  for (const asset of assets) {
    if (asset.startsWith('vendor/') || asset === 'governor_contract.js') continue;
    const source = readFileSync(new URL(`../../web/${asset}`, import.meta.url), 'utf8');
    vm.runInNewContext(source, context, { filename: asset });
  }
  return { context, rendered };
}

test('indexed runtime helpers and app bootstrap share valid JavaScript namespaces', () => {
  const { context, rendered } = bootstrap();
  assert.equal(typeof context.window.SpiralCoder.sandbox.resolvedCwd, 'function');
  assert.equal(typeof context.window.SpiralCoder.exec.stripShellTranscript, 'function');
  assert.equal(typeof context.window.SpiralCoder.observer.similarity, 'function');
  assert.equal(typeof context.window.SpiralCoderState.rootUserTextForRun, 'function');
  assert.equal(typeof rendered?.type, 'function', 'app should reach its React render entry point');
});

test('optional highlighting downloads do not block local application scripts', () => {
  const html = readFileSync(new URL('../../web/index.html', import.meta.url), 'utf8');
  const externalScripts = html.match(/<script\b[^>]*src="https?:[^>]*>/g) || [];
  assert.ok(externalScripts.length > 0);
  for (const script of externalScripts) assert.match(script, /\basync\b/);
  const externalStyles = html.match(/<link\b[^>]*href="https?:[^>]*>/g) || [];
  for (const stylesheet of externalStyles) {
    assert.match(stylesheet, /media="print"/);
    assert.match(stylesheet, /onload="this\.media='all'"/);
  }
});

test('initial app state reads legacy thread and preference values before persistence effects', () => {
  const legacyThread = [{ id: 'existing-thread', title: 'Saved work', messages: [] }];
  const db = new Map([
    ['obstral.lang.v1', 'en'],
    ['obstral.config.v1', JSON.stringify({ toolRoot: 'existing-work', autoObserve: false })],
    ['obstral.threads.v1', JSON.stringify(legacyThread)],
    ['obstral.active.v1', 'existing-thread'],
    ['obstral.splitPct.v2', '55'],
  ]);
  const initialValues = [];
  const effects = [];
  const { rendered } = bootstrap({
    localStorage: { getItem: key => db.get(key) ?? null, setItem: (key, value) => db.set(key, value) },
    React: {
      createElement: (type, props, ...children) => ({ type, props, children }),
      useState(value) { const initial = typeof value === 'function' ? value() : value; initialValues.push(initial); return [initial, () => {}]; },
      useRef: value => ({ current: value }),
      useMemo: fn => fn(),
      useCallback: fn => fn,
      useEffect: fn => effects.push(fn),
    },
  });
  rendered.type();
  assert.equal(initialValues[0], 'en');
  assert.ok(initialValues.some(value => value?.toolRoot === 'existing-work'));
  const threads = initialValues.find(value => value?.activeId === 'existing-thread');
  assert.equal(threads.threads[0].title, 'Saved work');
  assert.ok(initialValues.includes(55));
  assert.equal(db.get('spiral-coder.threads.v1'), db.get('obstral.threads.v1'));
  assert.ok(effects.length > 0, 'persistence effects are registered after initial migration reads');
});
