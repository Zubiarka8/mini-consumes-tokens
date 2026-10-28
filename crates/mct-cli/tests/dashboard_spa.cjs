'use strict';

// Deterministic tests of the shipped Alpine component; no browser or network.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function harness(respond = async () => ({})) {
  let factory;
  const requests = [];
  const context = {
    document: { addEventListener: (_, fn) => fn(), querySelector: () => ({ content: 'csrf' }), hidden: false },
    Alpine: { data: (_, fn) => { factory = fn; } },
    navigator: { language: 'en' },
    localStorage: { getItem: () => null, setItem: () => {} },
    URLSearchParams,
    setTimeout: () => 1, clearTimeout: () => {},
    fetch: async (url, options) => {
      const request = { url: new URL(url, 'http://test/'), body: options && options.body ? JSON.parse(options.body) : null };
      requests.push(request);
      const response = await respond(request);
      return { ok: !response.status || response.status === 200, status: response.status || 200, json: async () => response.data === undefined ? response : response.data };
    },
  };
  vm.createContext(context);
  vm.runInContext(fs.readFileSync(path.join(__dirname, '../src/web/spa/spa.js'), 'utf8'), context);
  return { app: factory(), requests, context };
}

const state = { status: 'pending' };
test('CSP warning binding exists before the first asynchronous finding load', () => {
  const { app } = harness();
  // Alpine CSP resolves dotted bindings during bootstrap, before rebuildDetail
  // replaces activeFile with a loaded copy. An absent key throws in the browser.
  assert.equal(app.activeFile.warning, '');
  selectGroup(app);
  assert.equal(app.activeFile.warning, '');
});
function group(id = 'group') {
  return { identity: id, fingerprint: 'fp-v2', revision: 1, language: 'rust', line_count: 3, token_count: 25, state,
    members: ['a.rs', 'b.rs'].map(path => ({ path, symbol: path, start_line: 1, end_line: 3, line_count: 3,
      excerpt: ['fn f() {', '  1', '}'], excerpt_first: 1, excerpt_available: true, excerpt_truncated: false, source_next_offset: null })) };
}
function detail(id = 'group', files = []) {
  return { identity: id, kind: 'duplicate', current: true, fingerprint: 'fp-v2', revision: 1, state,
    files, history: [], comments: [] };
}
function selectGroup(app, item = group(), files = []) {
  app.view = 'dup'; app.editor = true; app.sel = item.identity; app.dups = [item]; app.selItem = item;
  app.detail = detail(item.identity, files); app.syncFile(); app.syncReview(); app.rebuildDetail();
}
function deferred() {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
}

// One page is sufficient even when hundreds of candidates exist; filtering is
// sent to the server rather than applied to a preloaded full dataset.
test('loads one filtered page rather than preloading 330 findings', async () => {
  const { app, requests } = harness(async () => ({ items: Array.from({ length: 50 }, (_, n) => group(String(n))),
    total: 330, offset: 0, limit: 50, snapshot: 's1', counts: { all: 330, pending: 330 } }));
  app.view = 'dup'; app.query = 'needle'; app.filter = 'pending';
  await app.loadPage(true);
  assert.equal(requests.length, 1);
  assert.equal(requests[0].url.searchParams.get('limit'), '50');
  assert.equal(requests[0].url.searchParams.get('q'), 'needle');
  assert.equal(requests[0].url.searchParams.get('status'), 'pending');
  assert.equal(app.dups.length, 50);
  assert.equal(app.pageTotal, 330);
});

test('snapshot conflict restarts a page without mixing generations', async () => {
  const { app, requests } = harness(async ({ url }) => {
    if (url.searchParams.get('snapshot') === 'old') return { status: 409 };
    return { items: [group('new')], total: 1, offset: 0, limit: 50, snapshot: 'new', counts: { all: 1 } };
  });
  app.view = 'dup'; app.dups = [group('old')]; app.pageOffset = 50; app.pageSnapshot = 'old';
  await app.loadPage();
  assert.equal(requests.length, 2);
  assert.equal(requests[0].url.searchParams.get('snapshot'), 'old');
  assert.equal(requests[1].url.searchParams.get('offset'), '0');
  assert.equal(app.pageOffset, 0);
  assert.equal(app.dups.length, 1);
  assert.equal(app.dups[0].identity, 'new');
});

test('late search response cannot overwrite a newer page', async () => {
  const first = deferred();
  const { app } = harness(async ({ url }) => url.searchParams.get('q') === 'old' ? first.promise
    : ({ items: [group('new')], total: 1, offset: 0, snapshot: 's2', counts: {} }));
  app.view = 'dup'; app.query = 'old'; const pending = app.loadPage(true);
  app.query = 'new'; await app.loadPage(true);
  first.resolve({ items: [group('old')], total: 1, offset: 0, snapshot: 's1', counts: {} });
  await pending;
  assert.equal(app.dups[0].identity, 'new');
});

test('file drafts survive file and finding navigation', () => {
  const { app } = harness();
  const files = [{ path: 'a.rs', note: 'saved A', status: 'accepted', revision: 7 }];
  selectGroup(app, group(), files);
  app.markDirty({ target: { value: 'unsaved A' } });
  app.changeFile({ target: { value: '1' } });
  app.changeFile({ target: { value: '0' } });
  assert.equal(app.fileNote, 'unsaved A');
  assert.equal(app.fileDirty, true);
  assert.equal(app.drafts[app.activeDraftKey].expected_revision, 7);
  app.openFinding('other');
  app.dups = [group()]; app.openFinding('group'); app.detail = detail('group', files); app.syncFile();
  assert.equal(app.fileNote, 'unsaved A');
});

