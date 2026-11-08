import test from 'node:test';
import assert from 'node:assert/strict';
import { harness, settle, finding, defaultApi, list, symbolDetail } from './harness.mjs';

const findButton = (root, text) => [...root.querySelectorAll('button')].find(el => el.textContent.trim() === text);
// `shell`: the page sits inside the app shell, whose sidebar an open finding folds.
async function findings(fetch = defaultApi, query = 'key=duplicate-test', { shell = false, page = 'duplicados' } = {}) {
  const env = harness({ hash: `#/${page}?${query}`, fetch });
  if (shell) {
    env.shell = env.document.createElement('div');
    env.shell.className = 'shell';
    env.root.replaceWith(env.shell);
    env.shell.append(env.root);
  }
  const { render } = await env.load('page-findings.js');
  env.cleanup = await render({ root: env.root, page, params: new URLSearchParams(query) });
  await settle(); await settle();
  env.mounts = (await env.load('review.js')).mounts;
  return env;
}
const urlParams = (env) => new URLSearchParams(env.location.hash.split('?')[1]);
const shown = (env, selector) => !env.root.querySelector(selector).hidden;

// The open finding is review.js (the React workspace, tested with Vitest in web-ui); these
// tests check the page's side: when it mounts, what it hands over, and what the URL becomes.
test('an open duplicate gives the list way to one mounted workspace, with its stylesheet', async () => {
  const env = await findings();
  assert.equal(env.mounts.length, 1);
  const { props } = env.mounts[0];
  assert.equal(props.item.key, 'duplicate-test');
  assert.equal(props.file, 0);
  assert.equal(env.root.querySelector('.review-stub').textContent, 'alpha ≈ beta');
  assert.equal(shown(env, '.review-host'), true);
  assert.equal(shown(env, '.workbench'), false);
  assert.equal(shown(env, '.page-head'), false);
  assert.equal(env.document.querySelectorAll('link[data-review-css]').length, 1);
  assert.equal(env.document.querySelector('link[data-review-css]').getAttribute('href'), 'review.css');
  env.cleanup();
  assert.equal(env.mounts[0].unmounted, true, 'leaving the page unmounts the workspace');
});

// Duplicados and Código muerto: a filter bar (state, files, search, order, more filters) over blocks.
test('a stacked list shows each finding as a block and filters from its bar', async () => {
  const numbered = { ...finding, number: 4 };
  const facets = { state: [['unreviewed', 1]], scope: [['same-file', 1]] };
  const env = await findings((url) => (url.pathname.endsWith('/findings') ? list([numbered], facets) : defaultApi(url)), '');
  const block = env.root.querySelector('.stack-row');
  assert.equal(block.querySelector('.stack-no').textContent, '#4');
  assert.equal(block.querySelector('.stack-title').textContent, 'alpha ≈ beta');
  assert.equal(block.querySelector('.stack-where').textContent, '2 files');
  assert.match(block.querySelector('.pill').textContent, /Unreviewed/);
  const [state, files, sort] = [...env.root.querySelectorAll('.stack-bar select')];
  assert.equal(state.value, 'pending');
  assert.equal(files.value, 'all');
  assert.equal(sort.value, 'rank:asc');
  state.value = 'all'; state.dispatchEvent(env.event('change'));
  files.value = 'same-file'; files.dispatchEvent(env.event('change'));
  sort.value = 'lines:desc'; sort.dispatchEvent(env.event('change'));
  await settle();
  const params = urlParams(env);
  assert.deepEqual([params.get('state'), params.get('scope'), params.get('sort'), params.get('order')], ['all', 'same-file', 'lines', 'desc']);
  const panel = env.root.querySelector('aside.filters-col');
  [...env.root.querySelectorAll('.stack-bar button')].find(el => el.textContent === 'More filters').click();
  assert.equal(panel.classList.contains('open'), true);
  panel.dispatchEvent(env.event('keydown', { key: 'Escape' }));
  assert.equal(panel.classList.contains('open'), false);
  env.cleanup();
});

