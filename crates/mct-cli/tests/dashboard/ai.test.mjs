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
  assert.match(text, /Do not edit anything/);
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
