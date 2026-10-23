import { parseHTML } from 'linkedom';
import { SourceTextModule, createContext } from 'node:vm';
import { readFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

export const appPath = resolve(dirname(fileURLToPath(import.meta.url)), '../../src/web/app');
export const settle = () => new Promise(resolve => setTimeout(resolve, 10));

/** DOM contract tests, not a layout engine or native accessibility implementation. */
export function harness({ hash = '#/duplicados', compact = false, reduced = false, lang = 'en', fetch: handler } = {}) {
  const { window } = parseHTML('<!doctype html><html><body><main id="root"></main></body></html>');
  const { document } = window;
  const selectValue = Object.getOwnPropertyDescriptor(window.HTMLSelectElement.prototype, 'value');
  Object.defineProperty(window.HTMLSelectElement.prototype, 'value', {
    configurable: true,
    get() { return selectValue.get.call(this); },
    set(value) {
      for (const option of this.options) option.selected = option.value === String(value);
    },
  });
  let active = document.body;
  Object.defineProperty(document, 'activeElement', { get: () => active });
  window.HTMLElement.prototype.focus = function () { active = this; };
  window.HTMLElement.prototype.showModal = function () { this.open = true; this.focus(); };
  window.HTMLElement.prototype.close = function () { this.open = false; };
  const location = { hash, reload() {} }, storage = new Map([['mct-lang', lang]]), frames = new Map();
  let frameId = 0;
  const media = new Map();
  const matchMedia = query => {
    if (!media.has(query)) {
      const listeners = new Set();
      media.set(query, { matches: query.includes('reduced-motion') ? reduced : compact,
        addEventListener: (_, fn) => listeners.add(fn), removeEventListener: (_, fn) => listeners.delete(fn),
        change(value) { this.matches = value; listeners.forEach(fn => fn()); } });
    }
    return media.get(query);
  };
  const context = createContext({ document, window: { innerWidth: compact ? 375 : 1440 },
    Element: window.Element, Node: window.Node, Event: window.Event, console,
    URL, URLSearchParams, Response, AbortController, DOMException, Intl, Date, Math, performance,
    location, history: { replaceState: (_, __, value) => { location.hash = value; } },
    localStorage: { getItem: key => storage.get(key) ?? null, setItem: (key, value) => storage.set(key, value) },
    navigator: { clipboard: { writeText: async () => {} } }, matchMedia,
    setTimeout, clearTimeout, requestAnimationFrame: fn => { frames.set(++frameId, fn); return frameId; },
    cancelAnimationFrame: id => frames.delete(id),
    fetch: async (path, options = {}) => {
      const url = new URL(path, 'http://mct.test/');
      const body = await handler?.(url, options);
      if (options.signal?.aborted) throw new DOMException('Aborted', 'AbortError');
      return body instanceof Response ? body : new Response(JSON.stringify(body ?? {}));
    } });
  const modules = new Map();
  function moduleAt(path) {
    const sourcePath = path.endsWith('/graph.js') && process.env.MCT_GRAPH_SOURCE ? process.env.MCT_GRAPH_SOURCE : path;
    if (!modules.has(path)) modules.set(path, new SourceTextModule(readFileSync(sourcePath, 'utf8'), { context, identifier: path }));
    return modules.get(path);
  }
  return { document, window, location, frames, media, context,
    async load(name) {
      const module = moduleAt(resolve(appPath, name));
      if (module.status === 'unlinked') await module.link((specifier, importer) => moduleAt(resolve(dirname(importer.identifier), specifier)));
      if (module.status === 'linked') await module.evaluate();
      return module.namespace;
    },
    root: document.getElementById('root'),
    event: (type, fields = {}) => Object.assign(new window.Event(type, { bubbles: true, cancelable: true }), fields) };
}

export const finding = {
  key: 'duplicate-test', type: 'duplicate', title: 'alpha ≈ beta', state: 'unreviewed', revision: 'none',
  language: 'cpp', verdict: 'real', priority: 'normal', scope: 'cross-file', lines: 3, tokens: 8,
  reasons: [], history: [], method: {}, members: [
    { name: 'alpha', path: 'src/same.cpp', start: 1, end: 3, excerpt: ['int alpha() {', ' return 1;', '}'] },
    { name: 'beta', path: 'src/same.cpp', start: 5, end: 7, excerpt: ['int beta() {', ' return 1;', '}'] },
    { name: 'gamma', path: 'other.cpp', start: 9, end: 11, excerpt: ['int gamma() {', ' return 1;', '}'] },
  ], memberCount: 3,
};
export const symbol = { id: 1, name: 'alpha', path: 'same.cpp', line: 1, endLine: 3, kind: 'function', language: 'cpp', refs: 0, duplicateGroups: 0 };
export const symbolDetail = { symbol, references: [], related: [], duplicates: [], dead: null, notes: [] };
export const list = (items, facets = {}) => ({ items, total: items.length, all: items.length, size: 25, page: 0, facets });
export function defaultApi(url) {
  const route = url.pathname.split('/').at(-1);
  if (route === 'findings') return list([finding]);
  if (route === 'finding') return finding;
  if (route === 'source') return { lines: [`line ${url.searchParams.get('start')}`] };
  if (route === 'files') return { files: [['same.cpp', 'cpp', 1]] };
  if (route === 'symbols') return list([symbol], { kind: [['function', 1]], language: [['cpp', 1]], ext: [['cpp', 1]] });
  if (route === 'symbol') return symbolDetail;
  if (route === 'notes') return list([], { folder: [['notes', 1]], tag: [] });
  if (route === 'notes-graph') return { nodes: [], edges: [], total: 0 };
  return {};
}