test('leaving with a finding open, the menu link comes back to the list with its filters', async () => {
  const env = await findings(defaultApi, 'state=all');
  const core = await env.load('core.js');
  core.patchQuery({ n: 3, file: 1 }); await settle();
  assert.equal(env.location.hash, '#/duplicados?state=all&n=3&file=1');
  assert.equal(core.hrefTo('duplicados'), '#/duplicados?state=all');
  env.cleanup();
});

// The workspace belongs to Duplicados and Código muerto: Revisiones sends a finding to its own page.
test('Revisiones never mounts the workspace: an old link moves to the finding on its own page', async () => {
  const env = await findings(defaultApi, 'key=duplicate-test', { shell: true, page: 'revisiones' });
  assert.equal(env.mounts.length, 0);
  assert.equal(env.root.querySelector('.review-host').hidden, true);
  assert.equal(env.shell.classList.contains('collapsed'), false);
  assert.equal(env.location.hash, '#/duplicados?key=duplicate-test');
  assert.equal((await env.load('core.js')).hrefTo('revisiones'), '#/revisiones', 'the menu link opens the list again');
  env.cleanup();
});

test('a Revisiones row opens the finding on its own page', async () => {
  const numbered = { ...finding, number: 4 };
  const fetch = (url) => (url.pathname.endsWith('/findings') ? list([numbered]) : defaultApi(url));
  const env = await findings(fetch, '', { page: 'revisiones' });
  env.root.querySelector('tbody tr').click();
  await settle();
  assert.equal(env.location.hash, '#/duplicados?n=4');
  assert.equal(env.mounts.length, 0);
  env.cleanup();
});

test('choosing a file goes to the URL and the mounted workspace, without fetching the finding again', async () => {
  const lookups = [];
  const env = await findings((url) => { if (url.pathname.endsWith('/finding')) lookups.push(url); return defaultApi(url); });
  env.mounts[0].props.onSelectFile(2); await settle();
  assert.equal(urlParams(env).get('file'), '2');
  assert.equal(env.mounts.length, 1);
  assert.equal(env.mounts[0].props.file, 2);
  // Older comparison links named the file `left`; an index out of range falls back to the first.
  const core = await env.load('core.js');
  core.patchQuery({ file: null, left: '1' }); await settle();
  assert.equal(env.mounts[0].props.file, 1);
  core.patchQuery({ left: null, file: '9' }); await settle();
  assert.equal(env.mounts[0].props.file, 0);
  assert.equal(lookups.length, 1);
  env.cleanup();
});

test('a numbered finding opens from its number and is selected in the list', async () => {
  const lookups = [];
  const numbered = { ...finding, number: 7 };
  const env = await findings((url) => {
    const route = url.pathname.split('/').at(-1);
    if (route === 'findings') return { items: [numbered], total: 1, all: 1, size: 25, page: 0, facets: {} };
    if (route === 'finding') { lookups.push(Object.fromEntries(url.searchParams)); return numbered; }
    return defaultApi(url);
  }, 'n=7');
  assert.deepEqual(lookups, [{ type: 'duplicate', number: '7' }]);
  assert.equal(env.root.querySelector('.stack-row .stack-no').textContent, '#7');
  assert.equal(env.root.querySelector('.stack-row').classList.contains('current'), true);
  assert.equal(env.mounts[0].props.item.number, 7);
  env.root.querySelector('.stack-row .stack-open').dispatchEvent(env.event('click'));
  await settle(); await settle();
  assert.equal(lookups.length, 1, 'the open finding is not fetched again');
  assert.equal(env.mounts.length, 1);
  assert.equal(urlParams(env).get('n'), '7');
  assert.equal(urlParams(env).get('key'), null);
  env.cleanup();
});

