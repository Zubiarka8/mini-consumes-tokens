import type { Element, ElementContent, Root } from "hast";
import { createLowlight } from "lowlight";
import bash from "highlight.js/lib/languages/bash";
import c from "highlight.js/lib/languages/c";
import cpp from "highlight.js/lib/languages/cpp";
import css from "highlight.js/lib/languages/css";
import diff from "highlight.js/lib/languages/diff";
import go from "highlight.js/lib/languages/go";
import ini from "highlight.js/lib/languages/ini";
import java from "highlight.js/lib/languages/java";
import javascript from "highlight.js/lib/languages/javascript";
import json from "highlight.js/lib/languages/json";
import markdown from "highlight.js/lib/languages/markdown";
import python from "highlight.js/lib/languages/python";
import rust from "highlight.js/lib/languages/rust";
import sql from "highlight.js/lib/languages/sql";
import typescript from "highlight.js/lib/languages/typescript";
import xml from "highlight.js/lib/languages/xml";
import yaml from "highlight.js/lib/languages/yaml";

// A fixed set keeps the bundle small. Aliases (js, ts, sh, html, toml, yml,
// rs, py, ...) come from highlight.js itself.
const lowlight = createLowlight({
  bash,
  c,
  cpp,
  css,
  diff,
  go,
  ini,
  java,
  javascript,
  json,
  markdown,
  python,
  rust,
  sql,
  typescript,
  xml,
  yaml,
});

function toDom(doc: Document, node: Root | ElementContent): Node {
  if (node.type === "text") return doc.createTextNode(node.value);
  const holder: Node =
    node.type === "root"
      ? doc.createDocumentFragment()
      : doc.createElement((node as Element).tagName);
  if (node.type === "element") {
    const classes = node.properties?.className;
    if (Array.isArray(classes))
      (holder as HTMLElement).className = classes.join(" ");
  }
  if ("children" in node)
    for (const child of node.children)
      holder.appendChild(toDom(doc, child as ElementContent));
  return holder;
}

/** The language name when highlight.js knows it, otherwise `undefined`. */
export function knownLanguage(name: string): string | undefined {
  const lower = name.toLowerCase();
  return lowlight.registered(lower) ? lower : undefined;
}

/**
 * Highlights `code` as DOM nodes built straight from lowlight's syntax tree,
 * never from an HTML string. Unknown languages come back as plain text.
 */
export function highlightToDom(
  doc: Document,
  code: string,
  language: string,
): Node {
  const known = knownLanguage(language);
  if (!known) return doc.createTextNode(code);
  return toDom(doc, lowlight.highlight(known, code));
}
