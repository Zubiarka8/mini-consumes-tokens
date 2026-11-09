import type { Token, Tokens } from "marked";
import { highlightToDom, knownLanguage } from "./highlight";
import { attachPreview } from "./preview";
import type {
  EmbedToken,
  HighlightToken,
  MathToken,
  TagToken,
  WikiLinkToken,
} from "./obsidian";

export interface NoteContext {
  /** Open a note: a wikilink name (`isWikiName`) or a relative `.md` path. */
  onNote?: (target: string, isWikiName: boolean) => void;
  onTag?: (tag: string) => void;
  /** Short description of a wikilink target for the hover preview; `null` when unknown. */
  preview?: (
    name: string,
  ) => Promise<{ title: string; excerpt: string } | null>;
}

/** What every renderer function needs: the document to create nodes in, and the context. */
interface Env {
  doc: Document;
  ctx: NoteContext;
}

const SAFE_URL = /^(https?:|mailto:)/i;
const DATA_IMAGE = /^data:image\/(png|jpe?g|gif|webp);base64,[a-z0-9+/=]+$/i;
const IMAGE_FILE = /\.(png|jpe?g|gif|webp|svg|bmp|avif)$/i;
const CALLOUT = /^\[!([\w-]+)\]([+-]?)[ \t]*/;

type Attrs = Record<string, string | undefined>;

function el(
  doc: Document,
  tag: string,
  attrs: Attrs = {},
  ...children: (Node | string | null)[]
): HTMLElement {
  const node = doc.createElement(tag);
  for (const [name, value] of Object.entries(attrs))
    if (value !== undefined) node.setAttribute(name, value);
  for (const child of children) if (child !== null) node.append(child);
  return node;
}

const capitalize = (word: string) =>
  word.charAt(0).toUpperCase() + word.slice(1);

function inline(env: Env, tokens: Token[] | undefined, into: Node): void {
  for (const token of tokens ?? []) into.appendChild(inlineToken(env, token));
}

function inlineChildren(
  env: Env,
  tag: string,
  tokens: Token[] | undefined,
  attrs: Attrs = {},
): HTMLElement {
  const node = el(env.doc, tag, attrs);
  inline(env, tokens, node);
  return node;
}

function textOf(token: Token): string {
  const generic = token as Tokens.Generic;
  return typeof generic.text === "string" ? generic.text : generic.raw;
}

function noteLink(
  env: Env,
  label: string,
  open: () => void,
  attrs: Attrs = {},
): HTMLElement {
  const link = el(env.doc, "a", { href: "#", ...attrs }, label);
  link.addEventListener("click", (event) => {
    event.preventDefault();
    open();
  });
  return link;
}

/** A link to an element of the same note. The dashboard routes on the URL hash, so the click must never reach it. */
function jumpLink(
  env: Env,
  targetId: string,
  label: string,
  attrs: Attrs = {},
): HTMLElement {
  const link = el(env.doc, "a", { href: `#${targetId}`, ...attrs }, label);
  link.addEventListener("click", (event) => {
    event.preventDefault();
    env.doc.getElementById(targetId)?.scrollIntoView?.({ block: "center" });
  });
  return link;
}

function placeholder(
  env: Env,
  kind: "image" | "embed",
  label: string,
  source: string,
): HTMLElement {
  return el(
    env.doc,
    "span",
    { class: `note-${kind}-missing`, title: source },
    label || source,
  );
}

