// The light highlighter of app/code.js, ported as is: it only colours a
// fragment for reading, for every indexed language, without a grammar.

const KEYWORDS = new Set(
  `fn let mut pub use mod struct enum impl trait for while loop if else elif match return const static self Self super crate async await move as in is not and or
function var class extends implements interface new import export from default def lambda yield pass with try except catch finally raise throw throws final public private protected
package namespace using void int bool true false null None True False nil undefined this typeof instanceof switch case break continue do where type readonly abstract override
echo fi then done esac local select begin end module require`.split(/\s+/),
);
const HASH_LANGS = new Set(["python", "bash", "powershell", "ruby", "yaml", "toml", "markdown"]);
const DASH_LANGS = new Set(["lua", "sql"]);

/** A run of text and its token class ("" for plain text). */
export type Token = [className: string, text: string];

/** Tokenizes one line; `state.block` carries a block comment across lines. */
export function tokenize(line: string, language: string, state: { block: boolean }): Token[] {
  const out: Token[] = [];
  const hash = HASH_LANGS.has(language);
  const dash = DASH_LANGS.has(language);
  let plain = "";
  let i = 0;
  const flush = () => {
    if (plain) out.push(["", plain]);
    plain = "";
  };
  const span = (cls: string, text: string) => {
    flush();
    out.push([cls, text]);
  };
  while (i < line.length) {
    const rest = line.slice(i);
    if (state.block) {
      const end = rest.indexOf("*/");
      if (end < 0) {
        span("tk-c", rest);
        i = line.length;
      } else {
        span("tk-c", rest.slice(0, end + 2));
        i += end + 2;
        state.block = false;
      }
      continue;
    }
    if (!hash && !dash && rest.startsWith("/*")) {
      state.block = true;
      continue;
    }
    if ((hash && line[i] === "#") || (dash && rest.startsWith("--")) || (!hash && !dash && rest.startsWith("//"))) {
      span("tk-c", rest);
      break;
    }
    const ch = line[i];
    if (ch === '"' || ch === "'" || ch === "`") {
      let j = i + 1;
      while (j < line.length && line[j] !== ch) j += line[j] === "\\" ? 2 : 1;
      span("tk-s", line.slice(i, Math.min(j + 1, line.length)));
      i = j + 1;
      continue;
    }
    const number = /^\d[\d_.xa-fA-F]*/.exec(rest);
    if (number && !/\w/.test(line[i - 1] || "")) {
      span("tk-n", number[0]);
      i += number[0].length;
      continue;
    }
    const word = /^[A-Za-z_]\w*/.exec(rest);
    if (word) {
      const text = word[0];
      if (KEYWORDS.has(text)) span("tk-k", text);
      else if (rest[text.length] === "(" || rest[text.length] === "!") span("tk-f", text);
      else if (/^[A-Z]/.test(text) && /[a-z]/.test(text)) span("tk-t", text);
      else plain += text;
      i += text.length;
      continue;
    }
    plain += ch;
    i += 1;
  }
  flush();
  return out;
}
