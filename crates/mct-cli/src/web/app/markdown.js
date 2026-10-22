// Small, safe Markdown renderer. It builds DOM nodes only: raw HTML in notes is shown as text.
import { h } from "./core.js";
import { codeBlock } from "./code.js";

const SAFE_SCHEMES = /^(https?:|mailto:)/i;

/** Inline: `code`, **bold**, *italic*, [text](url), [[wiki]], #tag. */
function inline(text, ctx) {
  const out = [];
  const pattern = /(`[^`]+`)|(\*\*[^*]+\*\*)|(\*[^*\s][^*]*\*)|(\[\[[^\]]+\]\])|(\[[^\]]*\]\([^)]*\))|((?:^|(?<=\s))#[\p{L}\p{N}_/-]+)/gu;
  let last = 0, match;
  while ((match = pattern.exec(text))) {
    if (match.index > last) out.push(text.slice(last, match.index));
    const token = match[0];
    if (match[1]) out.push(h("code", {}, token.slice(1, -1)));
    else if (match[2]) out.push(h("strong", {}, inline(token.slice(2, -2), ctx)));
    else if (match[3]) out.push(h("em", {}, inline(token.slice(1, -1), ctx)));
    else if (match[4]) {
      const name = token.slice(2, -2).split("|")[0].split("#")[0].trim();
      out.push(h("a", { href: "#", onclick: (event) => { event.preventDefault(); ctx.onNote?.(name, true); } }, token.slice(2, -2).split("|").pop()));
    } else if (match[5]) {
      const [, label, url] = /^\[([^\]]*)\]\(([^)]*)\)$/.exec(token);
      if (SAFE_SCHEMES.test(url)) out.push(h("a", { href: url, target: "_blank", rel: "noopener noreferrer nofollow" }, label || url));
      else if (/\.md(#.*)?$/.test(url) && !/^[a-z]+:/i.test(url)) out.push(h("a", { href: "#", onclick: (event) => { event.preventDefault(); ctx.onNote?.(url, false); } }, label || url));
      else out.push(label || url);
    } else out.push(h("button", { class: "tag", onclick: () => ctx.onTag?.(token.slice(1)) }, token));
    last = pattern.lastIndex;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

const isTableRow = (line) => /^\s*\|.*\|\s*$/.test(line);
const cells = (line) => line.trim().replace(/^\||\|$/g, "").split("|").map((cell) => cell.trim());

/** ctx: { onNote(target, isWikiName), onTag(tag) } */
export function renderMarkdown(source, ctx = {}) {
  const root = h("div", { class: "note-body" });
  const lines = source.replace(/\r\n/g, "\n").split("\n");
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (!line.trim()) { i += 1; continue; }
    const fence = /^```(\w*)/.exec(line);
    if (fence) {
      const body = [];
      i += 1;
      while (i < lines.length && !lines[i].startsWith("```")) body.push(lines[i++]);
      i += 1;
      const block = codeBlock({ lines: body, language: fence[1], label: fence[1] || "code" });
      root.append(h("pre", { style: { padding: 0, border: 0, background: "none" } }, block));
      continue;
    }
    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    if (heading) { root.append(h(`h${Math.min(6, heading[1].length)}`, {}, inline(heading[2], ctx))); i += 1; continue; }
    if (/^---+\s*$/.test(line)) { root.append(h("hr")); i += 1; continue; }
    if (isTableRow(line) && i + 1 < lines.length && /^\s*\|?[\s:|-]+\|[\s:|-]*$/.test(lines[i + 1])) {
      const head = cells(line);
      i += 2;
      const rows = [];
      while (i < lines.length && isTableRow(lines[i])) rows.push(cells(lines[i++]));
      root.append(h("div", { style: { overflowX: "auto" } }, h("table", {}, h("thead", {}, h("tr", {}, head.map((c) => h("th", {}, inline(c, ctx))))),
        h("tbody", {}, rows.map((row) => h("tr", {}, row.map((c) => h("td", {}, inline(c, ctx)))))))));
      continue;
    }
    if (/^\s*>/.test(line)) {
      const body = [];
      while (i < lines.length && /^\s*>/.test(lines[i])) body.push(lines[i++].replace(/^\s*>\s?/, ""));
      root.append(h("blockquote", {}, inline(body.join(" "), ctx)));
      continue;
    }
    const list = /^(\s*)([-*+]|\d+\.)\s+(.*)$/.exec(line);
    if (list) {
      const ordered = /\d/.test(list[2]);
      const items = [];
      while (i < lines.length) {
        const item = /^(\s*)([-*+]|\d+\.)\s+(.*)$/.exec(lines[i]);
        if (!item) break;
        items.push(h("li", { style: { marginLeft: `${Math.min(4, Math.floor(item[1].length / 2)) * 14}px` } }, inline(item[3], ctx)));
        i += 1;
      }
      root.append(h(ordered ? "ol" : "ul", {}, items));
      continue;
    }
    const paragraph = [lines[i++]];
    while (i < lines.length && lines[i].trim() && !/^(#{1,6}\s|```|\s*>|\s*([-*+]|\d+\.)\s)/.test(lines[i])) paragraph.push(lines[i++]);
    root.append(h("p", {}, inline(paragraph.join(" "), ctx)));
  }
  return root;
}
