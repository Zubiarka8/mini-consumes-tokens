import { memo, type ReactNode } from "react";
import { cn } from "../lib/cn";
import { tokenize } from "./tokenize";

/**
 * A marked line: a glyph in the gutter (never colour alone), a text for
 * screen readers, and the words to box inside the line (the differing values).
 */
export interface LineMark {
  glyph: string;
  label: string;
  title?: string;
  words?: string[];
  /** `block`: one of the finding's own lines (yellow); otherwise a difference (red). */
  tone?: "block";
}

interface CodeViewProps {
  lines: string[];
  start: number;
  language: string;
  label: string;
  marks?: Map<number, LineMark>;
  /** The lines of the finding itself, highlighted; the others are context. */
  block?: { from: number; to: number };
  className?: string;
}

const escape = (word: string) => word.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** A line's tokens, with every occurrence of `words` boxed (a word may span tokens' edges only whole). */
function highlighted(parts: [string, string][], words: string[] | undefined): ReactNode[] {
  const pattern = words?.length ? new RegExp(`(${words.filter(Boolean).map(escape).join("|")})`) : null;
  return parts.flatMap(([cls, text], at) =>
    (pattern ? text.split(pattern) : [text]).map((piece, index) => {
      if (!piece) return null;
      const node = cls ? <span className={cls}>{piece}</span> : piece;
      return index % 2 ? (
        <mark key={`${at}-${index}`} className="rw-word">
          {node}
        </mark>
      ) : (
        <span key={`${at}-${index}`}>{node}</span>
      );
    }),
  );
}

/**
 * Scrolls the code view inside `scope` to `line`, centred, and flashes it.
 * Only the view scrolls: neither the page nor the row of columns moves.
 */
export function revealLine(scope: Element | null, line: number) {
  const row = scope?.querySelector<HTMLElement>(`.rw-code [data-line="${line}"]`);
  const view = row?.closest<HTMLElement>(".rw-code");
  if (!row || !view) return;
  view.scrollTop = row.offsetTop - (view.clientHeight - row.offsetHeight) / 2;
  row.setAttribute("data-flash", "");
  window.setTimeout(() => row.removeAttribute("data-flash"), 1600);
}

/**
 * A read-only, line-numbered fragment. Lines keep their spacing and scroll
 * sideways inside the view, so a long line never widens the page.
 */
export const CodeView = memo(function CodeView({
  lines,
  start,
  language,
  label,
  marks,
  block,
  className,
}: CodeViewProps) {
  const state = { block: false };
  return (
    <div
      role="region"
      aria-label={label}
      tabIndex={0}
      // `relative`: the screen-reader labels of marked lines are absolutely
      // positioned, and must be clipped here instead of lengthening the page.
      className={cn(
        "rw-code relative scroll-smooth overflow-auto bg-code font-mono text-[12.5px] leading-5 text-fg focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-focus motion-reduce:scroll-auto",
        className,
      )}
    >
      <div className="min-w-max py-1">
        {lines.map((text, index) => {
          const number = start + index;
          const mark = marks?.get(number);
          const inBlock = block && number >= block.from && number <= block.to;
          return (
            <div
              key={number}
              data-line={number}
              data-marked={mark ? "true" : undefined}
              data-block={inBlock ? "true" : undefined}
              title={mark?.title}
              className={cn("flex pr-3", mark && !mark.tone ? "bg-diff" : (inBlock || mark) && "bg-match")}
            >
              <span aria-hidden className="w-12 shrink-0 select-none pr-2 text-right text-fg-2">
                {number}
              </span>
              <span aria-hidden className="w-4 shrink-0 select-none text-center font-bold text-danger">
                {mark?.glyph}
              </span>
              {mark && <span className="sr-only">{mark.label}: </span>}
              <span className="whitespace-pre">{highlighted(tokenize(text, language, state), mark?.words)}</span>
            </div>
          );
        })}
      </div>
    </div>
  );
});
