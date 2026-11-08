// Loads the notes renderer on first use. notes.js is built from crates/mct-cli/web-ui/src/notes
// (marked + highlight.js, see ADR-033) and committed in src/web/notes-dist/; like review.js it is
// only fetched when the page needs it. It builds DOM nodes only: raw HTML in notes is shown as text.
import { h } from "./core.js";

let pending;

// Last resort when the bundle cannot load: the note's text, unformatted but readable.
const plainText = (source) =>
  h("div", { class: "note-body" }, h("pre", { class: "md-code note-code" }, source));

/**
 * Resolves to `render(source, { onNote, onTag, preview }) -> HTMLElement`.
 * A failed load is not cached, so the next note tries again.
 */
export async function markdownRenderer() {
  pending ||= import("./notes.js");
  try {
    return (await pending).renderNote;
  } catch {
    pending = undefined;
    return plainText;
  }
}
