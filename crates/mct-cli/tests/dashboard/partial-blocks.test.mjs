import test from 'node:test';
import assert from 'node:assert/strict';
import { harness, settle, defaultApi } from './harness.mjs';

const block = (id, extra = {}) => ({
  id, language: 'rust', tokens: 68, lines: 11,
  occurrences: [
    { path: 'src/blocks.rs', start: 12, end: 22, function: 'summarize_orders' },
    { path: 'src/blocks.rs', start: 28, end: 38, function: 'summarize_refunds' },
  ],
  ...extra,
});
const done = (items, extra = {}) => ({
  status: 'done', total: items.length, page: 0, size: 25, items, normalization: 'broad',
  budget: { minTokens: 40, minLines: 6, maxBlocks: 200, functionsSearched: 6, exhausted: null },
  indexCoverage: 'no-recorded-errors', indexSyntaxErrors: 0, indexReadFailures: 0, skipped: 0, elapsedMs: 12,
  ...extra,
});

// `states` answers the GETs in order (the last one repeats); `started` answers the POST.
async function duplicados({ states = [{ status: 'idle' }], started = { status: 'running' }, page = 'duplicados', query = '' } = {}) {
  const calls = [];
  const env = harness({
    hash: `#/${page}?${query}`,
    fetch: (url, options) => {
      if (!url.pathname.endsWith('/partial-blocks')) return defaultApi(url);
      calls.push({ method: options.method || 'GET', page: url.searchParams.get('page'), body: options.body });
      if (options.method === 'POST') return started;
      return states.length > 1 ? states.shift() : states[0];
    },
  });
  const { render } = await env.load('page-findings.js');
  env.cleanup = await render({ root: env.root, page, params: new URLSearchParams(query) });
  await settle(); await settle();
  env.calls = calls;
  env.panel = env.root.querySelector('.pblocks');
  env.button = (text) => [...env.panel.querySelectorAll('button')].find(el => el.textContent.trim() === text);
  return env;
}
const banners = (env) => [...env.panel.querySelectorAll('.banner')].map(el => el.textContent);

test('nothing is searched until the button is pressed, and only Duplicados has the panel', async () => {
  const env = await duplicados();
  assert.equal(env.panel.querySelector('h2').textContent, 'Repeated blocks inside functions');
  assert.ok(env.button('Search for repeated blocks'));
  assert.deepEqual(env.calls.map(call => call.method), ['GET']);
  assert.equal(env.panel.querySelector('.pblock'), null);
  env.cleanup();
  const dead = await duplicados({ page: 'codigo-muerto' });
  assert.equal(dead.panel, null);
  assert.equal(dead.calls.length, 0);
  dead.cleanup();
});

test('a started search is polled until done, then lists each block with file, lines and function', async () => {
  const env = await duplicados({ states: [{ status: 'idle' }, { status: 'running' }, done([block(1)])] });
  env.button('Search for repeated blocks').dispatchEvent(env.event('click'));
  await settle();
  assert.match(env.panel.querySelector('[role="status"]').textContent, /You can keep using the dashboard/);
  assert.equal(env.calls[1].method, 'POST');
  assert.ok('csrf' in JSON.parse(env.calls[1].body));
  await new Promise(resolve => setTimeout(resolve, 1000));
  assert.deepEqual(env.calls.map(call => call.method), ['GET', 'POST', 'GET', 'GET']);
  assert.match(env.panel.querySelector('.pblocks-summary').textContent, /^Blocks: 1 · functions searched: 6/);
  assert.match(env.panel.querySelector('.pblock-head').textContent, /Block 1rust · 11 lines · 68 tokens · 2 functions/);
  const places = [...env.panel.querySelectorAll('.pblock-open')];
  assert.deepEqual(places.map(el => el.textContent), [
    'src/blocks.rs:12–22summarize_orders', 'src/blocks.rs:28–38summarize_refunds']);
  assert.match(banners(env).join(' '), /not a proposal to extract/);
  // No limit was hit: nothing says the list is partial.
  assert.doesNotMatch(banners(env).join(' '), /Partial analysis|Incomplete index/);
  // Each location opens the guarded source view at its lines.
  const sources = [];
  const fetch = env.context.fetch;
  env.context.fetch = (path, options) => { if (String(path).includes('source')) sources.push(String(path)); return fetch(path, options); };
  places[1].dispatchEvent(env.event('click'));
  await settle();
  assert.equal(sources.length, 1);
  assert.match(sources[0], /path=src%2Fblocks\.rs/);
  assert.match(sources[0], /start=8&/, 'the lines before the block give it context');
  env.cleanup();
});

