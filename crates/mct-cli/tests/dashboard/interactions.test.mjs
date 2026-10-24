import test from 'node:test';
import assert from 'node:assert/strict';
import { harness, settle, finding, defaultApi } from './harness.mjs';

const findButton = (root, text) => [...root.querySelectorAll('button')].find(el => el.textContent.trim() === text);
async function findings(fetch = defaultApi, query = 'key=duplicate-test&view=side') {
  const env = harness({ hash: `#/duplicados?${query}`, fetch });
  const { render } = await env.load('page-findings.js');
  env.cleanup = await render({ root: env.root, page: 'duplicados', params: new URLSearchParams(query) });
  await settle();
  return env;
}

test('same-file comparison completes independent source requests and selects arbitrary members', async () => {
  const requests = [];
  const env = await findings(async (url, options) => { if (url.pathname.endsWith('/source')) { requests.push({ url, signal: options.signal }); await settle(); } return defaultApi(url); });
  await settle();
  assert.equal(requests.length, 2);
  assert.equal(requests[0].url.searchParams.get('path'), requests[1].url.searchParams.get('path'));
  assert.notEqual(requests[0].signal, requests[1].signal);
  assert.ok(requests.every(r => !r.signal.aborted));
  assert.equal(env.root.querySelectorAll('.compare .pane').length, 2);
  const right = env.root.querySelector('select[aria-label="Right fragment"]');
  right.value = '2'; right.dispatchEvent(env.event('change'));
  await settle(); await settle();
  assert.equal(new URLSearchParams(env.location.hash.split('?')[1]).get('right'), '2');
  assert.match(env.root.querySelector('.compare').textContent, /other.cpp/);
  env.cleanup();
});

test('failure retains Back, pair and retry; retry fetches the same selected fragments', async () => {
  let fail = true;
  const starts = [];
  const env = await findings((url) => {
    if (url.pathname.endsWith('/source')) { starts.push(url.searchParams.get('start')); if (fail) return new Response('{"error":"fixture failure"}', { status: 500 }); }
    return defaultApi(url);
  }, 'key=duplicate-test&view=side&left=1&right=2');
  assert.ok(findButton(env.root, 'Back to the list'));
  assert.ok(findButton(env.root, 'Retry'));
  assert.equal(env.root.querySelector('select[aria-label="Left fragment"]').value, '1');
  fail = false; findButton(env.root, 'Retry').click(); await settle();
  assert.deepEqual(starts, ['5', '9', '5', '9']);
  assert.equal(env.root.querySelectorAll('.compare .pane').length, 2);
  env.cleanup();
});

test('pair switches and teardown cancel obsolete comparisons without an AbortError screen', async () => {
  const pending = [];
  const env = await findings((url, { signal }) => {
    if (!url.pathname.endsWith('/source')) return defaultApi(url);
    return new Promise((resolve, reject) => { pending.push({ signal, resolve: () => resolve(defaultApi(url)) }); signal.addEventListener('abort', () => reject(new DOMException('Aborted', 'AbortError'))); });
  });
  const core = await env.load('core.js');
  core.patchQuery({ right: '2' }); await settle();
  assert.equal(pending.length, 4);
  assert.ok(pending.slice(0, 2).every(r => r.signal.aborted));
  pending.slice(2).forEach(r => r.resolve()); await settle();
  assert.doesNotMatch(env.root.textContent, /Aborted|operation was aborted/);
  assert.match(env.root.querySelector('.compare').textContent, /other.cpp/);
  core.patchQuery({ right: '1' }); await settle();
  env.cleanup();
  assert.ok(pending.slice(4).every(r => r.signal.aborted));
});

test('API cleanup cannot erase a newer operation; explicit cancellation remains effective through JSON decoding', async () => {
  const signals = [], releases = [];
  const env = harness({ fetch: (_, { signal }) => { signals.push(signal); return new Promise(resolve => releases.push(resolve)); } });
  const core = await env.load('core.js');
  const first = core.api('source', {}, 'owner').catch(e => e.name);
  const second = core.api('source', {}, 'owner').catch(e => e.name);
  releases[0]({}); await first;
  core.cancelApi('owner');
  assert.ok(signals.every(s => s.aborted));
  releases[1]({}); assert.equal(await second, 'AbortError');
});

test('Clear and chip removal synchronize URL/input/results and cancel buffered typing', async () => {
  const queries = [];
  const env = await findings(url => { if (url.pathname.endsWith('/findings')) queries.push(url.searchParams.get('q') || ''); return defaultApi(url); }, 'q=alpha');
  const input = env.root.querySelector('input[type=search]');
  input.value = 'buffered'; input.dispatchEvent(env.event('input'));
  findButton(env.root, 'Clear').click();
  await new Promise(resolve => setTimeout(resolve, 300));
  assert.equal(input.value, '');
  assert.ok(!env.location.hash.includes('q='));
  assert.deepEqual(queries, ['alpha', '']);
  env.cleanup();
});

