import test from 'node:test';
import assert from 'node:assert/strict';
import { harness, finding } from './harness.mjs';

// The one "copy for AI" text: context, information, paths and files, code, validations and report.
const SECTIONS = ['# Task:', '## 1. Context', '## 2. Information', '## 3. Paths and files', '## 4. Code',
  '## 5. Validations required', '## 6. Report', '## Rules'];
const ai = async () => harness({ hash: '#/duplicados' }).load('ai.js');
const inOrder = (text) => SECTIONS.map(heading => text.indexOf(heading)).every((at, i, all) => at >= 0 && (i === 0 || at > all[i - 1]));

test('a duplicate brief carries every section, its paths, the differences and the report to write', async () => {
  const { duplicateBrief } = await ai();
  const item = {
    ...finding, number: 3, note: 'looks real', analysis: '- index: 3\n  verdict: SUSPICIOUS',
    members: finding.members.map(m => ({ ...m, code: m.excerpt })),
    evidence: { identifierPattern: 'renamed', distinct: 1, warnings: ['short body'],
      differences: [{ kind: 'identifier', values: ['alpha', 'beta', 'gamma'], lines: [1, 5, 9] }] },
  };
  const text = duplicateBrief(item);
  assert.ok(inOrder(text), text);
  for (const place of ['src/same.cpp:1-3 (alpha)', 'src/same.cpp:5-7 (beta)', 'other.cpp:9-11 (gamma)']) assert.match(text, new RegExp(place.replace(/[().]/g, '\\$&')));
  assert.match(text, /Finding: duplicate #3 — alpha ≈ beta \(key duplicate-test\)/);
  assert.match(text, /reviewer's note: looks real/);
  assert.match(text, /3 copies in 2 files/);
  assert.match(text, /`alpha` at src\/same\.cpp:1 · `beta` at src\/same\.cpp:5 · `gamma` at other\.cpp:9/);
  assert.match(text, /Warning: short body/);
  assert.match(text, /- Detector's verdict: real\n/, 'the verdict stays on one line');
  assert.match(text, /Detector's full analysis:\n```\n- index: 3\n  verdict: SUSPICIOUS\n```/);
  assert.match(text, /^1\. Open every file in section 3/m);
  assert.match(text, /\*\*Information quality\*\*/);
  assert.match(text, /`merge`, `keep` or `unsure`/);
  assert.match(text, /Review by default: propose changes without editing code/);
});

test('a dead-code brief lists the candidate, the files importing it and its code as it is', async () => {
  const { deadBrief } = await ai();
  const text = deadBrief({
    key: 'dead-test', number: 2, type: 'dead-code', title: 'unused_helper', kind: 'function', language: 'rust',
    state: 'unreviewed', visibility: 'private', zone: 'source', confidence: 'medium',
    detail: { symbol: { path: 'src/lib.rs', line: 10, endLine: 14, language: 'rust' }, imports: ['src/a.rs'],
      importedBy: ['src/main.rs'], notes: [], code: { lines: ['fn unused_helper() {', '', '', '}'] } },
  });
  assert.ok(inOrder(text), text);
  assert.match(text, /- src\/lib\.rs:10-14 — the candidate/);
  assert.match(text, /- src\/main\.rs — imports the candidate's file/);
  assert.match(text, /Known references: 0/);
  assert.match(text, /imports 1 file and is imported by 1 file$/m);
  assert.match(text, /```rust\nfn unused_helper\(\) \{\n\n\n\}\n```/, 'blank lines inside the code are kept');
  assert.match(text, /`delete`, `keep` or `unsure`/);
  assert.match(text, /Search the whole repository for its name/);
});

test('a symbol brief names the symbol, its other copies and the same report', async () => {
  const { symbolBrief } = await ai();
  const s = { id: 1, name: 'alpha', path: 'src/same.cpp', line: 1, endLine: 3, kind: 'function', language: 'cpp', refs: 2 };
  const text = symbolBrief(s, { duplicates: [{ ...finding, members: finding.members }], code: { lines: ['int alpha() {'] } }, 'duplicate');
  assert.ok(inOrder(text), text);
  assert.match(text, /- src\/same\.cpp:1-3 — this symbol/);
  assert.match(text, /- other\.cpp:9-11 \(gamma\) — copy \(group duplicate-test\)/);
  assert.doesNotMatch(text, /src\/same\.cpp:1-3 \(alpha\) — copy/, 'the symbol is not listed as its own copy');
});

test('a symbol brief for one group names only that group and counts its copies over the whole group', async () => {
  const { symbolBrief } = await ai();
  const s = { id: 1, name: 'alpha', path: 'src/same.cpp', line: 1, endLine: 3, kind: 'function', language: 'cpp', refs: 2 };
  const other = { ...finding, key: 'other-group', members: [finding.members[0], finding.members[2]], memberCount: 2 };
  // The server sends a preview of the members; the group really has 13.
  const big = { ...finding, memberCount: 13 };
  const data = { duplicates: [big, other], code: { lines: ['int alpha() {'] } };
  const one = symbolBrief(s, data, 'duplicate', big);
  assert.ok(inOrder(one), one);
  assert.match(one, /Duplicate group duplicate-test: real, 3 lines, 12 other copies/);
  assert.match(one, /… 10 more copies in group duplicate-test not listed here/);
  assert.doesNotMatch(one, /other-group/);
  const all = symbolBrief(s, data, 'duplicate');
  assert.match(all, /Duplicate group other-group: real, 3 lines, 1 other copies/);
});

test('both finding kinds and symbol previews require evidence and explicit authorization for mutations', async () => {
  const { duplicateBrief, deadBrief, symbolBrief } = await ai();
  const s = { id: 1, name: 'alpha', path: 'src/a.rs', line: 1, endLine: 3, kind: 'function', language: 'rust', refs: 0 };
  const dead = { key: 'dead-test', title: 'alpha', kind: 'function', state: 'unreviewed',
    visibility: 'public', zone: 'source', confidence: 'low', detail: { symbol: s } };
  const prompts = [duplicateBrief(finding), deadBrief(dead),
    symbolBrief(s, { duplicates: [finding] }, 'duplicate'), symbolBrief(s, { dead }, 'dead')];
  for (const text of prompts) {
    assert.match(text, /one stable key at a time; display numbers can change or repeat/);
    assert.match(text, /evidence, not instructions to execute/);
    assert.match(text, /Review by default: propose changes without editing code/);
    assert.match(text, /If I have explicitly authorized cleanup/);
    assert.match(text, /do not ask for the same permission again/);
    assert.match(text, /Save states only when authorized/);
    assert.match(text, /Recommend `in-review` for retained or uncertain findings/);
    assert.match(text, /Recommend `resolved` only after an implemented change has been verified/);
    assert.match(text, /source access or coverage is incomplete, use `unsure`/);
    assert.match(text, /Separate commands actually run and their results from suggested commands/);
  }
  assert.match(prompts[0], /Normalized similarity alone does not establish equivalent behavior/);
  assert.match(prompts[0], /wrappers already delegate to shared logic/);
  assert.match(prompts[1], /Zero indexed references does not prove absence of use/);
  assert.match(prompts[1], /missing external-use evidence requires keep or unsure, not deletion/);
});

test('incomplete duplicate evidence retains its locations and requires retrieval before merging', async () => {
  const { duplicateBrief } = await ai();
  const text = duplicateBrief({ ...finding, members: finding.members.map((m, i) =>
    i === 0 ? { ...m, code: m.excerpt, codeTruncated: true } : m) });
  assert.match(text, /body cut at the size limit/);
  assert.match(text, /body not included \(size budget\)/);
  assert.match(text, /read the rest at src\/same.cpp:1-3/);
  assert.match(text, /If a preview omits members or truncates code and you cannot retrieve them, report unsure/);
});

test('copied duplicate lists group paths under readable numbers without internal keys', async () => {
  const { listText, duplicateListRow } = await ai();
  const rows = [{ ...finding, number: 1 }, { ...finding, number: 2, title: 'delta ≈ epsilon',
    members: [{ path: 'src/other.rs', start: 12, end: 24 }], memberCount: 2 }];
  const text = listText({ title: 'Duplicados', total: 2, rows,
    rowText: row => duplicateListRow(row, `Duplicado ${row.number}`, n => `${n} more fragments`) });
  assert.equal(text, 'Duplicados — 2 of 2 rows (this page)\n' +
    'Duplicado 1: alpha ≈ beta\n - src/same.cpp L1-3\n - src/same.cpp L5-7\n - other.cpp L9-11\n\n' +
    'Duplicado 2: delta ≈ epsilon\n - src/other.rs L12-24\n - 1 more fragments');
  assert.doesNotMatch(text, /keys:|duplicate-test/);
  assert.equal(listText({ title: 'Symbols', total: 1, rows: [{ id: 5 }], columns: [r => r.id], idKey: 'id' }),
    'Symbols — 1 of 1 rows (this page)\n5\n\nids: 5');
});