test('zero blocks, an exhausted budget and an incomplete index read differently', async () => {
  const none = await duplicados({ states: [done([])] });
  assert.equal(none.panel.querySelector('.empty h3').textContent, 'No repeated blocks');
  assert.deepEqual(banners(none), []);
  assert.match(none.panel.querySelector('.pblocks-summary').textContent, /^Blocks: 0 ·/);
  none.cleanup();

  const budget = await duplicados({ states: [done([], { budget: { minTokens: 40, minLines: 6, functionsSearched: 3, exhausted: 'max-windows' } })] });
  assert.equal(budget.panel.querySelector('.empty h3').textContent, 'No repeated blocks in what was analyzed');
  assert.match(banners(budget)[0], /Partial analysis: the search stopped at its max-windows budget/);
  assert.match(budget.panel.querySelector('.pblocks-summary').textContent, /^Blocks so far: 0 \(partial analysis\)/);
  budget.cleanup();

  const index = await duplicados({ states: [done([block(1)], { indexCoverage: 'partial', indexSyntaxErrors: 2, indexReadFailures: 1, skipped: 4 })] });
  assert.match(banners(index)[0], /Incomplete index: files with syntax errors: 2; unreadable files: 1\./);
  assert.match(banners(index)[1], /were not searched: 4\./);
  assert.equal(index.panel.querySelectorAll('.pblock').length, 1);
  index.cleanup();

  const unknown = await duplicados({ states: [done([], { indexCoverage: 'unknown' })] });
  assert.match(banners(unknown)[0], /records no indexing run/);
  unknown.cleanup();
});

test('another page is read from the kept result and starts no new search', async () => {
  const first = done(Array.from({ length: 25 }, (_, id) => block(id + 1)), { total: 30 });
  const second = done(Array.from({ length: 5 }, (_, id) => block(id + 26)), { total: 30, page: 1 });
  const env = await duplicados({ states: [first, second] });
  assert.equal(env.panel.querySelectorAll('.pblock').length, 25);
  env.panel.querySelector('.pager button[aria-label="Next page"]').dispatchEvent(env.event('click'));
  await settle();
  assert.deepEqual(env.calls.map(call => [call.method, call.page]), [['GET', null], ['GET', '1']]);
  assert.equal(env.panel.querySelectorAll('.pblock').length, 5);
  assert.match(env.panel.querySelector('.pblock-head').textContent, /Block 26/);
  env.cleanup();
});

test('an outdated or failed search says so and offers to run again', async () => {
  const outdated = await duplicados({ states: [{ status: 'outdated' }], started: done([block(1)]) });
  assert.match(banners(outdated)[0], /index changed while the search ran/);
  outdated.button('Search again').dispatchEvent(outdated.event('click'));
  await settle();
  assert.equal(outdated.panel.querySelectorAll('.pblock').length, 1);
  outdated.cleanup();

  const failed = await duplicados({ states: [{ status: 'failed', error: 'cannot open the index' }] });
  assert.match(failed.panel.querySelector('[role="alert"]').textContent, /cannot open the index/);
  assert.ok(failed.button('Retry'));
  failed.cleanup();
});

test('an open finding hides the panel with the list, and leaving the page stops the polling', async () => {
  const env = await duplicados({ states: [{ status: 'running' }], query: 'key=duplicate-test' });
  assert.equal(env.panel.hidden, true);
  const before = env.calls.length;
  env.cleanup();
  await new Promise(resolve => setTimeout(resolve, 800));
  assert.equal(env.calls.length, before);
});
