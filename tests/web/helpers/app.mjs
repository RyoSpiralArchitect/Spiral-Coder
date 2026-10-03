import { readFileSync } from 'node:fs';
import vm from 'node:vm';

// Exercise the real App's rendered controls and callbacks without a browser or
// providers. DOM/focus behavior is separately checked in rendered UI QA.
export function mountApp(config = {}, options = {}) {
  const values = [];
  let cursor = 0;
  let component;
  let tree;
  let effects = [];
  const requests = [];
  const entries = new Map([
    ['spiral-coder.lang.v1', 'en'],
    ['spiral-coder.config.v1', JSON.stringify({
      stream: false, forceAgent: false, autoObserve: false, chatAutoTasks: false, ...config,
    })],
  ]);
  function state(initial) {
    const index = cursor++;
    if (!(index in values)) values[index] = typeof initial === 'function' ? initial() : initial;
    return [values[index], next => { values[index] = typeof next === 'function' ? next(values[index]) : next; }];
  }
  const context = {
    window: {},
    document: { getElementById: () => ({}) },
    localStorage: { getItem: key => entries.get(key) ?? null, setItem: (key, value) => entries.set(key, value) },
    React: {
      createElement: (type, props, ...children) => ({ type, props: props || {}, children: children.flat(Infinity) }),
      useState: state,
      useRef: initial => state(() => ({ current: initial }))[0],
      useMemo: fn => fn(),
      useCallback: fn => fn,
      useEffect: (run, deps) => effects.push({ run, deps }),
    },
    ReactDOM: { createRoot: () => ({ render: node => { component = node.type; } }) },
    requestAnimationFrame: fn => fn(),
    setTimeout: () => 0,
    clearTimeout: () => {},
    AbortController,
    fetch: async (url, options = {}) => {
      requests.push({ url, body: options.body ? JSON.parse(options.body) : null });
      return { ok: true, headers: { get: () => 'application/json' }, json: async () => ({ content: '[Observer] QA response' }) };
    },
  };
  if (options.fetch) {
    context.fetch = async (url, requestOptions = {}) => {
      requests.push({ url, body: requestOptions.body ? JSON.parse(requestOptions.body) : null });
      return options.fetch(url, requestOptions);
    };
  }
  const html = readFileSync(new URL('../../../web/index.html', import.meta.url), 'utf8');
  const assets = [...html.matchAll(/<script src="\/assets\/([^"?]+)(?:\?[^" ]*)?"/g)].map(match => match[1]);
  for (const asset of assets) {
    if (asset.startsWith('vendor/') || asset === 'governor_contract.js') continue;
    vm.runInNewContext(readFileSync(new URL(`../../../web/${asset}`, import.meta.url), 'utf8'), context, { filename: asset });
  }
  function render() {
    cursor = 0;
    effects = [];
    tree = component();
    return tree;
  }
  function all(predicate, node = tree) {
    if (!node || typeof node !== 'object') return [];
    return [...(predicate(node) ? [node] : []), ...(node.children || []).flatMap(child => all(predicate, child))];
  }
  render();
  return {
    render, all, requests,
    threadState: () => values.find(value => value?.threads && value?.activeId),
    config: () => values.find(value => value?.provider && value?.toolRoot),
    effects: () => effects,
    storage: entries,
  };
}

export function textOf(node) {
  if (typeof node === 'string' || typeof node === 'number') return String(node);
  return (node?.children || []).map(textOf).join('');
}