test('typing during a file save remains dirty with the new server revision', async () => {
  const saving = deferred();
  const files = [{ path: 'a.rs', note: 'first draft', status: 'accepted', revision: 2 }];
  const { app, requests } = harness(async ({ url }) => {
    if (url.pathname === '/api/file-review') return saving.promise;
    if (url.pathname === '/api/finding') return detail('group', files);
    return { items: [group()], snapshot: 's2' };
  });
  selectGroup(app, group(), [{ path: 'a.rs', note: '', status: 'accepted', revision: 1 }]);
  app.markDirty({ target: { value: 'first draft' } });
  const pending = app.saveFile();
  app.markDirty({ target: { value: 'later typing' } });
  saving.resolve({ revision: 2 });
  await pending;
  assert.equal(requests[0].body.expected_revision, 1);
  assert.equal(app.fileNote, 'later typing');
  assert.equal(app.fileDirty, true);
  assert.equal(app.drafts[app.activeDraftKey].expected_revision, 2);
});

test('file revision conflicts retain the draft and captured revision', async () => {
  const { app } = harness(async () => ({ status: 409 }));
  selectGroup(app, group(), [{ path: 'a.rs', note: '', status: 'pending', revision: 4 }]);
  app.markDirty({ target: { value: 'keep me' } });
  await app.saveFile();
  assert.equal(app.fileNote, 'keep me'); assert.equal(app.fileDirty, true); assert.equal(app.fileConflict, true);
  assert.equal(app.drafts[app.activeDraftKey].expected_revision, 4);
});

test('finding decision sends the reviewed fingerprint and revision', async () => {
  const { app, requests } = harness(async () => ({ status: 409 }));
  selectGroup(app);
  app.setReviewAction({ currentTarget: { dataset: { v: 'false_positive' } } });
  app.markReviewDirty({ target: { value: 'entry point' } });
  await app.saveReview();
  assert.equal(requests[0].url.pathname, '/api/review');
  assert.equal(requests[0].body.action, 'false_positive');
  assert.equal(requests[0].body.fingerprint, 'fp-v2');
  assert.equal(requests[0].body.expected_revision, 1);
  assert.equal(app.reviewDirty, true); assert.equal(app.reviewConflict, true);
  assert.equal(app.reviewReason, 'entry point');
});

