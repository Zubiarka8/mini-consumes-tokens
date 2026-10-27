'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function editor() {
  let factory;
  const requests = [];
  const context = {
    document: { addEventListener: (_, fn) => fn(), hidden: false, querySelector: () => ({content: 'csrf'}) },
    Alpine: {data: (_, fn) => { factory = fn; }}, navigator: {language: 'en'},
    localStorage: {getItem: () => null, setItem: () => {}}, URLSearchParams,
    setTimeout: () => 1, clearTimeout: () => {},
    fetch: async (url, options) => {
      requests.push({url, body: options?.body && JSON.parse(options.body)});
      return {ok: true, json: async () => ({revision: 5, finding_revision: 2})};
    },
  };
  vm.createContext(context);
  vm.runInContext(fs.readFileSync(path.join(__dirname, '../src/web/spa/spa.js'), 'utf8'), context);
  const app = factory();
  const members = ['a', 'b'].map(member_key => ({member_key, path: 'same.rs', symbol: member_key,
    start_line: 1, end_line: 3, line_count: 3, excerpt: ['fn f() {', '1', '}'], excerpt_first: 1}));
  app.view = 'dup'; app.editor = true; app.sel = 'group';
  app.selItem = {identity: 'group', fingerprint: 'fp', state: {status: 'pending'}, members, language: 'rust', line_count: 3};
  app.dups = [app.selItem];
  app.detail = {identity: 'group', fingerprint: 'fp', state: {status: 'pending'}, revision: 1,
    members: [{member_key: 'a', revision: 4}, {member_key: 'b', revision: 7}],
    files: [{member_key: 'a', path: 'same.rs', status: 'accepted', note: 'keep a', revision: 4, code_changed: true},
      {member_key: 'b', path: 'same.rs', status: 'rejected', note: 'remove b', revision: 7}], history: [], comments: []};
  app.syncFile(); app.syncReview(); app.rebuildDetail();
  return {app, requests};
}
const pick = (app, idx) => app.pickFile({currentTarget: {dataset: {v: String(idx)}}});

test('same-path copies keep separate saved labels and unsaved drafts', () => {
  const {app} = editor();
  assert.equal(app.fileNote, 'keep a');
  app.markDirty({target: {value: 'draft a'}});
  pick(app, 1); assert.equal(app.fileStatus, 'rejected'); assert.equal(app.fileNote, 'remove b');
  app.markDirty({target: {value: 'draft b'}});
  pick(app, 0); assert.equal(app.fileNote, 'draft a'); assert.equal(app.fileRevision, 4);
  pick(app, 1); assert.equal(app.fileNote, 'draft b'); assert.equal(app.fileRevision, 7);
  assert.equal(Object.keys(app.drafts).length, 2);
});

test('warning follows the changed copy into file lists, active panel and AI prompt', () => {
  const {app} = editor();
  assert.equal(app.fileRows[0].warning, 'Code changed, review needed');
  assert.equal(app.fileRows[1].warning, '');
  assert.equal(app.activeFile.warning, 'Code changed, review needed');
  assert.match(app.promptText, /A: Propose keeping this copy — keep a — Code changed, review needed/);
  assert.match(app.promptText, /B: Propose removing this copy — remove b/);
  pick(app, 1); assert.equal(app.activeFile.warning, '');
  const html = fs.readFileSync(path.join(__dirname, '../src/web/spa/index.html'), 'utf8');
  assert.equal((html.match(/x-text="f.warning"/g) || []).length, 2);
  assert.match(html, /x-text="activeFile.warning"/);
});

test('copy saves send the opaque key and cleared copies retain their revision', async () => {
  const {app, requests} = editor();
  app.detail.files = []; app.syncFile();
  assert.equal(app.fileStatus, 'pending'); assert.equal(app.fileRevision, 4);
  app.loadDetail = async () => {};
  app.setFileStatus({currentTarget: {dataset: {v: 'accepted'}}});
  await app.saveFile();
  assert.equal(requests[0].body.member_key, 'a');
  assert.equal(requests[0].body.path, 'same.rs');
  assert.equal(requests[0].body.expected_revision, 4);
  assert.ok(!Object.hasOwn(requests[0].body, 'code_hash'));
  assert.equal(app.selItem.state.status, 'pending');
});

test('bulk controls sit together beside search outside the collapsible editor', () => {
  const html = fs.readFileSync(path.join(__dirname, '../src/web/spa/index.html'), 'utf8');
  const header = html.slice(html.indexOf('<header'), html.indexOf('</header>'));
  assert.match(header, /@input="headerSearch"[\s\S]*?<\/label>\s*<div[^>]*x-show="showDupEditor"[^>]*role="group"/);
  assert.match(header, /@click="openAll"[^>]*x-text="L.openAll"/);
  assert.match(header, /@click="closeAll"[^>]*x-text="L.closeAll"/);
  const {app} = editor();
  app.lang = 'es';
  assert.equal(app.L.openAll, 'Abrir todos');
  assert.equal(app.L.closeAll, 'Cerrar todos');
});

test('closing and reopening all complete code panes preserves review state and drafts', () => {
  const {app, requests} = editor();
  app.markDirty({target: {value: 'unsaved copy note'}});
  app.markReviewDirty({target: {value: 'unsaved group reason'}});
  const before = JSON.stringify({detail: app.detail, drafts: app.drafts, findingDrafts: app.findingDrafts});
  for (let repeat = 0; repeat < 2; repeat += 1) app.closeAll();
  assert.equal(app.panes.length, 0);
  assert.equal(app.noneOpen, true);
  assert.equal(app.showDupEditor, true, 'top controls remain visible with every pane closed');
  assert.ok(app.fileRows.every(row => row.shut));
  for (let repeat = 0; repeat < 2; repeat += 1) app.openAll();
  assert.deepEqual(Array.from(app.panes, pane => pane.idx), [0, 1]);
  assert.ok(app.fileRows.every(row => !row.shut));
  assert.equal(app.fileNote, 'unsaved copy note');
  assert.equal(app.reviewReason, 'unsaved group reason');
  assert.equal(JSON.stringify({detail: app.detail, drafts: app.drafts, findingDrafts: app.findingDrafts}), before);
  assert.equal(requests.length, 0, 'presentation actions never save or delete data');
  app.closeFile({currentTarget: {dataset: {v: '1'}}});
  assert.equal(app.panes.length, 1);
  pick(app, 1);
  assert.equal(app.panes.length, 2, 'individual reopen still works after bulk actions');
});