test('Back clears the finding from the URL, unmounts the workspace and shows the list again', async () => {
  const env = await findings(defaultApi, 'key=duplicate-test&view=side&file=1');
  env.mounts[0].props.onBack(); await settle();
  for (const name of ['key', 'n', 'view', 'file', 'left', 'right', 'context']) assert.equal(urlParams(env).get(name), null, name);
  assert.equal(env.mounts[0].unmounted, true);
  assert.equal(shown(env, '.review-host'), false);
  assert.equal(shown(env, '.workbench'), true);
  assert.equal(shown(env, '.page-head'), true);
  env.cleanup();
});

test('an open finding folds the shell sidebar, and Back unfolds only what it folded', async () => {
  const env = await findings(defaultApi, 'key=duplicate-test', { shell: true });
  assert.equal(env.shell.classList.contains('collapsed'), true);
  env.mounts[0].props.onBack(); await settle();
  assert.equal(env.shell.classList.contains('collapsed'), false);
  // Folded by the person beforehand: it stays folded.
  const core = await env.load('core.js');
  core.patchQuery({ key: 'duplicate-test' }); await settle(); await settle();
  env.mounts.at(-1).props.onBack(); await settle();
  env.shell.classList.add('collapsed');
  core.patchQuery({ key: 'duplicate-test' }); await settle(); await settle();
  env.mounts.at(-1).props.onBack(); await settle();
  assert.equal(env.shell.classList.contains('collapsed'), true);
  env.cleanup();
});

test('a saved decision shows at once, then the list and the finding are fetched again', async () => {
  const calls = { findings: 0, finding: 0 };
  const env = await findings((url) => {
    const route = url.pathname.split('/').at(-1);
    if (route in calls) calls[route] += 1;
    return defaultApi(url);
  });
  const before = { ...calls };
  env.mounts[0].props.onSaved({ key: 'duplicate-test', state: 'confirmed', note: 'checked', revision: 'r1' });
  assert.equal(env.mounts[0].props.item.state, 'confirmed');
  assert.equal(env.mounts[0].props.item.note, 'checked');
  await settle(); await settle();
  assert.equal(calls.findings, before.findings + 1);
  assert.equal(calls.finding, before.finding + 1);
  assert.equal(env.mounts.length, 1, 'the workspace stays mounted, with its drafts');
  env.cleanup();
});

test('the state in the list opens the other decisions and saves the pick for the whole group', async () => {
  const saved = [];
  const env = await findings((url, options) => {
    if (url.pathname.endsWith('/review')) {
      saved.push(JSON.parse(options.body));
      return { key: 'duplicate-test', state: 'confirmed', revision: 'r1' };
    }
    return defaultApi(url);
  }, '');
  const button = env.root.querySelector('.stack-row .state-btn');
  assert.equal(button.getAttribute('aria-haspopup'), 'menu');
  assert.equal(button.getAttribute('aria-expanded'), 'false');
  button.dispatchEvent(env.event('click'));
  const menu = env.document.querySelector('.state-menu');
  assert.equal(button.getAttribute('aria-expanded'), 'true');
  assert.match(menu.querySelector('.state-menu-note').textContent, /all 3 files in the group/);
  const items = [...menu.querySelectorAll('[role=menuitem]')];
  assert.equal(items.length, 5, 'every decision but the current one; an unreviewed finding has nothing to reopen');
  assert.equal(env.document.activeElement, items[0]);
  items[1].dispatchEvent(env.event('click'));
  await settle(); await settle();
  assert.equal(env.document.querySelector('.state-menu'), null, 'picking closes the menu');
  assert.equal(saved.length, 1);
  assert.deepEqual([saved[0].key, saved[0].state, saved[0].revision], ['duplicate-test', 'confirmed', 'none']);
  env.cleanup();
});

