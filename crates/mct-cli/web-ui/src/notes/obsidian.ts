import type { MarkedExtension, Tokens } from "marked";

// Obsidian syntax that CommonMark/GFM does not know. Each construct becomes a
// token; dom.ts turns tokens into DOM nodes, so no HTML string is ever built.

export interface WikiLinkToken extends Tokens.Generic {
  type: "wikilink";
  /** Note name without heading, block id or alias. */
  target: string;
  /** Heading or block reference after `#`, if any. */
  section: string;
  label: string;
}

export interface EmbedToken extends Tokens.Generic {
  type: "embed";
  target: string;
  label: string;
}

export interface TagToken extends Tokens.Generic {
  type: "tag";
  tag: string;
}

export interface HighlightToken extends Tokens.Generic {
  type: "highlight";
  text: string;
}

export interface MathToken extends Tokens.Generic {
  type: "math";
  text: string;
  display: boolean;
}

const WIKILINK = /^\[\[([^\]\n]+)\]\]/;
const EMBED = /^!\[\[([^\]\n]+)\]\]/;
const TAG = /^#([\p{L}\p{N}_/-]*[\p{L}_/-][\p{L}\p{N}_/-]*)/u;
const HIGHLIGHT = /^==(?=\S)([^=\n]+?)(?<=\S)==/;
// `$x$` only when no space touches the dollar signs and the closing one is not
// followed by a digit, so prices such as "$5 and $6" stay text.
const INLINE_MATH = /^\$(?=[^\s$])([^$\n]*?[^\s$]|[^\s$])\$(?!\d)/;
const DISPLAY_MATH = /^\$\$([\s\S]+?)\$\$/;

/** Splits `Note#Section|alias` into its parts. */
export function parseWikiTarget(inner: string): {
  target: string;
  section: string;
  label: string;
} {
  const pipe = inner.indexOf("|");
  const reference = (pipe < 0 ? inner : inner.slice(0, pipe)).trim();
  const alias = pipe < 0 ? "" : inner.slice(pipe + 1).trim();
  const hash = reference.indexOf("#");
  const target = (hash < 0 ? reference : reference.slice(0, hash)).trim();
  const section = hash < 0 ? "" : reference.slice(hash + 1).trim();
  const label =
    alias || (section && target ? `${target} > ${section}` : target || section);
  return { target, section, label };
}

/** `index` of the first `char` that may start a token: at the very start or after whitespace. */
const startAfterSpace = (src: string, char: string): number | undefined => {
  for (let i = src.indexOf(char); i >= 0; i = src.indexOf(char, i + 1)) {
    if (i === 0 || /\s/.test(src[i - 1] ?? "")) return i;
  }
  return undefined;
};

export const obsidianExtension: MarkedExtension = {
  extensions: [
    {
      name: "embed",
      level: "inline",
      start: (src) => src.indexOf("![["),
      tokenizer(src) {
        const match = EMBED.exec(src);
        if (!match?.[1]) return undefined;
        const { target, label } = parseWikiTarget(match[1]);
        const token: EmbedToken = {
          type: "embed",
          raw: match[0],
          target,
          label,
        };
        return token;
      },
    },
    {
      name: "wikilink",
      level: "inline",
      start: (src) => src.indexOf("[["),
      tokenizer(src) {
        const match = WIKILINK.exec(src);
        if (!match?.[1]) return undefined;
        const { target, section, label } = parseWikiTarget(match[1]);
        const token: WikiLinkToken = {
          type: "wikilink",
          raw: match[0],
          target,
          section,
          label,
        };
        return token;
      },
    },
    {
      name: "tag",
      level: "inline",
      start: (src) => startAfterSpace(src, "#"),
      tokenizer(src) {
        const match = TAG.exec(src);
        if (!match?.[1]) return undefined;
        const token: TagToken = { type: "tag", raw: match[0], tag: match[1] };
        return token;
      },
    },
    {
      name: "highlight",
      level: "inline",
      start: (src) => src.indexOf("=="),
      tokenizer(src) {
        const match = HIGHLIGHT.exec(src);
        if (!match?.[1]) return undefined;
        const token: HighlightToken = {
          type: "highlight",
          raw: match[0],
          text: match[1],
          tokens: this.lexer.inlineTokens(match[1]),
        };
        return token;
      },
    },
    {
      name: "math",
      level: "inline",
      start: (src) => src.indexOf("$"),
      tokenizer(src) {
        const display = DISPLAY_MATH.exec(src);
        if (display?.[1]) {
          const token: MathToken = {
            type: "math",
            raw: display[0],
            text: display[1].trim(),
            display: true,
          };
          return token;
        }
        const inline = INLINE_MATH.exec(src);
        if (!inline?.[1]) return undefined;
        const token: MathToken = {
          type: "math",
          raw: inline[0],
          text: inline[1],
          display: false,
        };
        return token;
      },
    },
  ],
};

const FENCE = /(^|\n)(```|~~~)[^\n]*\n[\s\S]*?\n\2[^\n]*(?=\n|$)/g;

/**
 * Removes Obsidian `%%comments%%` (they are private notes), leaving fenced code
 * untouched. An unterminated `%%` is kept as text.
 */
export function stripComments(source: string): string {
  let out = "";
  let last = 0;
  const prose = (text: string) => text.replace(/%%[\s\S]*?%%/g, "");
  for (const fence of source.matchAll(FENCE)) {
    const at = fence.index ?? 0;
    out += prose(source.slice(last, at)) + fence[0];
    last = at + fence[0].length;
  }
  return out + prose(source.slice(last));
}