test('mobile symbol dialog owns the inspector, dismisses with Escape and restores the originating row', async () => {
  const env = harness({ hash: '#/codigo', compact: true, fetch: defaultApi });
  const { render } = await env.load('page-symbols.js');
  const cleanup = await render({ root: env.root, params: new URLSearchParams() });
  const row = env.root.querySelector('tbody tr'); row.focus();
  row.dispatchEvent(env.event('keydown', { key: 'Enter' })); await settle();
  const dialog = env.document.querySelector('dialog');
  assert.equal(dialog.open, true);
  assert.equal(dialog.getAttribute('aria-label'), 'Symbol details');
  assert.ok(dialog.contains(env.document.activeElement));
  assert.ok(dialog.querySelector('.inspector-col'));
  dialog.dispatchEvent(env.event('cancel'));
  assert.equal(dialog.open, false);
  assert.equal(env.document.activeElement, row);
  assert.ok(!env.location.hash.includes('sel='));
  row.click(); await settle();
  dialog.querySelector('[aria-label="Close"]').click();
  assert.equal(dialog.open, false);
  row.focus(); row.click(); await settle();
  env.media.get('(max-width: 1280px)').change(false);
  assert.equal(dialog.open, false);
  assert.ok(env.root.querySelector('.inspector-col'));
  cleanup(); assert.equal(env.document.querySelector('dialog'), null);
});

test('Show more reaches all 111 backlinks without replacing the focused control', async () => {
  const env = harness(); const { h } = await env.load('core.js'); const { pagedLinks } = await env.load('components.js');
  const box = pagedLinks(Array.from({ length: 111 }, (_, id) => id), id => h('a', { href: `#${id}` }, `Note ${id}`)); env.root.append(box);
  const more = box.querySelector('button'); more.focus();
  assert.equal(box.querySelectorAll('a').length, 30);
  for (const count of [60, 90, 111]) { more.click(); assert.equal(box.querySelectorAll('a').length, count); assert.equal(env.document.activeElement, more); }
  assert.equal(more.disabled, true);
});

test('reduced-motion graph layout yields work to animation frames instead of blocking', async () => {
  const env = harness({ reduced: true });
  const { graphView } = await env.load('graph.js');
  const nodes = Array.from({ length: 200 }, (_, id) => ({ id: String(id), label: `node-${id}` }));
  const graph = graphView({ nodes, edges: [] });
  assert.equal(env.frames.size, 1);
  const [id, frame] = env.frames.entries().next().value;
  env.frames.delete(id); frame();
  assert.ok(env.frames.size <= 1);
  graph.destroy();
  assert.equal(env.frames.size, 0);
});

test('Obsidian navigation reflects Graph after Backlinks even with cached facets', async () => {
  const env = harness({ hash: '#/obsidian?view=backlinks', fetch: defaultApi });
  const { render } = await env.load('page-notes.js'); const cleanup = await render({ root: env.root, params: new URLSearchParams('view=backlinks') });
  const core = await env.load('core.js'); core.patchQuery({ view: 'grafo' }); await settle();
  assert.equal(env.root.querySelector('nav[aria-label="Navigation"] [aria-current="page"]')?.textContent, 'Graph');
  cleanup();
});

test('obsolete Obsidian note requests cannot replace the newly selected detail', async () => {
  const env = harness({ hash: '#/obsidian', fetch: (url, { signal }) => {
    if (url.pathname.endsWith('/notes')) return { items: [], total: 0, all: 0, page: 0, size: 40, facets: { folder: [], tag: [] } };
    if (url.pathname.endsWith('/note') && url.searchParams.get('path') === 'old.md') {
      return new Promise((_, reject) => signal.addEventListener('abort', () => reject(new DOMException('Aborted', 'AbortError'))));
    }
    if (url.pathname.endsWith('/note')) return { title: 'New note', path: 'new.md', linksTo: [], backlinkList: [], tags: [], frontmatter: [], body: 'fresh content', codePath: null, symbols: [], duplicateKeys: [], deadCodeKeys: [] };
    return defaultApi(url);
  } });
  const { render } = await env.load('page-notes.js');
  const cleanup = await render({ root: env.root, params: new URLSearchParams() });
  const core = await env.load('core.js');
  core.patchQuery({ note: 'old.md' }); await settle();
  core.patchQuery({ note: 'new.md' }); await settle();
  assert.match(env.root.querySelector('.inspector-col').textContent, /New note/);
  assert.match(env.root.querySelector('.inspector-col').textContent, /fresh content/);
  assert.doesNotMatch(env.root.textContent, /Aborted/);
  cleanup();
});

test('number formatting follows the selected locale including statistics decimals', async () => {
  for (const [lang, expected] of [['en', '1,234.5'], ['es', '1234,5']]) {
    const env = harness({ lang }); const { fmt } = await env.load('core.js'); assert.equal(fmt(1234.5, { maximumFractionDigits: 1 }), expected);
  }
});

