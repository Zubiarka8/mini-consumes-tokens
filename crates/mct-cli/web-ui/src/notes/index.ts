import { Lexer, Marked } from "marked";
import markedFootnote from "marked-footnote";
import { tokensToDom, type NoteContext } from "./dom";
import { obsidianExtension, stripComments } from "./obsidian";

// Entry of `notes.js`, the Markdown renderer for the dashboard's notes page
// (see web-ui/README.md and ADR-033). It lexes with `marked` and builds DOM
// nodes directly: raw HTML in a note is shown as text and no HTML string is
// ever produced or assigned to the page.

export type { NoteContext } from "./dom";

// marked-footnote keeps per-document state in a closure and resets it in
// `walkTokens`, so every note gets its own instance: a note that fails to
// lex cannot leave state behind for the next one. Order matters: Obsidian's
// `![[...]]` and `[[...]]` must win over image and link syntax.
function createMarked(): Marked {
  const marked = new Marked({ gfm: true, breaks: false });
  marked.use(obsidianExtension);
  marked.use(markedFootnote());
  return marked;
}

/** Renders an Obsidian-flavoured Markdown note into a `div.note-body`. */
export function renderNote(
  source: string,
  ctx: NoteContext = {},
  doc: Document = document,
): HTMLElement {
  const marked = createMarked();
  const options = marked.defaults;
  const text = stripComments(source.replace(/\r\n?/g, "\n"));
  // The same steps, in the same order, as `marked.parse` minus the HTML renderer.
  const lexed = Lexer.lex(text, options);
  const tokens = options.hooks ? options.hooks.processAllTokens(lexed) : lexed;
  if (options.walkTokens) marked.walkTokens(tokens, options.walkTokens);
  return tokensToDom(doc, tokens, ctx);
}