test('the bridge reaches the source and review APIs', async () => {
  const requests = [];
  const env = await findings((url, options) => {
    if (/\/(source|review)$/.test(url.pathname)) requests.push({ url, options });
    if (url.pathname.endsWith('/review')) return { key: 'duplicate-test', state: 'confirmed', revision: 'r2' };
    return defaultApi(url);
  });
  const { bridge } = env.mounts[0].props;
  const source = await bridge.source({ path: 'src/same.cpp', start: 1, end: 3 }, 'review-member-0');
  assert.deepEqual(source.lines, ['line 1']);
  assert.equal(requests[0].url.searchParams.get('path'), 'src/same.cpp');
  const saved = await bridge.save('duplicate-test', 'confirmed', 'note', 'none');
  assert.equal(saved.revision, 'r2');
  assert.equal(requests[1].options.method, 'POST');
  assert.deepEqual(
    (({ key, state, note, revision }) => ({ key, state, note, revision }))(JSON.parse(requests[1].options.body)),
    { key: 'duplicate-test', state: 'confirmed', note: 'note', revision: 'none' },
  );
  assert.equal(bridge.t('dup.backToList'), 'Back to the list');
  env.cleanup();
});

test('a finding that fails to load keeps Back and Retry, and Retry mounts the workspace', async () => {
  let fail = true;
  const env = await findings((url) => {
    if (url.pathname.endsWith('/finding') && fail) return new Response('{"error":"fixture failure"}', { status: 500 });
    return defaultApi(url);
  });
  assert.equal(env.mounts.length, 0);
  assert.ok(findButton(env.root, 'Back to the list'));
  fail = false;
  findButton(env.root, 'Retry').click(); await settle(); await settle();
  assert.equal(env.mounts.length, 1);
  assert.equal(env.root.querySelector('.review-host').textContent.includes('fixture failure'), false);
  env.cleanup();
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

// A dead-code candidate opens in the same workspace; its context lines come from the URL.
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
    if (route === 'source') requests.push(url);
    return defaultApi(url);
  };
}
async function deadPage(query) {
  const requests = [];
  const env = harness({ hash: `#/codigo-muerto?${query}`, fetch: deadApi(requests) });
  const { render } = await env.load('page-findings.js');
  env.cleanup = await render({ root: env.root, page: 'codigo-muerto', params: new URLSearchParams(query) });
  await settle(); await settle();
  env.mounts = (await env.load('review.js')).mounts;
  env.requests = requests;
  return env;
}

test('a dead-code candidate mounts the workspace with the context lines its URL names', async () => {
  const env = await deadPage('key=dead-test&view=code&context=10');
  assert.equal(env.mounts.length, 1);
  assert.equal(env.mounts[0].props.item.type, 'dead-code');
  assert.equal(env.mounts[0].props.context, 10);
  // The workspace loads the code itself; the page does not.
  assert.equal(env.requests.length, 0);
  env.mounts[0].props.onContext(25); await settle();
  assert.equal(urlParams(env).get('context'), '25');
  assert.equal(env.mounts[0].props.context, 25);
  assert.equal(env.mounts.length, 1);
  env.cleanup();
});

test('a dead-code link without context gets three lines', async () => {
  const env = await deadPage('key=dead-test');
  assert.equal(env.mounts[0].props.context, 3);
  env.cleanup();
});

test('the dead-code list row opens the workspace instead of a dialog', async () => {
  const env = await deadPage('');
  assert.equal(env.mounts.length, 0);
  const block = env.root.querySelector('.stack-row .stack-open');
  assert.match(block.textContent, /unused_helper/);
  block.dispatchEvent(env.event('click'));
  await settle(); await settle();
  assert.equal(urlParams(env).get('key'), 'dead-test');
  assert.equal(urlParams(env).get('view'), null);
  assert.equal(env.root.querySelector('dialog'), null);
  assert.equal(env.mounts.length, 1);
  env.cleanup();
});