test('recent activity links carry the finding identity and include findings outside default filters', async () => {
  const env = harness({ hash: '#/panel' });
  const { render } = await env.load('page-dashboard.js');
  render({ root: env.root,
    summary: { symbols: 0, files: 0, duplicates: { groups: 1, complete: true, skipped: 0 }, deadCode: { candidates: 0 }, findings: { total: 1, states: { confirmed: 1 } }, notes: 0, languages: [], reviewActivity: [], recent: [{ to: 'confirmed', at: null, finding: { key: 'duplicate:key', type: 'duplicate', title: 'recent finding' } }], health: { readFailures: 0, syntaxErrors: 0 } },
    meta: { project: 'fixture', indexing: { state: 'ready', lastIndexedAt: null } },
  });
  const link = [...env.root.querySelectorAll('a')].find(item => item.textContent === 'recent finding');
  assert.ok(link);
  const params = new URLSearchParams(link.getAttribute('href').split('?')[1]);
  assert.equal(params.get('key'), 'duplicate:key');
  assert.equal(params.get('state'), 'all');
});

// Dead code gets the comparison's layout: the list gives way to one pane with the
// candidate's lines marked and a few context lines around them.
const deadFinding = {
  key: 'dead-test', type: 'dead-code', title: 'unused_helper', kind: 'function', language: 'rust', state: 'unreviewed', revision: 'none',
  confidence: 'high', zone: 'source', visibility: 'private', path: 'src/lib.rs', line: 10, history: [],
  detail: { symbol: { path: 'src/lib.rs', line: 10, endLine: 12, language: 'rust' }, code: null, imports: [], importedBy: [], notes: [] },
};
function deadApi(requests) {
  return async (url) => {
    const route = url.pathname.split('/').at(-1);
    if (route === 'findings') return { items: [deadFinding], total: 1, all: 1, size: 25, page: 0, facets: {} };
    if (route === 'finding') return deadFinding;
    if (route === 'source') {
      requests.push(url);
      const start = Number(url.searchParams.get('start')), end = Number(url.searchParams.get('end'));
      return { start, lines: Array.from({ length: end - start + 1 }, (_, n) => `line ${start + n}`), truncated: false, language: 'rust' };
    }
    return defaultApi(url);
  };
}

test('a dead-code candidate opens in the code view with its own lines marked', async () => {
  const requests = [];
  const query = 'key=dead-test&view=code';
  const env = harness({ hash: `#/codigo-muerto?${query}`, fetch: deadApi(requests) });
  const { render } = await env.load('page-findings.js');
  const cleanup = await render({ root: env.root, page: 'codigo-muerto', params: new URLSearchParams(query) });
  await settle(); await settle();
  assert.equal(requests.length, 1);
  assert.equal(requests[0].searchParams.get('start'), '7', 'three context lines before the symbol');
  assert.equal(requests[0].searchParams.get('end'), '15');
  assert.equal(env.root.querySelectorAll('.compare.single .pane').length, 1);
  assert.equal(env.root.querySelector('.workbench').classList.contains('compare-mode'), true);
  const marked = [...env.root.querySelectorAll('.compare .ln.del')].map(row => row.firstChild.textContent);
  assert.deepEqual(marked, ['10', '11', '12']);
  assert.match(env.root.textContent, /Lines with no known references \(3\)/);
  const context = env.root.querySelector('select[aria-label="Surrounding context"]');
  context.value = '10'; context.dispatchEvent(env.event('change'));
  await settle(); await settle();
  assert.equal(new URLSearchParams(env.location.hash.split('?')[1]).get('context'), '10');
  assert.equal(requests.at(-1).searchParams.get('start'), '1', 'context never starts before line 1');
  assert.equal(requests.at(-1).searchParams.get('end'), '22');
  assert.equal(env.root.querySelectorAll('.compare .ln').length, 22);
  assert.equal(env.root.querySelectorAll('.compare .ln.del').length, 3);
  findButton(env.root, 'Back to the list').dispatchEvent(env.event('click'));
  await settle();
  assert.equal(new URLSearchParams(env.location.hash.split('?')[1]).get('view'), null);
  assert.equal(env.root.querySelector('.compare'), null);
  cleanup();
});

test('the dead-code list row opens the code view instead of a dialog', async () => {
  const requests = [];
  const env = harness({ hash: '#/codigo-muerto', fetch: deadApi(requests) });
  const { render } = await env.load('page-findings.js');
  const cleanup = await render({ root: env.root, page: 'codigo-muerto', params: new URLSearchParams() });
  await settle();
  const view = [...env.root.querySelectorAll('button')].find(el => el.getAttribute('aria-label') === 'View code of: unused_helper');
  view.dispatchEvent(env.event('click'));
  await settle(); await settle();
  const params = new URLSearchParams(env.location.hash.split('?')[1]);
  assert.equal(params.get('key'), 'dead-test');
  assert.equal(params.get('view'), 'code');
  assert.equal(env.root.querySelector('dialog'), null);
  assert.equal(env.root.querySelectorAll('.compare.single .pane').length, 1);
  cleanup();
});
