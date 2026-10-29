import { useMemo, useRef, useState } from "react";
import { Copy, CopyCheck, ExternalLink, Eye, EyeOff, X } from "lucide-react";
import { CodeView, revealLine, type LineMark } from "./code/CodeView";
import { History } from "./History";
import { ReviewPanel } from "./ReviewPanel";
import { BackButton, PaneBody, Swatch, Workspace } from "./Workspace";
import { useSource } from "./hooks";
import { cn } from "./lib/cn";
import { copyText } from "./notify";
import { ActionGroup, Caution, LoadState } from "./ui/actions";
import { Badge } from "./ui/badge";
import { Button } from "./ui/button";
import { Section } from "./ui/section";
import type { Bridge, Difference, DuplicateFinding, Member, WorkspaceProps } from "./types";

/** Lines shown above and below each fragment, so the duplicate is seen in its place. */
export const CONTEXT = 5;

/** The name of a file within its group: A, B … Z, AA, AB … (spreadsheet columns), so it can be told apart and named. */
export function fileLetter(index: number): string {
  let letters = "";
  for (let rest = index + 1; rest > 0; rest = Math.floor((rest - 1) / 26))
    letters = String.fromCharCode(65 + ((rest - 1) % 26)) + letters;
  return letters;
}

/** The file's letter as a chip, the same in the list, the column and the review. */
function Letter({ index, bridge, dim }: { index: number; bridge: Bridge; dim?: boolean }) {
  const letter = fileLetter(index);
  return (
    <span
      role="img"
      aria-label={bridge.t("dup.fileLetter", { letter })}
      className={cn(
        "inline-flex h-[18px] min-w-[22px] shrink-0 items-center justify-center rounded-sm px-1 font-mono text-[11px] font-bold",
        dim ? "bg-elevated text-fg-2" : "bg-primary text-on-primary",
      )}
    >
      {letter}
    </span>
  );
}

const basename = (path: string) => path.split("/").pop() ?? path;
const directory = (path: string) => path.split("/").slice(0, -1).join("/");
const range = (member: Member) => `${member.start}–${member.end}`;

/**
 * Lines the detector found different, per member, with the differing values
 * to box. Values and lines are in member order; two differences on one line
 * share its mark.
 */
function differenceMarks(item: DuplicateFinding, bridge: Bridge): Map<number, LineMark>[] {
  const { t } = bridge;
  return item.members.map((_, member) => {
    const marks = new Map<number, LineMark>();
    for (const difference of item.evidence?.differences ?? []) {
      const line = difference.lines[member];
      if (!line) continue;
      const value = difference.values[member] ?? "";
      const text = `${t(`rw.kind.${difference.kind}`)}: ${value}`;
      const known = marks.get(line);
      marks.set(line, {
        glyph: "≠",
        label: t("rw.diffMark"),
        title: known?.title ? `${known.title}\n${text}` : text,
        words: [...(known?.words ?? []), value],
      });
    }
    return marks;
  });
}