test('each dead-code row copies the AI template of its candidate and confirms it', async () => {
  const copied = [];
  const env = await deadPage('');
  env.context.navigator.clipboard.writeText = async (text) => { copied.push(text); };
  const toasts = env.document.createElement('div'); toasts.className = 'toasts'; env.document.body.append(toasts);
  const button = env.root.querySelector('.stack-row button[aria-label="Copy unused code for AI: unused_helper"]');
  assert.ok(button, 'the row has the button');
  assert.equal(button.textContent.trim(), 'Copy AI template', 'the button says what it copies');
  button.click(); await settle();
  assert.equal(copied.length, 1);
  assert.match(copied[0], /# Task: check whether this dead-code candidate can be deleted/);
  assert.match(copied[0], /src\/lib\.rs:10-12/);
  assert.match(env.document.querySelector('.toasts [role=status]').textContent, /Copied with context/);
  env.cleanup();
});

// Archivos can show and edit the project's .mctignore without leaving the page.
test('the explorer opens the .mctignore editor in place of the viewer, and closes it again', async () => {
  const ignore = { content: 'target/\n', exists: true, directive: '#import .gitignore', template: '', command: 'mct', maxBytes: 4096, gitignorePresent: true };
  const env = harness({ hash: '#/espacio', fetch: async (url) => (url.pathname.split('/').at(-1) === 'ignore' ? ignore : defaultApi(url)) });
  const { render } = await env.load('page-workspace.js');
  env.cleanup = await render({ root: env.root, params: new URLSearchParams('') });
  await settle();
  const toggle = [...env.root.querySelectorAll('.explorer button')].find(el => el.textContent.includes('.mctignore'));
  assert.equal(toggle.getAttribute('aria-pressed'), 'false');
  assert.equal(env.root.querySelector('textarea'), null);
  toggle.dispatchEvent(env.event('click'));
  await settle(); await settle();
  assert.equal(urlParams(env).get('view'), 'ignore');
  assert.equal(env.root.querySelector('textarea').value, 'target/\n');
  const open = [...env.root.querySelectorAll('.explorer button')].find(el => el.textContent.includes('.mctignore'));
  assert.equal(open.getAttribute('aria-pressed'), 'true');
  open.dispatchEvent(env.event('click'));
  await settle(); await settle();
  assert.equal(urlParams(env).get('view'), null);
  assert.equal(env.root.querySelector('textarea'), null);
  env.cleanup();
});

test('the explorer opens .gitignore with the same editor, but without the .mctignore-only options', async () => {
  const requests = [];
  const files = { mctignore: 'docs/\n', gitignore: 'target/\n' };
  const fetch = async (url, options) => {
    if (url.pathname.split('/').at(-1) !== 'ignore') return defaultApi(url);
    if (options?.method === 'POST') {
      const { content } = JSON.parse(options.body);
      files.gitignore = content;
      return { file: 'gitignore', content, exists: true, directive: '@import-gitignore', template: 'starter', command: 'mct', maxBytes: 4096, gitignorePresent: true };
    }
    const file = url.searchParams.get('file');
    requests.push(file);
    return { file, content: files[file], exists: true, directive: '@import-gitignore', template: 'starter', command: 'mct', maxBytes: 4096, gitignorePresent: true };
  };
  const env = harness({ hash: '#/espacio', fetch });
  const { render } = await env.load('page-workspace.js');
  env.cleanup = await render({ root: env.root, params: new URLSearchParams('') });
  await settle();
  const button = (text) => [...env.root.querySelectorAll('.explorer button')].find(el => el.textContent.includes(text));
  button('.gitignore').dispatchEvent(env.event('click'));
  await settle(); await settle();
  assert.equal(urlParams(env).get('view'), 'gitignore');
  assert.deepEqual(requests, ['gitignore']);
  assert.equal(env.root.querySelector('textarea').value, 'target/\n');
  assert.equal(button('.gitignore').getAttribute('aria-pressed'), 'true');
  assert.equal(button('.mctignore').getAttribute('aria-pressed'), 'false');
  // Save and discard stay; the import toggle and the starter template are .mctignore's own.
  const card = [...env.root.querySelectorAll('section.card button')].map(el => el.textContent.trim());
  assert.deepEqual(card, ['Save .gitignore', 'Discard changes']);
  // Saving says to wait: the dashboard reindexes on its own and the page redraws when it is done.
  const status = () => env.root.querySelector('section.card [role=status]');
  assert.equal(status().textContent, '');
  const area = env.root.querySelector('textarea');
  area.value = 'target/\n*.log\n';
  area.dispatchEvent(env.event('input'));
  env.root.querySelector('section.card .btn.primary').dispatchEvent(env.event('click'));
  await settle(); await settle();
  assert.match(status().textContent, /Wait up to a minute/);
  area.value = 'target/\n';
  area.dispatchEvent(env.event('input'));
  assert.equal(status().textContent, '', 'editing again clears it');
  button('.mctignore').dispatchEvent(env.event('click'));
  await settle(); await settle();
  assert.equal(urlParams(env).get('view'), 'ignore');
  assert.equal(env.root.querySelector('textarea').value, 'docs/\n');
  env.cleanup();
});

test('right-click in the explorer copies the path from the project root of a folder, a file and the folder holding it', async () => {
  const copied = [];
  const fetch = async (url) => (url.pathname.split('/').at(-1) === 'files' ? { files: [['src/a.rs', 'rust', 1]] } : defaultApi(url));
  const env = harness({ hash: '#/espacio', fetch });
  env.context.navigator.clipboard.writeText = async (text) => { copied.push(text); };
  const { render } = await env.load('page-workspace.js');
  env.cleanup = await render({ root: env.root, params: new URLSearchParams('') });
  await settle();
  const node = (title) => [...env.root.querySelectorAll('.tree button.node')].find(el => el.title === title);
  const pick = async (title, index) => {
    node(title).dispatchEvent(env.event('contextmenu', { clientX: 5, clientY: 5 }));
    const items = [...env.document.querySelectorAll('.context-menu [role=menuitem]')];
    items[index].dispatchEvent(env.event('click'));
    await settle();
    assert.equal(env.document.querySelector('.context-menu'), null, 'choosing closes the menu');
    return items.length;
  };
  assert.equal(await pick('src', 0), 1);
  assert.equal(await pick('src/a.rs', 0), 2);
  assert.equal(await pick('src/a.rs', 1), 2);
  assert.deepEqual(copied, ['src', 'src/a.rs', 'src']);
  env.cleanup();
});

test('a duplicate row counts the files of the whole group, not of the members the list previews', async () => {
  const row = { ...finding, memberCount: 13, fileCount: 13 };
  const env = await findings(url => (url.pathname.endsWith('/findings') ? list([row]) : defaultApi(url)), '');
  await settle();
  assert.match(env.root.textContent, /13 files/);
  assert.doesNotMatch(env.root.textContent, /\b2 files/);
  const pop = env.root.querySelector('.stack-row .files-pop');
  assert.ok(pop, 'the row lists its fragments in its own panel');
  assert.match(pop.textContent, /and 10 more fragments/, 'the panel says how many fragments the preview leaves out');
  env.cleanup();
});

test('a stacked row lists its files in a panel that hover and focus reveal, not in a native title', async () => {
  const row = { ...finding, memberCount: 2, fileCount: 2 };
  const env = await findings(url => (url.pathname.endsWith('/findings') ? list([row]) : defaultApi(url)), '');
  await settle();
  const open = env.root.querySelector('.stack-row .stack-open');
  const pop = env.root.querySelector('.stack-row .files-pop');
  assert.ok(pop, 'the panel exists');
  assert.equal(pop.getAttribute('role'), 'tooltip');
  assert.equal(open.getAttribute('aria-describedby'), pop.id, 'the button is described by the panel');
  assert.equal(open.hasAttribute('title'), false, 'no native tooltip on the row');
  const lines = [...pop.querySelectorAll('.files-pop-line')].map(el => el.textContent);
  assert.equal(lines.length, row.members.length);
  for (const member of row.members) assert.ok(lines.some(line => line.startsWith(member.path)), member.path);
  env.cleanup();
});

test('a dead-code row names the full path of its candidate in the same panel', async () => {
  const env = await deadPage('');
  const open = env.root.querySelector('.stack-row .stack-open');
  const pop = env.root.querySelector('.stack-row .files-pop');
  assert.ok(pop, 'the panel exists');
  assert.equal(pop.textContent.trim(), 'src/lib.rs:10');
  assert.equal(open.getAttribute('aria-describedby'), pop.id);
  assert.equal(open.hasAttribute('title'), false);
  env.cleanup();
});

test('each duplicate group of a symbol has its own copy-for-AI button that confirms the copy', async () => {
  const copied = [];
  const group = { ...finding, memberCount: 3 };
  const fetch = url => (url.pathname.endsWith('/symbol') ? { ...symbolDetail, duplicates: [group, { ...group, key: 'second', title: 'alpha ≈ delta' }] } : defaultApi(url));
  const env = harness({ hash: '#/codigo?sel=1', fetch });
  env.context.navigator.clipboard.writeText = async (text) => { copied.push(text); };
  const toasts = env.document.createElement('div'); toasts.className = 'toasts'; env.document.body.append(toasts);
  const { render } = await env.load('page-symbols.js');
  const cleanup = await render({ root: env.root, params: new URLSearchParams('sel=1') });
  await settle();
  const buttons = [...env.root.querySelectorAll('button[aria-label^="Copy duplicate for AI:"]')];
  assert.deepEqual(buttons.map(b => b.getAttribute('aria-label')), ['Copy duplicate for AI: alpha ≈ beta', 'Copy duplicate for AI: alpha ≈ delta']);
  buttons[1].click(); await settle();
  assert.equal(copied.length, 1);
  assert.match(copied[0], /Duplicate group second:/);
  assert.doesNotMatch(copied[0], /Duplicate group duplicate-test:/, 'only the chosen group is copied');
  assert.match(env.document.querySelector('.toasts [role=status]').textContent, /Copied with context/);
  cleanup();
});

test('the dashboard shows the server\'s pending count, which includes findings that need re-evaluation', async () => {
  const env = harness({ hash: '#/panel' });
  const { render } = await env.load('page-dashboard.js');
  render({ root: env.root,
    summary: { symbols: 0, files: 0, duplicates: { groups: 5, complete: true, skipped: 0 }, deadCode: { candidates: 2 },
      findings: { total: 7, states: { unreviewed: 2, 'in-review': 1, 'needs-reevaluation': 3, resolved: 1 },
        pending: { duplicate: 4, 'dead-code': 2 } },
      notes: 0, languages: [], reviewActivity: [], recent: [], health: { readFailures: 0, syntaxErrors: 0, pendingReviews: 6 } },
    meta: { project: 'fixture', indexing: { state: 'ready', lastIndexedAt: null } },
  });
  const counts = [...env.root.querySelectorAll('.count-box')].map(el => el.textContent.trim());
  assert.deepEqual(counts.slice(0, 2), ['4', '2']);
  const health = [...env.root.querySelectorAll('dt')].find(el => el.textContent === 'Pending reviews');
  const pending = health ? health.nextElementSibling : null;
  assert.ok(pending, 'the health row is shown');
  assert.equal(pending.textContent.trim(), '6');
  assert.match(pending.querySelector('a').getAttribute('href'), /needs-reevaluation/);
});

test('each duplicate row copies the AI template of its whole group and confirms it', async () => {
  const copied = [];
  const full = { ...finding, memberCount: 3, members: finding.members.map(m => ({ ...m, code: m.excerpt })) };
  const fetch = url => (url.pathname.endsWith('/findings') ? list([{ ...finding, members: finding.members.slice(0, 1), memberCount: 3 }]) : url.pathname.endsWith('/finding') ? full : defaultApi(url));
  const env = await findings(fetch, '');
  env.context.navigator.clipboard.writeText = async (text) => { copied.push(text); };
  const toasts = env.document.createElement('div'); toasts.className = 'toasts'; env.document.body.append(toasts);
  await settle();
  const button = env.root.querySelector('button[aria-label="Copy duplicate for AI: alpha ≈ beta"]');
  assert.ok(button, 'the row has the button');
  button.click(); await settle();
  assert.equal(copied.length, 1);
  assert.match(copied[0], /# Task: decide whether this duplicate-code group should be merged/);
  assert.match(copied[0], /3 copies in 2 files/, 'the brief uses the full group, not the preview');
  assert.match(env.document.querySelector('.toasts [role=status]').textContent, /Copied with context/);
  env.cleanup();
});

test('copying the duplicate list uses grouped locations and omits internal review keys', async () => {
  const copied = [];
  const row = { ...finding, number: 12 };
  const env = await findings(url => url.pathname.endsWith('/findings') ? list([row]) : defaultApi(url), '');
  env.context.navigator.clipboard.writeText = async text => copied.push(text);
  const button = [...env.root.querySelectorAll('button')].find(button => button.textContent.includes('Copy list'));
  assert.ok(button);
  button.click(); await settle();
  assert.equal(copied.length, 1);
  assert.match(copied[0], /Duplicate 12: alpha ≈ beta\n - src\/same.cpp L1-3\n - src\/same.cpp L5-7/);
  assert.doesNotMatch(copied[0], /keys:|duplicate-test| ↔ /);
  env.cleanup();
});

async function filteredDeadPage(query, typeCount) {
  const fetch = async (url) => {
    const route = url.pathname.split('/').at(-1);
    if (route === 'findings')
      return { items: [deadFinding], total: 1, all: 1, size: 25, page: 0, facets: { type: [['dead-code', typeCount]] } };
    return defaultApi(url);
  };
  const env = harness({ hash: `#/codigo-muerto?${query}`, fetch });
  const { render } = await env.load('page-findings.js');
  env.cleanup = await render({ root: env.root, page: 'codigo-muerto', params: new URLSearchParams(query) });
  await settle(); await settle();
  return env;
}

test('a filtered dead-code list says how many candidates exist without filters and clears them', async () => {
  const env = await filteredDeadPage('zone=website', 302);
  const pager = env.root.querySelector('.pager');
  assert.match(pager.textContent, /Showing 1–1 of 1/);
  assert.match(pager.textContent, /302 in total/);
  const clear = pager.querySelector('button.pager-clear');
  assert.ok(clear, 'the pager offers to clear the filters');
  clear.click(); await settle();
  assert.match(env.context.location.hash, /zone=all/);
  assert.match(env.context.location.hash, /state=all/);
  env.cleanup();
});

test('an unfiltered list has no total hint', async () => {
  const env = await filteredDeadPage('zone=all&state=all', 1);
  const pager = env.root.querySelector('.pager');
  assert.doesNotMatch(pager.textContent, /in total/);
  assert.equal(pager.querySelector('button.pager-clear'), null);
  env.cleanup();
});

test('the Obsidian graph draws edges sent as positions in the node list', async () => {
  const nodes = ['a.md', 'b.md', 'c.md'].map(id => ({ id, label: id, folder: '', tags: [], degree: 1, orphan: false, hasCode: false }));
  const fetch = (url) => url.pathname.endsWith('/notes-graph') ? { nodes, edges: [[0, 1], [1, 2]], total: 3 } : defaultApi(url);
  const env = harness({ hash: '#/obsidian?view=grafo', fetch });
  const { render } = await env.load('page-notes.js');
  const cleanup = await render({ root: env.root, params: new URLSearchParams('view=grafo') });
  await settle(); await settle();
  assert.equal(env.root.querySelectorAll('svg line').length, 2);
  cleanup();
});
