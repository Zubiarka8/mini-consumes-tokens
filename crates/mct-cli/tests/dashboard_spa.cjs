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
  return { app: factory(), requests };
}

const state = { status: 'pending' };
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