/** One file of the duplicate: its path and lines, then its fragment among its context lines. */
function FileColumn({
  item,
  index,
  selected,
  marks,
  props,
  onHide,
}: {
  item: DuplicateFinding;
  index: number;
  selected: boolean;
  marks: Map<number, LineMark>;
  props: WorkspaceProps;
  onHide?(): void;
}) {
  const { bridge, onSelectFile } = props;
  const { t } = bridge;
  const member = item.members[index];
  const start = Math.max(1, member.start - CONTEXT);
  const fetched = useSource(bridge, { path: member.path, start, end: member.end + CONTEXT }, `review-member-${index}`)!;
  // Until its context arrives, the fragment the finding carries is shown.
  const loaded =
    fetched.status !== "ready" && member.code
      ? { status: "ready" as const, data: { start: member.start, lines: member.code } }
      : fetched;
  const own = (lines: string[], first: number) => lines.slice(member.start - first, member.end - first + 1);
  return (
    <section
      aria-label={`${member.path}:${range(member)}`}
      data-member={index}
      className={cn(
        "flex min-w-[280px] flex-1 flex-col border-r border-line bg-code last:border-r-0",
        selected && "outline-2 -outline-offset-2 outline-topbar",
      )}
    >
      <header className="flex shrink-0 items-start gap-0.5 border-b border-line py-1.5 pl-3 pr-1">
        <button
          type="button"
          aria-pressed={selected}
          title={t("dup.selectFile", { path: member.path })}
          onClick={() => onSelectFile(index)}
          className="min-w-0 flex-1 cursor-pointer rounded-sm border-0 bg-transparent p-0 text-left text-fg focus-visible:outline-2 focus-visible:outline-focus"
        >
          <span className="flex items-center gap-1.5">
            <Letter index={index} bridge={bridge} />
            <span className="min-w-0 flex-1 truncate font-semibold">{member.path}</span>
          </span>
          <span className="block truncate text-[11.5px] text-fg-2">
            {t("dup.lineRange", { from: member.start, to: member.end })} · {member.name}
          </span>
        </button>
        <Button
          size="icon"
          variant="ghost"
          aria-label={`${t("ai.copyCode")}: ${member.path}`}
          title={t("ai.copyCode")}
          disabled={loaded.status !== "ready"}
          onClick={() =>
            loaded.status === "ready" &&
            copyText(
              `${member.path}:${member.start}-${member.end}\n${own(loaded.data.lines, loaded.data.start || member.start).join("\n")}`,
              t("ai.codeCopied"),
              t("common.copyFailed"),
            )
          }
        >
          <Copy aria-hidden size={14} />
        </Button>
        <Button
          size="icon"
          variant="ghost"
          aria-label={`${t("code.openSource")}: ${member.path}`}
          title={t("code.openSource")}
          onClick={() => bridge.openSource({ path: member.path, line: member.start, language: item.language })}
        >
          <ExternalLink aria-hidden size={14} />
        </Button>
        {onHide && (
          <Button
            size="icon"
            variant="ghost"
            aria-label={t("rw.hideFile", { path: member.path })}
            title={t("rw.hideFile", { path: member.path })}
            onClick={onHide}
          >
            <X aria-hidden size={14} />
          </Button>
        )}
      </header>
      <LoadState state={loaded} loading={t("common.loading")} retry={t("common.retry")}>
        {(source) => (
          <>
            <CodeView
              lines={source.lines}
              start={source.start || member.start}
              language={item.language}
              marks={marks}
              block={{ from: member.start, to: member.end }}
              label={`${t("compare.code")}: ${member.path}`}
              className="min-h-0 flex-1"
            />
            {(source.truncated || member.codeTruncated) && (
              <p className="m-0 border-t border-line p-2 text-xs text-fg-2">{t("code.truncated")}</p>
            )}
          </>
        )}
      </LoadState>
    </section>
  );
}

/** The detector's differences, each one a jump to its line in every file. */
function Differences({
  item,
  file,
  bridge,
  onReveal,
}: {
  item: DuplicateFinding;
  file: number;
  bridge: Bridge;
  onReveal(difference: Difference): void;
}) {
  const { t } = bridge;
  const evidence = item.evidence;
  const differences = evidence?.differences ?? [];
  if (!differences.length) return <p className="m-0 border-t border-line py-2 text-xs text-fg-2">{t("rw.noDifferences")}</p>;
  return (
    <Section title={t("rw.differences", { n: evidence?.distinct ?? 0 })}>
      <ul className="m-0 flex list-none flex-col gap-1 p-0">
        {differences.map((difference, index) => (
          <li key={index} className="flex items-start gap-2 text-xs">
            <span aria-hidden className="pt-0.5 font-bold text-danger">
              ≠
            </span>
            <span className="min-w-0 flex-1">
              <span className="block">{t(`rw.kind.${difference.kind}`)}</span>
              <span className="block break-all font-mono text-fg-2">
                {difference.values[file]} → {[...new Set(difference.values.filter((_, at) => at !== file))].join(", ")}
              </span>
            </span>
            {difference.lines[file] ? (
              <Button
                size="sm"
                variant="ghost"
                className="h-6 font-mono text-[11.5px] text-fg-2"
                aria-label={t("rw.goToLine", { line: difference.lines[file] })}
                onClick={() => onReveal(difference)}
              >
                {t("rw.line", { n: difference.lines[file] })}
              </Button>
            ) : null}
          </li>
        ))}
      </ul>
      {evidence && evidence.distinct > differences.length && (
        <p className="m-0 mt-1 text-xs text-fg-2">
          {t("rw.differencesListed", { shown: differences.length, total: evidence.distinct })}
        </p>
      )}
    </Section>
  );
}