function inlineToken(env: Env, token: Token): Node {
  const { doc, ctx } = env;
  switch (token.type) {
    case "text":
    case "escape":
      return (token as Tokens.Text).tokens
        ? inlineChildren(env, "span", (token as Tokens.Text).tokens)
        : doc.createTextNode(textOf(token));
    case "strong":
      return inlineChildren(env, "strong", (token as Tokens.Strong).tokens);
    case "em":
      return inlineChildren(env, "em", (token as Tokens.Em).tokens);
    case "del":
      return inlineChildren(env, "del", (token as Tokens.Del).tokens);
    case "codespan":
      return el(doc, "code", {}, (token as Tokens.Codespan).text);
    case "br":
      return el(doc, "br");
    case "link": {
      const link = token as Tokens.Link;
      if (SAFE_URL.test(link.href)) {
        const anchor = inlineChildren(env, "a", link.tokens, {
          href: link.href,
          target: "_blank",
          rel: "noopener noreferrer nofollow",
        });
        return anchor;
      }
      if (
        /\.md(#.*)?$/i.test(link.href) &&
        !/^[a-z][a-z0-9+.-]*:/i.test(link.href)
      ) {
        const anchor = noteLink(env, "", () => ctx.onNote?.(link.href, false));
        inline(env, link.tokens, anchor);
        return anchor;
      }
      return inlineChildren(env, "span", link.tokens);
    }
    case "image": {
      const image = token as Tokens.Image;
      if (DATA_IMAGE.test(image.href))
        return el(doc, "img", {
          src: image.href,
          alt: image.text,
          class: "note-image",
        });
      return placeholder(env, "image", image.text, image.href);
    }
    case "wikilink": {
      const link = token as WikiLinkToken;
      // A bare `[[#Heading]]` points inside the same note: nothing to open.
      if (!link.target)
        return el(doc, "span", { class: "note-self-link" }, link.label);
      const anchor = noteLink(
        env,
        link.label,
        () => ctx.onNote?.(link.target, true),
        {
          class: "wikilink",
          "data-note": link.target,
        },
      );
      if (ctx.preview) attachPreview(anchor, link.target, ctx.preview);
      return anchor;
    }
    case "embed": {
      const embed = token as EmbedToken;
      if (IMAGE_FILE.test(embed.target))
        return placeholder(env, "embed", embed.label, embed.target);
      return noteLink(
        env,
        `↳ ${embed.label}`,
        () => ctx.onNote?.(embed.target, true),
        {
          class: "note-embed",
          "data-note": embed.target,
        },
      );
    }
    case "tag": {
      const tag = (token as TagToken).tag;
      const button = el(
        doc,
        "button",
        { class: "tag", type: "button" },
        `#${tag}`,
      );
      button.addEventListener("click", () => ctx.onTag?.(tag));
      return button;
    }
    case "checkbox": {
      const checked = Boolean((token as Tokens.Generic).checked);
      const box = el(doc, "input", {
        type: "checkbox",
        disabled: "",
        "aria-label": checked ? "Done" : "To do",
      });
      if (checked) box.setAttribute("checked", "");
      return box;
    }
    case "highlight":
      return inlineChildren(env, "mark", (token as HighlightToken).tokens);
    case "math":
      return el(
        doc,
        "code",
        { class: (token as MathToken).display ? "math math-display" : "math" },
        textOf(token),
      );
    case "footnoteRef": {
      const ref = token as Tokens.Generic;
      const id = String(ref.id);
      return el(
        doc,
        "sup",
        { class: "footnote-ref" },
        jumpLink(env, `footnote-${id}`, `[${ref.label ?? id}]`, {
          id: `footnote-ref-${id}`,
          "aria-label": `Footnote ${ref.label ?? id}`,
        }),
      );
    }
    case "html":
    default:
      // Raw HTML is shown as text, never parsed.
      return doc.createTextNode(textOf(token));
  }
}

const textToken = (text: string): Tokens.Text => ({
  type: "text",
  raw: text,
  text,
});

const isBlank = (tokens: Token[]) =>
  tokens.every(
    (token) =>
      (token.type === "text" || token.type === "escape") &&
      !textOf(token).trim(),
  );

/**
 * Splits the inline tokens of a callout's first paragraph at its first line
 * break: what comes before is the title, what comes after starts the body.
 * `lead` is the first token with the `[!type]` marker already removed.
 */
function splitAtFirstLine(
  tokens: Token[],
  lead: string,
): { title: Token[]; rest: Token[] } {
  const title: Token[] = [];
  const rest: Token[] = [];
  let broken = false;
  tokens.forEach((token, index) => {
    if (broken) return void rest.push(token);
    if (token.type === "br") {
      broken = true;
      return;
    }
    if (token.type !== "text") return void title.push(token);
    const text = index === 0 ? lead : textOf(token);
    const newline = text.indexOf("\n");
    if (newline < 0) return void title.push(textToken(text));
    broken = true;
    title.push(textToken(text.slice(0, newline)));
    if (text.slice(newline + 1)) rest.push(textToken(text.slice(newline + 1)));
  });
  return { title, rest };
}

/**
 * An Obsidian callout is a blockquote whose first line is `[!type]`, optionally
 * followed by `-` (folded) or `+` (open) and a title. The tokens marked already
 * made are split, not lexed again: lexing twice would reset the footnote
 * extension's state.
 */
function calloutOf(env: Env, quote: Tokens.Blockquote): HTMLElement | null {
  const first = quote.tokens[0];
  if (first?.type !== "paragraph") return null;
  const inlineTokens = (first as Tokens.Paragraph).tokens;
  const lead = inlineTokens[0];
  const match = lead?.type === "text" ? CALLOUT.exec(textOf(lead)) : null;
  if (!lead || !match) return null;
  const [marker, kind = "note", fold = ""] = match;
  const { title, rest } = splitAtFirstLine(
    inlineTokens,
    textOf(lead).slice(marker.length),
  );
  const titleNodes = el(env.doc, "span", { class: "callout-title-text" });
  if (isBlank(title)) titleNodes.append(capitalize(kind.replace(/-/g, " ")));
  else inline(env, title, titleNodes);
  const body = el(env.doc, "div", { class: "callout-body" });
  const paragraph: Tokens.Paragraph[] = isBlank(rest)
    ? []
    : [{ type: "paragraph", raw: "", text: "", pre: false, tokens: rest }];
  blocks(env, [...paragraph, ...quote.tokens.slice(1)], body);
  const attrs: Attrs = { class: "callout", "data-callout": kind.toLowerCase() };
  if (!fold) {
    return el(
      env.doc,
      "div",
      attrs,
      el(env.doc, "div", { class: "callout-title" }, titleNodes),
      body,
    );
  }
  const details = el(env.doc, "details", {
    ...attrs,
    open: fold === "+" ? "" : undefined,
  });
  details.append(
    el(env.doc, "summary", { class: "callout-title" }, titleNodes),
    body,
  );
  return details;
}

function codeBlock(env: Env, token: Tokens.Code): HTMLElement {
  const language = (token.lang ?? "").split(/\s+/)[0] ?? "";
  const code = el(env.doc, "code", {
    class: language ? `language-${language}` : undefined,
  });
  code.append(highlightToDom(env.doc, token.text, language));
  // `has-label` leaves room above the code for the language label.
  const pre = el(env.doc, "pre", {
    class: language ? "md-code note-code has-label" : "md-code note-code",
  });
  if (language)
    pre.append(
      el(
        env.doc,
        "span",
        { class: "note-code-label" },
        knownLanguage(language) ?? language,
      ),
    );
  pre.append(code);
  return pre;
}

function listBlock(env: Env, list: Tokens.List): HTMLElement {
  const node = el(env.doc, list.ordered ? "ol" : "ul", {
    start:
      list.ordered && typeof list.start === "number" && list.start !== 1
        ? String(list.start)
        : undefined,
    class: list.items.some((item) => item.task) ? "task-list" : undefined,
  });
  for (const item of list.items) {
    // The checkbox itself is an inline `checkbox` token at the start of the item's text.
    const li = el(env.doc, "li", item.task ? { class: "task-item" } : {});
    blocks(env, item.tokens, li);
    node.append(li);
  }
  return node;
}

function tableBlock(env: Env, table: Tokens.Table): HTMLElement {
  const cell = (tag: "th" | "td", content: Tokens.TableCell) => {
    // A class, not `style.textAlign`: no inline style may reach the page.
    return inlineChildren(
      env,
      tag,
      content.tokens,
      content.align ? { class: `align-${content.align}` } : {},
    );
  };
  const head = el(env.doc, "tr", {}, ...table.header.map((c) => cell("th", c)));
  const rows = table.rows.map((row) =>
    el(env.doc, "tr", {}, ...row.map((c) => cell("td", c))),
  );
  return el(
    env.doc,
    "div",
    { class: "md-table-wrap" },
    el(
      env.doc,
      "table",
      {},
      el(env.doc, "thead", {}, head),
      el(env.doc, "tbody", {}, ...rows),
    ),
  );
}

function footnotesBlock(env: Env, token: Tokens.Generic): HTMLElement | null {
  const items = (token.rawItems ?? []) as Tokens.Generic[];
  if (!items.length) return null;
  const list = el(env.doc, "ol");
  for (const item of items) {
    const id = String(item.label);
    const li = el(env.doc, "li", { id: `footnote-${id}` });
    blocks(env, item.content as Token[], li);
    const back = jumpLink(env, `footnote-ref-${id}`, "↩", {
      class: "footnote-back",
      "aria-label": "Back to the text",
    });
    (li.lastElementChild ?? li).append(" ", back);
    list.append(li);
  }
  return el(
    env.doc,
    "section",
    { class: "footnotes", "aria-label": "Footnotes" },
    el(env.doc, "hr"),
    list,
  );
}

function blocks(env: Env, tokens: Token[], into: Node): void {
  for (const token of tokens) {
    switch (token.type) {
      case "space":
      case "footnote":
        break;
      case "heading": {
        const heading = token as Tokens.Heading;
        into.appendChild(
          inlineChildren(
            env,
            `h${Math.min(6, Math.max(1, heading.depth))}`,
            heading.tokens,
          ),
        );
        break;
      }
      case "paragraph":
        into.appendChild(
          inlineChildren(env, "p", (token as Tokens.Paragraph).tokens),
        );
        break;
      case "text": {
        // A tight list item holds its text without a paragraph.
        const text = token as Tokens.Text;
        if (text.tokens) inline(env, text.tokens, into);
        else into.appendChild(env.doc.createTextNode(text.text));
        break;
      }
      case "code":
        into.appendChild(codeBlock(env, token as Tokens.Code));
        break;
      case "blockquote": {
        const quote = token as Tokens.Blockquote;
        const callout = calloutOf(env, quote);
        if (callout) into.appendChild(callout);
        else {
          const node = el(env.doc, "blockquote");
          blocks(env, quote.tokens, node);
          into.appendChild(node);
        }
        break;
      }
      case "list":
        into.appendChild(listBlock(env, token as Tokens.List));
        break;
      case "table":
        into.appendChild(tableBlock(env, token as Tokens.Table));
        break;
      case "hr":
        into.appendChild(el(env.doc, "hr"));
        break;
      case "footnotes": {
        const section = footnotesBlock(env, token as Tokens.Generic);
        if (section) into.appendChild(section);
        break;
      }
      case "html": {
        const html = textOf(token).trim();
        // An HTML comment is invisible in a rendered note.
        if (html && !/^<!--[\s\S]*-->$/.test(html))
          into.appendChild(el(env.doc, "p", {}, html));
        break;
      }
      default:
        into.appendChild(inlineToken(env, token));
    }
  }
}

/** Builds the note's DOM from block tokens. */
export function tokensToDom(
  doc: Document,
  tokens: Token[],
  ctx: NoteContext,
): HTMLElement {
  const root = el(doc, "div", { class: "note-body" });
  blocks({ doc, ctx }, tokens, root);
  return root;
}