for (const view of ['dup', 'dead']) {
  for (const action of ['false_positive', 'ignored', 'restore']) {
    test(`${view} ${action} refreshes the filtered queue before the next poll`, async () => {
      const item = view === 'dup' ? group() : {
        identity: 'group', fingerprint: 'fp-v2', revision: 1, name: 'entry',
        kind: 'function', language: 'rust', path: 'a.rs', line: 1, end_line: 3, state,
      };
      const effective = action === 'restore' ? 'pending' : action;
      const updated = { ...item, revision: 2, state: { status: effective } };
      let saved = false;
      const route = view === 'dup' ? '/api/duplicates' : '/api/dead';
      const { app, requests } = harness(async ({ url }) => {
        if (url.pathname === '/api/review') { saved = true; return { revision: 2 }; }
        if (url.pathname === '/api/finding') return { ...detail(), state: updated.state, revision: 2 };
        if (url.pathname === route) {
          if (url.searchParams.get('full') === '1') return listPage([updated]);
          assert.equal(saved, true);
          assert.equal(url.searchParams.get('status'), 'pending');
          assert.equal(url.searchParams.has('snapshot'), false);
          return listPage(effective === 'pending' ? [updated] : [], { counts: { pending: effective === 'pending' ? 1 : 0 } });
        }
        throw new Error(`Unexpected request: ${url}`);
      });
      selectGroup(app);
      app.view = view;
      app.selItem = item;
      if (view === 'dead') { app.deads = [item]; app.dups = []; }
      app.syncReview();
      app.filter = 'pending'; app.pageSnapshot = 'before-save';
      app.setReviewAction({ currentTarget: { dataset: { v: action } } });
      await app.saveReview();
      assert.ok(requests.some(({ url }) => url.pathname === route && !url.searchParams.has('full')),
        'saving must refresh the queue without a poll');
      assert.equal(app.items().length, effective === 'pending' ? 1 : 0);
      assert.equal(app.pageCounts.pending, effective === 'pending' ? 1 : 0);
      assert.equal(app.reviewStateText, app.t(effective));
      assert.equal(app.reviewDirty, false);
      // DASH-002: a dismissed finding has left the empty queue, so no ordinal.
      const word = app.t(view === 'dup' ? 'dupWord' : 'deadWord');
      if (effective === 'pending') assert.match(app.editorFooter, new RegExp(`^${word} #1 of 1 · `));
      else {
        assert.doesNotMatch(app.editorFooter, /#\d/);
        assert.match(app.editorFooter, new RegExp(`^${word} \\(not in this list · list empty\\) · `));
      }
    });
  }
}

// DASH-002: the editor position is the finding's place in the filtered queue,
// never an invented "#1" once it has left that queue.
test('editor position follows the filtered queue for kept and removed selections', () => {
  const { app } = harness();
  const kept = group('kept');
  selectGroup(app, kept);
  app.dups = [group('first'), kept, group('last')]; app.pageOffset = 50; app.pageTotal = 53;
  app.rebuildDetail();
  assert.match(app.editorFooter, /^Duplicate #52 of 53 · /);
  assert.equal(app.footer.title, 'Duplicate #52');
  app.dups = [group('first'), group('last')]; app.pageTotal = 52;
  app.rebuildDetail();
  assert.match(app.editorFooter, /^Duplicate \(not in this list · 52 left\) · /);
  assert.equal(app.footer.title, 'Duplicate');
  app.lang = 'es'; app.rebuildDetail();
  assert.match(app.editorFooter, /^Duplicado \(fuera de esta lista · quedan 52\) · /);
});

test('prompts retain effective decision, reason and source fingerprint', () => {
  const { app } = harness();
  const item = { name: 'entry', kind: 'function', language: 'rust', path: 'a.rs', line: 1,
    fingerprint: 'fp-reviewed', state: { status: 'false_positive', reason: 'public entry point' } };
  const prompt = app.deadPrompt(item);
  assert.match(prompt, /False positive/);
  assert.match(prompt, /public entry point/);
  assert.match(prompt, /fp-reviewed/);
  assert.match(prompt, /do not repeat the investigation/);
  assert.doesNotMatch(prompt, /Delete it/);
  const duplicate = group(); duplicate.state = { status: 'ignored', reason: 'separate lifecycles' };
  app.detail = detail();
  const copied = app.dupPrompt(duplicate, ['A', 'B']);
  assert.match(copied, /separate lifecycles/);
  assert.match(copied, /fp-v2/);
});

test('source reads stay bounded and expose partial code navigation', async () => {
  const { app, requests } = harness(async ({ url }) => {
    const offset = Number(url.searchParams.get('source_offset'));
    const item = group();
    item.members.forEach(member => Object.assign(member, {
      excerpt_first: 1 + offset, excerpt_truncated: true, source_next_offset: offset === 0 ? 400 : null,
    }));
    return { items: [item], snapshot: 's1' };
  });
  selectGroup(app); app.pageSnapshot = 's1';
  await app.loadFullCode('group');
  assert.equal(app.sourcePartial, true); assert.equal(app.sourceHasNext, true);
  await app.nextSource();
  assert.equal(requests[1].url.searchParams.get('source_offset'), '400');
  assert.equal(requests[1].url.searchParams.get('source_limit'), '400');
  assert.equal(requests[1].url.searchParams.get('snapshot'), 's1');
  assert.equal(app.panes[0].rows[0].n, 401);
});

test('orphan details are loaded only when requested and deletion is revision checked', async () => {
  const { app, requests } = harness(async ({ url }) => {
    if (url.pathname === '/api/reviews') return { items: [{ identity: 'gone', kind: 'dead', revision: 8, reason: '', at: 1 }], total: 1, offset: 0, snapshot: 's1', counts: {} };
    if (url.pathname === '/api/finding') return { history: [{ status: 'ignored', reason: 'prior reason', at: 1 }], comments: [{ body: 'annotation', updated_at: 2 }], files: [] };
    return { status: 409 };
  });
  app.view = 'orphans'; await app.loadPage(true);
  assert.equal(requests.length, 1);
  await app.inspectOrphan({ currentTarget: { dataset: { v: 'dead:gone' } } });
  assert.equal(app.orphanInfoRows.length, 2);
  await app.deleteOrphan({ currentTarget: { dataset: { v: 'dead:gone' } } });
  assert.equal(requests[2].body.expected_revision, 8);
});


test('polling never rebases an unsaved finding decision onto unseen code', async () => {
  const changed = group(); changed.fingerprint = 'changed-fp'; changed.revision = 3;
  const current = detail(); current.fingerprint = 'changed-fp'; current.revision = 3;
  const { app } = harness(async ({ url }) => url.pathname === '/api/finding' ? current
    : ({ items: [changed], snapshot: 's2' }));
  selectGroup(app);
  app.setReviewAction({ currentTarget: { dataset: { v: 'false_positive' } } });
  app.markReviewDirty({ target: { value: 'reviewed the original source' } });
  await app.loadDetail();
  assert.equal(app.reviewFingerprint, 'fp-v2');
  assert.equal(app.reviewRevision, 1);
  assert.equal(app.reviewConflict, true);
  assert.equal(app.reviewReason, 'reviewed the original source');
});

test('orphaned selection retains its drafts without presenting an editable live finding', async () => {
  const { app } = harness(async () => ({ ...detail(), current: false, fingerprint: null }));
  selectGroup(app);
  app.markDirty({ target: { value: 'draft before source removal' } });
  await app.loadDetail();
  assert.equal(app.selItem, null);
  assert.equal(app.missing, true);
  assert.equal(app.reviewSaveDisabled, true);
  assert.equal(app.drafts['duplicate:group:a.rs'].note, 'draft before source removal');
});

// ---- client/server contract, drafts, pagination and source paging --------

function listPage(items, extra = {}) {
  return { items, total: items.length, offset: 0, limit: 50, snapshot: 's1',
    counts: { all: items.length, pending: items.length, confirmed: 0, false_positive: 0, ignored: 0, needs_reevaluation: 0 }, ...extra };
}

// Every name the template binds must exist on the component, every label in
// both languages, so a renamed method or key cannot silently blank the UI.
test('index.html bindings and translated labels all resolve on the component', () => {
  const { app } = harness();
  const html = fs.readFileSync(path.join(__dirname, '../src/web/spa/index.html'), 'utf8');
  const loopVars = new Set();
  const sources = [];
  for (const [, expr] of html.matchAll(/\sx-for="([^"]+)"/g)) {
    const [vars, source] = expr.split(' in ');
    vars.replace(/[()\s]/g, '').split(',').forEach(name => loopVars.add(name));
    sources.push(source.trim());
  }
  const exprs = [...html.matchAll(/\s(x-text|x-show|x-model|:[\w-]+|@[\w.-]+)="([^"]+)"/g)];
  assert.ok(exprs.length > 100, 'the template parser found the bindings');
  const missing = [];
  for (const [, attribute, expr] of [...exprs, ...sources.map(s => [null, 'x-for', s])]) {
    const root = expr.split('.')[0];
    if (loopVars.has(root)) continue;
    if (root === 'L') {
      for (const lang of ['es', 'en']) {
        app.lang = lang;
        if (typeof app.L[expr.slice(2)] !== 'string') missing.push(`${lang}:${expr}`);
      }
      continue;
    }
    if (!(root in app)) missing.push(`${attribute}=${expr}`);
    else if (attribute.startsWith('@') && typeof app[root] !== 'function') missing.push(`${attribute}=${expr} is not a method`);
  }
  assert.deepEqual(missing, []);
});

test('both languages define the same labels, including every status label the code looks up', () => {
  const source = fs.readFileSync(path.join(__dirname, '../src/web/spa/spa.js'), 'utf8');
  const { app } = harness();
  app.lang = 'es'; const es = Object.keys(app.L).sort();
  app.lang = 'en'; const en = Object.keys(app.L).sort();
  assert.deepEqual(es, en);
  const used = new Set([...source.matchAll(/this\.t\('(\w+)'\)/g)].map(m => m[1]));
  ['pending', 'confirmed', 'false_positive', 'ignored', 'needs_reevaluation', 'restore',
    'fPending', 'fAccepted', 'fRejected', 'fInReview', 'fIgnored'].forEach(key => used.add(key));
  assert.deepEqual([...used].filter(key => !es.includes(key)), []);
});

// AUD-09: the initial load is one summary, one version and one bounded page.
test('reload reads one bounded page instead of preloading every finding', async () => {
  const { app, requests } = harness(async ({ url }) => {
    if (url.pathname === '/api/summary') return { project: 'p', snapshot_at: 1, duplicates_pending: 330 };
    if (url.pathname === '/api/version') return { token: 't1', rebuilding: false };
    return listPage(Array.from({ length: 50 }, (_, n) => group(String(n))), { total: 330 });
  });
  app.view = 'dup';
  await app.reloadAll();
  assert.deepEqual(requests.map(r => r.url.pathname), ['/api/summary', '/api/version', '/api/duplicates']);
  assert.equal(requests[2].url.searchParams.get('limit'), '50');
  assert.equal(app.rows.length, 50);
  assert.equal(app.listCount, '1–50 / 330');
  assert.equal(app.nextDisabled, false);
});

// AUD-08: a reindex between pages invalidates the pinned snapshot; the list
// restarts from a fresh first page and tells the user why.
test('a stale page restarts from the first page of the new generation and says so', async () => {
  const fresh = Array.from({ length: 50 }, (_, n) => group(`new-${n}`));
  const { app, requests } = harness(async ({ url }) => {
    if (url.searchParams.get('snapshot') === 'old') return { status: 409 };
    return listPage(fresh, { total: 350, snapshot: 'new' });
  });
  app.view = 'dup'; app.pageSnapshot = 'old'; app.pageTotal = 330; app.pageOffset = 0;
  app.dups = Array.from({ length: 50 }, (_, n) => group(`old-${n}`));
  await app.nextPage();
  assert.equal(requests[0].url.searchParams.get('offset'), '50');
  assert.equal(requests[0].url.searchParams.get('snapshot'), 'old');
  assert.equal(requests[1].url.searchParams.get('offset'), '0');
  assert.equal(requests[1].url.searchParams.get('snapshot'), '');
  assert.equal(app.pageSnapshot, 'new');
  assert.equal(app.pageOffset, 0);
  assert.ok(app.dups.every(item => item.identity.startsWith('new-')), 'no row from the old generation survives');
  assert.equal(app.toast, app.t('pageRestarted'));
});

// AUD-06: a save that is in flight must not discard later typing, even when a
// poll reload observes the saved revision before the save response arrives.
test('a poll during an in-flight file save does not leave a false conflict on the newer draft', async () => {
  const saving = deferred();
  let saved = false;
  const { app } = harness(async ({ url }) => {
    if (url.pathname === '/api/file-review') { saved = true; return saving.promise; }
    if (url.pathname === '/api/summary') return { project: 'p' };
    if (url.pathname === '/api/version') return { token: 't2', rebuilding: false };
    if (url.pathname === '/api/finding') return detail('group', [{ path: 'a.rs', note: 'first draft', status: 'accepted', revision: saved ? 2 : 1 }]);
    return listPage([group()]);
  });
  selectGroup(app, group(), [{ path: 'a.rs', note: '', status: 'accepted', revision: 1 }]);
  app.markDirty({ target: { value: 'first draft' } });
  const pending = app.saveFile();
  app.markDirty({ target: { value: 'later typing' } });
  await app.reloadAll();
  saving.resolve({ revision: 2 });
  await pending;
  assert.equal(app.fileNote, 'later typing');
  assert.equal(app.fileDirty, true);
  assert.equal(app.drafts['duplicate:group:a.rs'].expected_revision, 2);
  assert.equal(app.fileConflict, false);
});

test('typing during an in-flight finding decision save keeps the newer reason', async () => {
  const saving = deferred();
  const { app, requests } = harness(async ({ url }) => {
    if (url.pathname === '/api/review') return saving.promise;
    if (url.pathname === '/api/finding') return { ...detail(), revision: 2, state: { status: 'false_positive', reason: 'first' } };
    return listPage([group()]);
  });
  selectGroup(app);
  app.setReviewAction({ currentTarget: { dataset: { v: 'false_positive' } } });
  app.markReviewDirty({ target: { value: 'first' } });
  const pending = app.saveReview();
  app.markReviewDirty({ target: { value: 'first, and a later thought' } });
  saving.resolve({ state: { status: 'false_positive', reason: 'first' }, revision: 2 });
  await pending;
  assert.equal(requests[0].body.reason, 'first');
  assert.equal(requests[0].body.expected_revision, 1);
  assert.equal(app.reviewReason, 'first, and a later thought');
  assert.equal(app.reviewDirty, true);
  assert.equal(app.reviewConflict, false);
  assert.equal(app.findingDrafts['duplicate:group'].expected_revision, 2);
});

// AUD-07: drafts survive a poll reload and a round trip through another view.
test('unsaved drafts survive a poll reload and switching views', async () => {
  const dead = { identity: 'dead-1', fingerprint: 'fp-dead', revision: 0, name: 'entry', kind: 'function', language: 'rust',
    path: 'a.rs', line: 1, end_line: 2, parent: null, state };
  const { app } = harness(async ({ url }) => {
    if (url.pathname === '/api/summary') return { project: 'p' };
    if (url.pathname === '/api/version') return { token: 't3', rebuilding: false };
    if (url.pathname === '/api/finding') return { identity: 'dead-1', kind: 'dead', current: true, fingerprint: 'fp-dead', revision: 0, state, files: [], history: [], comments: [] };
    if (url.pathname === '/api/dead') return listPage([dead]);
    return listPage([group()]);
  });
  app.view = 'dead'; app.editor = true; await app.loadPage(true);
  app.select({ currentTarget: { dataset: { v: 'dead-1' } } });
  await app.loadDetail();
  app.setReviewAction({ currentTarget: { dataset: { v: 'false_positive' } } });
  app.markReviewDirty({ target: { value: 'called from a build script' } });
  await app.reloadAll();
  assert.equal(app.reviewReason, 'called from a build script');
  await app.go({ currentTarget: { dataset: { v: 'dup' } } });
  await app.go({ currentTarget: { dataset: { v: 'dead' } } });
  app.select({ currentTarget: { dataset: { v: 'dead-1' } } });
  await app.loadDetail();
  assert.equal(app.reviewReason, 'called from a build script');
  assert.equal(app.reviewAction, 'false_positive');
  assert.equal(app.reviewDirty, true);
  assert.equal(app.reviewConflict, false);
});

// The dead-code editor mirrors the duplicate one: its own tab, one pane with the
// symbol's lines marked and the context dimmed, and the analysis caveats.
function deadFull() {
  return { identity: 'dead-1', fingerprint: 'fp-dead', revision: 0, name: 'entry', kind: 'function', language: 'python',
    path: 'pkg/a.py', line: 3, end_line: 4, parent: null, state, line_count: 2, cautions: ['dynamic', 'unseen'],
    excerpt: ['import os', '', 'def entry():', '    return 1', '', 'x = 2'], excerpt_first: 1,
    window_first: 1, window_last: 6, excerpt_available: true, excerpt_truncated: false, source_next_offset: null };
}

// DASH-003: the list template reads the same row fields in both views, so a
// field missing on dead-code rows makes Alpine warn "row.marked is not defined".
test('list rows define every field the row template reads, in both views', () => {
  const { app } = harness();
  const html = fs.readFileSync(path.join(__dirname, '../src/web/spa/index.html'), 'utf8');
  const block = html.match(/x-for="row in rows"[\s\S]*?<\/template>/)[0];
  const fields = [...new Set([...block.matchAll(/\brow\.(\w+)/g)].map(([, name]) => name))];
  assert.ok(fields.includes('marked'), 'the template still shows the bookmark mark');
  for (const [view, item] of [['dup', group('g')], ['dead', deadFull()]]) {
    app.view = view;
    const row = app.rowOf(item);
    assert.deepEqual(fields.filter((name) => !(name in row)), [], `${view} row`);
  }
  app.view = 'dead';
  assert.equal(app.rowOf(deadFull()).marked, false);
});

test('a dead candidate opens in its own editor tab like a duplicate', () => {
  const opened = [];
  const { app, context } = harness();
  context.window = { open: (url) => { opened.push(url); return {}; }, location: {} };
  app.view = 'dead'; app.deads = [deadFull()];
  app.select({ currentTarget: { dataset: { v: 'dead-1' } } });
  assert.equal(opened.length, 1);
  const query = new URL(opened[0], 'http://test/').searchParams;
  assert.equal(query.get('view'), 'editor');
  assert.equal(query.get('kind'), 'dead');
  assert.equal(query.get('id'), 'dead-1');
  assert.equal(app.sel, '', 'the list itself keeps no selection');
});

test('the dead editor loads the source window and marks only the symbol lines', async () => {
  const { app, requests } = harness(async ({ url }) => {
    if (url.pathname === '/api/finding') return { identity: 'dead-1', kind: 'dead', current: true, fingerprint: 'fp-dead', revision: 0, state, files: [], history: [], comments: [] };
    return listPage([deadFull()]);
  });
  app.view = 'dead'; app.editor = true; app.deads = [deadFull()]; app.pageTotal = 1;
  app.openFinding('dead-1');
  assert.equal(await app.loadDetail(), true);
  const full = requests.find((r) => r.url.pathname === '/api/dead');
  assert.equal(full.url.searchParams.get('id'), 'dead-1');
  assert.equal(full.url.searchParams.get('full'), '1');
  assert.equal(app.panes.length, 1);
  const own = app.panes[0].rows.filter((r) => r.cls === 'row-dead').map((r) => r.n);
  assert.deepEqual(Array.from(own), [3, 4]);
  assert.equal(app.panes[0].rows.length, 6);
  assert.equal(app.tabs.length, 1);
  assert.equal(app.canCloseAll, false, 'one pane cannot be closed');
  assert.match(app.editorFooter, /^Dead code #1 of 1 · 2 lines · function$/);
  assert.deepEqual(Array.from(app.legend, (l) => l.key), ['legendDead', 'legendCtx']);
  assert.equal(app.deadInfo.cautions.length, 2);
  assert.match(app.deadInfo.cautions[0], /Dynamic language/);
  app.setCodeFilter({ currentTarget: { dataset: { v: 'symbol' } } });
  assert.deepEqual(Array.from(app.panes[0].rows, (r) => r.n), [3, 4]);
  app.setCodeFilter({ currentTarget: { dataset: { v: 'context' } } });
  assert.deepEqual(Array.from(app.panes[0].rows, (r) => r.n), [1, 2, 5, 6]);
  assert.match(app.promptText, /What this analysis cannot see:\n- Dynamic language/);
});

// AUD-03: the copied prompt carries effective state, reason, fingerprint and
// a scoped task, and separates per-copy working labels from the decision.
test('a re-evaluation prompt names the previous decision, its reason and the new fingerprint', () => {
  const { app } = harness();
  const item = { name: 'entry', kind: 'function', language: 'rust', path: 'a.rs', line: 3, end_line: 9, fingerprint: 'fp-new',
    state: { status: 'needs_reevaluation', previous: 'false_positive', reason: 'reflection entry point' } };
  const prompt = app.deadPrompt(item);
  assert.match(prompt, /Saved decision: Re-evaluate \(False positive\) — reflection entry point/);
  assert.match(prompt, /Source fingerprint: fp-new/);
  assert.match(prompt, /Reevaluate the earlier decision/);
  assert.match(prompt, /a\.rs:3–9/);
  assert.match(prompt, /Look for uses the index cannot see/, 'a re-evaluation still carries the investigation steps');
});

test('a duplicate prompt reports the group decision apart from per-copy working labels', () => {
  const { app } = harness();
  const item = group(); item.state = { status: 'confirmed', reason: 'same parser loop' };
  app.detail = detail('group', [{ path: 'b.rs', status: 'rejected', note: 'older copy', revision: 1 }]);
  const prompt = app.dupPrompt(item, ['A', 'B']);
  assert.match(prompt, /Saved decision: Confirmed — same parser loop/);
  assert.match(prompt, /Source fingerprint: fp-v2/);
  assert.match(prompt, /Review only this finding/);
  const labels = prompt.indexOf(app.t('pFileLabels'));
  assert.ok(labels > prompt.indexOf('Saved decision'), 'working labels follow the decision under their own heading');
  assert.ok(prompt.indexOf('B: Propose removing this copy — older copy') > labels);
  assert.match(prompt, /extract the shared logic/);
});

// AUD-04: a per-copy label is a file-review write only; it never posts a
// finding decision and never changes the group's effective state.
test('a rejected copy label saves only a file review and leaves the group decision alone', async () => {
  const { app, requests } = harness(async ({ url }) => {
    if (url.pathname === '/api/file-review') return { data: { path: 'b.rs', status: 'rejected', revision: 1 } };
    if (url.pathname === '/api/finding') return detail('group', [{ path: 'b.rs', status: 'rejected', note: '', revision: 1 }]);
    return listPage([group()]);
  });
  selectGroup(app);
  app.pickFile({ currentTarget: { dataset: { v: '1' } } });
  app.setFileStatus({ currentTarget: { dataset: { v: 'rejected' } } });
  await app.saveFile();
  assert.equal(requests[0].url.pathname, '/api/file-review');
  assert.deepEqual(Object.keys(requests[0].body).sort(), ['expected_revision', 'fingerprint', 'id', 'note', 'path', 'status']);
  assert.equal(requests[0].body.fingerprint, 'fp-v2', 'the label is tied to the code the editor showed');
  assert.ok(!requests.some(r => r.url.pathname === '/api/review'));
  assert.equal(app.selItem.state.status, 'pending');
  assert.equal(app.reviewStateText, app.t('pending'));
  assert.match(app.t('fileHint'), /do not dismiss the finding/);
  assert.equal(app.fileRows[1].chip, app.t('fRejected'));
});

// AUD-13: source chunks walk forward and back, and copying a partial view says so.
test('source paging walks back to the first chunk and copying a partial chunk is flagged', async () => {
  let copied = '';
  const { app, requests, context } = harness(async ({ url }) => {
    const offset = Number(url.searchParams.get('source_offset'));
    const item = group();
    item.members.forEach(member => Object.assign(member, {
      excerpt: [`line ${offset + 1}`], excerpt_first: 1 + offset, excerpt_truncated: true,
      source_next_offset: offset === 0 ? 400 : null,
    }));
    return listPage([item]);
  });
  selectGroup(app);
  await app.loadFullCode('group');
  await app.nextSource();
  assert.equal(app.sourceNextDisabled, true);
  await app.previousSource();
  assert.equal(requests[2].url.searchParams.get('source_offset'), '0');
  assert.equal(app.panes[0].rows[0].n, 1);
  assert.equal(app.sourcePreviousDisabled, true);
  context.navigator.clipboard = { writeText: async (text) => { copied = text; } };
  await app.copyCode();
  assert.equal(copied, 'line 1');
  assert.equal(app.toast, app.t('copiedPartial'));
});

test('an unreadable member is reported instead of shown as complete code', async () => {
  const { app } = harness(async () => {
    const item = group();
    Object.assign(item.members[1], { excerpt: [], excerpt_available: false, excerpt_truncated: true, source_next_offset: null });
    return listPage([item]);
  });
  selectGroup(app);
  assert.equal(await app.loadFullCode('group'), false);
  assert.equal(app.sourceUnavailable, true);
  assert.equal(app.sourcePartial, true);
});

test('orphan rows show their saved status and date', async () => {
  const row = { identity: 'a'.repeat(64), kind: 'dead', status: 'ignored', revision: 2, reason: 'kept', at: 1, current: false, state: { status: 'ignored' } };
  const { app } = harness(async () => ({ data: listPage([row]) }));
  app.view = 'orphans'; await app.loadPage(true);
  assert.match(app.orphans[0].label, /Ignored/);
  assert.ok(app.orphans[0].when.length > 0);
});

// Saving a label advances the group's decision revision (store contract).
// A pending decision draft follows that bump only when this save caused it.
test('an own label save keeps a pending decision draft current without hiding foreign writes', async () => {
  for (const [findingRevision, conflict] of [[2, false], [3, true]]) {
    const { app, requests } = harness(async ({ url }) => {
      if (url.pathname === '/api/file-review') return { data: { path: 'a.rs', status: 'accepted', revision: 1, finding_revision: findingRevision, scope: 'file_label', finding_state: state } };
      if (url.pathname === '/api/finding') return { ...detail('group', [{ path: 'a.rs', status: 'accepted', note: '', revision: 1, scope: 'file_label' }]), revision: findingRevision };
      if (url.pathname === '/api/review') return { status: 409 };
      return listPage([group()]);
    });
    selectGroup(app);
    app.setReviewAction({ currentTarget: { dataset: { v: 'confirmed' } } });
    app.markReviewDirty({ target: { value: 'same loop' } });
    app.setFileStatus({ currentTarget: { dataset: { v: 'accepted' } } });
    await app.saveFile();
    assert.equal(app.reviewConflict, conflict, `finding_revision ${findingRevision}`);
    assert.equal(app.reviewReason, 'same loop');
    await app.saveReview();
    const sent = requests.find(r => r.url.pathname === '/api/review');
    assert.equal(sent.body.expected_revision, conflict ? 1 : 2);
  }
});

test('a label drafted on code that has since changed conflicts and keeps its fingerprint', async () => {
  const changed = { ...detail(), fingerprint: 'fp-v3', revision: 1 };
  const { app, requests } = harness(async ({ url }) => {
    if (url.pathname === '/api/finding') return changed;
    if (url.pathname === '/api/file-review') return { status: 409 };
    const item = group(); item.fingerprint = 'fp-v3'; return listPage([item]);
  });
  selectGroup(app);
  app.markDirty({ target: { value: 'checked the old body' } });
  await app.loadDetail();
  assert.equal(app.fileConflict, true);
  assert.equal(app.fileNote, 'checked the old body');
  await app.saveFile();
  const sent = requests.find(r => r.url.pathname === '/api/file-review');
  assert.equal(sent.body.fingerprint, 'fp-v2');
  await app.reviewLatest();
  assert.equal(app.drafts['duplicate:group:a.rs'].fingerprint, 'fp-v3');
  assert.equal(app.fileConflict, false);
});

test('labels on an ambiguous group are sent without a fingerprint', async () => {
  const { app, requests } = harness(async ({ url }) => url.pathname === '/api/file-review'
    ? { data: { revision: 1, finding_revision: 1 } } : { ...detail(), fingerprint: '' });
  const item = group(); item.fingerprint = '';
  app.view = 'dup'; app.editor = true; app.sel = item.identity; app.dups = [item]; app.selItem = item;
  app.detail = { ...detail(), fingerprint: '' }; app.syncFile(); app.syncReview(); app.rebuildDetail();
  app.markDirty({ target: { value: 'note' } });
  await app.saveFile();
  assert.equal('fingerprint' in requests[0].body, false);
});

test('a copy conflict can be reconciled explicitly without discarding its note', async () => {
  let saving = 0;
  const { app, requests } = harness(async ({ url }) => {
    if (url.pathname === '/api/file-review') {
      saving += 1;
      return saving === 1 ? { status: 409 } : { revision: 3, finding_revision: 3 };
    }
    if (url.pathname === '/api/finding') return detail('group', [{ path: 'a.rs', status: 'accepted', note: 'other tab', revision: saving > 1 ? 3 : 2 }]);
    return listPage([group()]);
  });
  selectGroup(app, group(), [{ path: 'a.rs', status: 'accepted', note: '', revision: 1 }]);
  app.markDirty({ target: { value: 'my retained note' } });
  await app.saveFile();
  assert.equal(app.fileConflict, true);
  assert.equal(app.fileNote, 'my retained note');
  await app.loadDetail();
  assert.equal(app.drafts['duplicate:group:a.rs'].expected_revision, 1, 'refresh does not acknowledge a foreign revision');
  await app.reviewLatest();
  assert.equal(app.fileNote, 'my retained note');
  assert.equal(app.fileDirty, true);
  assert.equal(app.fileConflict, false);
  assert.equal(app.drafts['duplicate:group:a.rs'].expected_revision, 2);
  assert.equal(requests.filter(request => request.body).length, 1, 'acknowledging the latest version never saves');
  await app.saveFile();
  const writes = requests.filter(request => request.body);
  assert.equal(writes[1].body.expected_revision, 2);
  assert.equal(writes[1].body.note, 'my retained note');
  const html = fs.readFileSync(path.join(__dirname, '../src/web/spa/index.html'), 'utf8');
  const copyForm = html.slice(html.indexOf('@submit.prevent="saveFile"'), html.indexOf('<h3 class="font-semibold" x-text="L.allFiles"'));
  assert.match(copyForm, /@click="reviewLatest" x-show="fileConflict"/);
  assert.match(copyForm, /x-text="L.fileHint"/);
  assert.match(copyForm, /x-text="L.draftHint"/);
});

test('comment conflict preserves text and token across refresh, collapse and finding navigation', async () => {
  const { app, requests } = harness(async () => ({ status: 409 }));
  selectGroup(app);
  app.detail.comments = [{ id: 7, body: 'saved' }]; app.syncComments();
  app.editComment({ currentTarget: { dataset: { v: '7' } } });
  app.markCommentDirty({ target: { value: 'tab B draft' } });
  await app.saveComment();
  assert.equal(requests[0].body.expected_revision, 1);
  assert.equal(requests[0].body.kind, 'duplicate');
  assert.equal(requests[0].body.id, 'group');
  assert.equal(app.commentConflict, true);
  app.detail.revision = 2; app.detail.comments[0].body = 'tab A saved'; app.syncComments();
  const writes = requests.length;
  app.closeAll(); app.openAll();
  assert.equal(requests.length, writes);
  assert.equal(app.commentBody, 'tab B draft');
  assert.equal(app.commentDrafts[app.commentKey].expected_revision, 1);
  app.openFinding('other'); app.openFinding('group');
  app.detail = detail(); app.detail.comments = [{ id: 7, body: 'tab A saved' }]; app.detail.revision = 2;
  app.editComment({ currentTarget: { dataset: { v: '7' } } });
  assert.equal(app.commentBody, 'tab B draft'); assert.equal(app.commentConflict, true);
});

test('stale comment deletion preserves its draft and deleted comment is unavailable', async () => {
  let status = 409;
  const { app, requests } = harness(async () => ({ status }));
  selectGroup(app); app.detail.comments = [{ id: 7, body: 'saved' }]; app.syncComments();
  app.editComment({ currentTarget: { dataset: { v: '7' } } });
  app.markCommentDirty({ target: { value: 'retain before deleting' } });
  await app.deleteComment();
  assert.equal(requests[0].url.pathname, '/api/comment/delete');
  assert.equal(requests[0].body.expected_revision, 1); assert.equal('body' in requests[0].body, false);
  assert.equal(app.commentBody, 'retain before deleting'); assert.equal(app.commentConflict, true);
  status = 404; await app.saveComment();
  assert.equal(app.commentUnavailable, true); assert.equal(app.commentSaveDisabled, true);
  const before = requests.length; await app.saveComment(); assert.equal(requests.length, before);
  app.detail.comments = []; app.syncComments();
  assert.equal(app.commentBody, 'retain before deleting'); assert.equal(app.commentUnavailable, true);
});

test('typing during comment edit keeps newer draft with own committed revision', async () => {
  const saving = deferred();
  const { app, requests } = harness(async ({ url }) => {
    if (url.pathname === '/api/comment/update') return saving.promise;
    if (url.pathname === '/api/finding') return { ...detail(), revision: 2, comments: [{ id: 7, body: 'sent' }] };
    return { items: [group()], snapshot: 's2' };
  });
  selectGroup(app); app.detail.comments = [{ id: 7, body: 'saved' }]; app.syncComments();
  app.editComment({ currentTarget: { dataset: { v: '7' } } });
  app.markCommentDirty({ target: { value: 'sent' } });
  const pending = app.saveComment(); await app.saveComment(); // duplicate submission is blocked
  app.markCommentDirty({ target: { value: 'later typing' } });
  saving.resolve({ revision: 2 }); await pending;
  assert.equal(requests.filter(r => r.body).length, 1);
  assert.equal(app.commentBody, 'later typing'); assert.equal(app.commentDirty, true);
  assert.equal(app.commentDrafts[app.commentKey].expected_revision, 2);
  assert.equal(app.commentConflict, false);
});

test('new comment typed during append stays a separate new draft', async () => {
  const saving = deferred();
  const { app } = harness(async ({ url }) => url.pathname === '/api/comment' ? saving.promise
    : url.pathname === '/api/finding' ? { ...detail(), revision: 2, comments: [{ id: 8, body: 'sent' }] } : { items: [group()] });
  selectGroup(app); app.syncComments();
  app.markCommentDirty({ target: { value: 'sent' } }); const pending = app.saveComment();
  app.markCommentDirty({ target: { value: 'another note' } });
  saving.resolve({ comment_id: 8 }); await pending;
  assert.equal(app.commentId, null); assert.equal(app.commentBody, 'another note'); assert.equal(app.commentDirty, true);
});

test('explicit comment reconciliation loads latest but never recreates deleted comment', async () => {
  const { app } = harness(); selectGroup(app);
  app.detail.comments = [{ id: 7, body: 'saved' }]; app.syncComments();
  app.editComment({ currentTarget: { dataset: { v: '7' } } });
  app.markCommentDirty({ target: { value: 'draft' } });
  app.reloadAll = async () => { app.detail.revision = 5; app.detail.comments[0].body = 'newer'; app.syncComments(); };
  await app.commentLatest();
  assert.equal(app.commentBody, 'draft'); assert.equal(app.commentDrafts[app.commentKey].expected_revision, 5);
  assert.equal(app.commentConflict, false);
  app.reloadAll = async () => { app.detail.revision = 6; app.detail.comments = []; app.syncComments(); };
  await app.commentLatest(); assert.equal(app.commentUnavailable, true);
  assert.equal(app.commentDrafts[app.commentKey].expected_revision, 5);
});

test('decision diagnostics distinguish saved false positive from ineffective unavailable source', () => {
  const { app } = harness(); selectGroup(app);
  app.detail.saved_decision = { status: 'false_positive' };
  app.detail.effective_decision = null; app.detail.review_validity = 'unavailable'; app.syncComments();
  assert.equal(app.decisionRows[0].value, app.t('false_positive'));
  assert.equal(app.decisionRows[1].value, app.t('noDecision'));
  assert.equal(app.validityText, app.t('validity_unavailable'));
});