export function DuplicateWorkspace(props: WorkspaceProps & { item: DuplicateFinding }) {
  const { item, bridge, onSelectFile, onSaved, onBack } = props;
  const { t } = bridge;
  const file = Math.min(Math.max(props.file, 0), item.members.length - 1);
  const marks = useMemo(() => differenceMarks(item, bridge), [item, bridge]);
  const columns = useRef<HTMLDivElement>(null);
  // Files closed from the comparison; choosing one in the list brings it back.
  const [hidden, setHidden] = useState<number[]>([]);
  const visible = item.members.map((_, index) => index).filter((index) => !hidden.includes(index));
  const names = item.members.map((member) => basename(member.path));
  const ambiguous = (name: string) => names.filter((other) => other === name).length > 1;

  const select = (index: number) => {
    setHidden(hidden.filter((other) => other !== index));
    onSelectFile(index);
  };
  // Every file scrolls to its own line of the difference.
  const reveal = (difference: Difference) =>
    columns.current?.querySelectorAll("[data-member]").forEach((column) => {
      const line = difference.lines[Number(column.getAttribute("data-member"))];
      if (line) revealLine(column, line);
    });

  const files = (
    <PaneBody as="nav" label={t("dup.fileListLabel")} footer={<BackButton bridge={bridge} onBack={onBack} />}>
      <ul className="m-0 flex list-none flex-col gap-0.5 px-1.5 pb-2 pt-1">
        {item.members.map((member, index) => {
          const closed = hidden.includes(index);
          return (
            <li key={`${member.path}:${member.start}`}>
              <button
                type="button"
                aria-current={index === file ? "true" : undefined}
                title={`${member.path}:${range(member)} · ${member.name}`}
                onClick={() => select(index)}
                className={cn(
                  "flex w-full cursor-pointer items-start gap-2 rounded-ui border-0 border-l-[3px] px-2 py-1.5 text-left focus-visible:outline-2 focus-visible:outline-focus",
                  // Open: tinted and coloured. Closed: faded, as if switched off.
                  closed
                    ? "border-l-transparent bg-transparent text-fg-2 opacity-60 hover:bg-hover hover:opacity-100"
                    : "border-l-success bg-success-soft text-fg",
                  index === file && "font-semibold outline-2 -outline-offset-2 outline-topbar",
                )}
              >
                {closed ? (
                  <EyeOff aria-hidden size={15} className="mt-px shrink-0" />
                ) : (
                  <Eye aria-hidden size={15} className="mt-px shrink-0 text-success" />
                )}
                <Letter index={index} bridge={bridge} dim={closed} />
                <span className="min-w-0 flex-1">
                  <span className={cn("block truncate", closed && "line-through decoration-1")}>{names[index]}</span>
                  {ambiguous(names[index]) && (
                    <span className="block truncate text-[11px] font-normal text-fg-2">
                      {directory(member.path) || "."} · {member.start}
                    </span>
                  )}
                </span>
                <span className="sr-only">{closed ? t("rw.fileClosed") : t("rw.fileOpen")}</span>
              </button>
            </li>
          );
        })}
      </ul>
    </PaneBody>
  );

  const comparison = (
    <div
      ref={columns}
      role="region"
      aria-label={t("dup.compareTitle")}
      tabIndex={0}
      className="flex h-full min-h-0 overflow-x-auto bg-code focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-focus"
    >
      {visible.map((index) => (
        <FileColumn
          key={`${item.members[index].path}:${item.members[index].start}`}
          item={item}
          index={index}
          selected={index === file}
          marks={marks[index]}
          props={props}
          onHide={visible.length > 1 ? () => setHidden([...hidden, index]) : undefined}
        />
      ))}
    </div>
  );

  const review = (
    <PaneBody as="aside" label={t("rw.review")}>
      <div className="flex flex-col gap-3 px-3 pb-3">
        <ReviewPanel item={item} bridge={bridge} onSaved={onSaved}>
          <div className="flex flex-col gap-1.5 rounded-ui border border-line bg-elevated p-2">
            <span className="text-xs font-semibold text-fg">{t("dup.decisionTarget")}</span>
            <ul className="m-0 flex list-none flex-wrap gap-1 p-0">
              {item.members.map((member, index) => (
                <li
                  key={`${member.path}:${member.start}`}
                  title={`${member.path}:${range(member)}`}
                  className="flex items-center gap-1 text-xs"
                >
                  <Letter index={index} bridge={bridge} />
                  <span className="max-w-[120px] truncate text-fg-2">{basename(member.path)}</span>
                </li>
              ))}
            </ul>
            <p className="m-0 text-xs text-fg-2">{t("dup.groupDecision", { n: item.members.length })}</p>
          </div>
        </ReviewPanel>
        <ActionGroup label={t("rw.aiTools")}>
          <Button size="sm" onClick={() => copyText(bridge.aiBrief(item), t("ai.copied"), t("common.copyFailed"))}>
            <CopyCheck aria-hidden size={14} />
            {t("ai.copy.duplicate")}
          </Button>
        </ActionGroup>
        <Caution>{t("dup.caution")}</Caution>
        <div>
          <Differences item={item} file={file} bridge={bridge} onReveal={reveal} />
          <Section title={t("dup.evidence")}>
            <dl className="m-0 grid grid-cols-[max-content_1fr] gap-x-3 gap-y-1">
              <dt className="text-fg-2">{t("dup.method")}</dt>
              <dd className="m-0">{t("dup.methodTokens", { mode: item.method?.normalization || "—" })}</dd>
              <dt className="text-fg-2">{t("dup.verdictLabel")}</dt>
              <dd className="m-0">{t(`dup.verdict.${item.verdict}`)}</dd>
              <dt className="text-fg-2">{t("dup.size")}</dt>
              <dd className="m-0">{t("dup.sizeValue", { lines: item.lines, tokens: item.tokens })}</dd>
              <dt className="text-fg-2">{t("dup.priority")}</dt>
              <dd className="m-0">
                {t(`dup.priority.${item.priority}`)}
                {item.priorityReason ? ` — ${item.priorityReason}` : ""}
              </dd>
              <dt className="text-fg-2">{t("dup.scope")}</dt>
              <dd className="m-0">{t(`dup.scope.${item.scope}`)}</dd>
            </dl>
            {item.reasons?.length ? (
              <ul className="m-0 mt-2 pl-4">
                {item.reasons.map((reason) => (
                  <li key={reason}>{t(`dup.reason.${reason}`)}</li>
                ))}
              </ul>
            ) : null}
            <p className="m-0 mt-2 text-xs text-fg-2">{t("dup.noSimilarity")}</p>
            {item.analysis && (
              <details className="mt-2">
                <summary className="cursor-pointer">{t("dup.technical")}</summary>
                <pre className="m-0 mt-1 max-h-64 overflow-auto whitespace-pre font-mono text-[11.5px]">
                  {item.analysis}
                </pre>
              </details>
            )}
          </Section>
          <Section title={`${t("review.history")} (${item.history?.length ?? 0})`}>
            <History events={item.history} bridge={bridge} />
          </Section>
        </div>
      </div>
    </PaneBody>
  );

  return (
    <Workspace
      bridge={bridge}
      icon={<Copy size={15} />}
      heading={item.number ? t("find.number.duplicate", { n: item.number }) : t("common.duplicate")}
      title={item.title}
      layoutId="review-duplicate"
      summary={
        <>
          <span>{t("dup.meta", { files: item.members.length, lines: item.lines, tokens: item.tokens })}</span>
          <Badge tone={item.verdict === "real" ? "success" : "warning"}>{t(`dup.verdict.${item.verdict}`)}</Badge>
        </>
      }
      legend={
        <>
          <Swatch className="bg-match">{t("rw.matchLegend")}</Swatch>
          <Swatch className="bg-diff" glyph="≠">
            {t("rw.diffLegend")}
          </Swatch>
        </>
      }
      panes={[
        { id: "files", label: t("dup.fileListLabel"), minSize: 150, defaultSize: "14", content: files },
        {
          id: "comparison",
          label: t("rw.comparison"),
          minSize: 360,
          defaultSize: "64",
          content: comparison,
          stacked: "h-[75vh]",
        },
        { id: "review", label: t("rw.review"), minSize: 260, defaultSize: "22", content: review },
      ]}
    />
  );
}
