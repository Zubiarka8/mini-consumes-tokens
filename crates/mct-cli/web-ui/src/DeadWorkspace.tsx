import { useId, useMemo } from "react";
import { Ban, Copy, CopyCheck, ExternalLink, FileCode, Link } from "lucide-react";
import { CodeView, type LineMark } from "./code/CodeView";
import { History } from "./History";
import { ReviewPanel } from "./ReviewPanel";
import { BackButton, PaneBody, Swatch, Workspace } from "./Workspace";
import { useSource } from "./hooks";
import { copyText } from "./notify";
import { ActionGroup, Caution, LoadState } from "./ui/actions";
import { Badge } from "./ui/badge";
import { Button } from "./ui/button";
import { Section } from "./ui/section";
import type { DeadFinding, Source, WorkspaceProps } from "./types";

// Lines shown around a candidate; the page passes 3 when its URL names none.
export const CONTEXT_CHOICES = [0, 3, 10, 25];
const LIMITS = ["byName", "dynamic", "macros", "ffi", "public", "tests", "generated", "config"];
const CONFIDENCE_TONE = { high: "danger", medium: "warning" } as const;

export function DeadWorkspace(props: WorkspaceProps & { item: DeadFinding }) {
  const { item, bridge, onSaved, onBack, onContext } = props;
  const { t } = bridge;
  const symbol = item.detail.symbol;
  const end = Math.max(symbol.endLine || symbol.line, symbol.line);
  const context = CONTEXT_CHOICES.includes(props.context) ? props.context : 3;
  const first = Math.max(1, symbol.line - context);
  const source = useSource(bridge, { path: symbol.path, start: first, end: end + context }, "review-dead-source");
  const location = `${symbol.path}:${symbol.line}–${end}`;
  const contextId = useId();
  const marks = useMemo(() => {
    const lines = new Map<number, LineMark>();
    for (let line = symbol.line; line <= end; line += 1)
      lines.set(line, { glyph: "▸", label: t("rw.symbolLine"), tone: "block" });
    return lines;
  }, [symbol.line, end, t]);
  const confidenceTone = CONFIDENCE_TONE[item.confidence as keyof typeof CONFIDENCE_TONE] ?? "neutral";
  const own = (data: Source) => {
    const start = data.start || first;
    return data.lines.slice(symbol.line - start, end - start + 1);
  };

  const files = (
    <PaneBody as="nav" label={t("rw.file")} footer={<BackButton bridge={bridge} onBack={onBack} />}>
      <div className="px-1.5 pb-2 pt-1">
        <div
          aria-current="true"
          title={location}
          className="flex items-start gap-2 rounded-ui bg-match px-2 py-1.5 font-semibold"
        >
          <FileCode aria-hidden size={15} className="mt-px shrink-0 text-fg-2" />
          <span className="min-w-0 truncate">{symbol.path.split("/").pop()}</span>
        </div>
      </div>
    </PaneBody>
  );

  const code = (
    <section aria-label={t("rw.code")} className="flex h-full min-h-0 flex-col bg-code outline-2 -outline-offset-2 outline-topbar">
      <header className="flex shrink-0 items-start gap-0.5 border-b border-line py-1.5 pl-3 pr-1">
        <span className="min-w-0 flex-1">
          <span className="block truncate font-semibold">{symbol.path}</span>
          <span className="block truncate text-[11.5px] text-fg-2">
            {t("dup.lineRange", { from: symbol.line, to: end })} · {item.title} · {item.kind}
          </span>
        </span>
        <label htmlFor={contextId} className="sr-only">
          {t("dead.context")}
        </label>
        <select
          id={contextId}
          value={String(context)}
          title={t("dead.context")}
          onChange={(event) => onContext(Number(event.target.value))}
          className="mt-0.5 h-7 rounded-ui border border-control bg-surface px-1.5 text-xs text-fg focus-visible:outline-2 focus-visible:outline-focus"
        >
          {CONTEXT_CHOICES.map((lines) => (
            <option key={lines} value={lines}>
              {lines ? t("dead.contextLines", { n: lines }) : t("dead.contextNone")}
            </option>
          ))}
        </select>
        <Button
          size="icon"
          variant="ghost"
          aria-label={t("ai.copyCode")}
          title={t("ai.copyCode")}
          disabled={source?.status !== "ready"}
          onClick={() =>
            source?.status === "ready" &&
            copyText(
              `${symbol.path}:${symbol.line}-${end}\n${own(source.data).join("\n")}`,
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
          aria-label={t("code.copyPath")}
          title={t("code.copyPath")}
          onClick={() => copyText(`${symbol.path}:${symbol.line}-${end}`, t("common.pathCopied"), t("common.copyFailed"))}
        >
          <Link aria-hidden size={14} />
        </Button>
        <Button
          size="icon"
          variant="ghost"
          aria-label={t("code.openSource")}
          title={t("code.openSource")}
          onClick={() => bridge.openSource({ path: symbol.path, line: symbol.line, language: symbol.language })}
        >
          <ExternalLink aria-hidden size={14} />
        </Button>
      </header>
      {source && (
        <LoadState state={source} loading={t("common.loading")} retry={t("common.retry")}>
          {(data) => (
            <>
              <CodeView
                lines={data.lines}
                start={data.start || first}
                language={data.language || symbol.language}
                marks={marks}
                label={`${t("compare.code")}: ${symbol.path}`}
                className="min-h-0 flex-1"
              />
              {data.truncated && <p className="m-0 border-t border-line p-2 text-xs text-fg-2">{t("dead.codeTruncated")}</p>}
            </>
          )}
        </LoadState>
      )}
    </section>
  );

  const review = (
    <PaneBody as="aside" label={t("rw.review")}>
      <div className="flex flex-col gap-3 px-3 pb-3">
        {/* The heuristic's limit stays in view, next to the decision it informs. */}
        <Caution>{t("dead.caution")}</Caution>
        <ReviewPanel item={item} bridge={bridge} onSaved={onSaved} />
        <ActionGroup label={t("rw.aiTools")}>
          <Button size="sm" onClick={() => copyText(bridge.aiBrief(item), t("ai.copied"), t("common.copyFailed"))}>
            <CopyCheck aria-hidden size={14} />
            {t("ai.copy.dead")}
          </Button>
        </ActionGroup>
        <div>
          <Section title={t("dead.refsTitle")} defaultOpen>
            <p className="m-0">
              {t("dead.knownRefs")}: <strong>0</strong>
            </p>
            <p className="m-0 text-fg-2">{t("dead.noReferences")}</p>
            <p className="m-0 mt-2 text-fg-2">
              {t("dead.deps")}: {t("dead.imports", { n: item.detail.imports.length, m: item.detail.importedBy.length })}
            </p>
            {item.detail.importedBy.slice(0, 6).map((path) => (
              <div key={path} className="break-all font-mono text-xs">
                {path}
              </div>
            ))}
          </Section>
          <Section title={t("dead.why")}>
            <p className="m-0">{t("dead.whyText", { name: item.title })}</p>
            <dl className="m-0 mt-2 grid grid-cols-[max-content_1fr] gap-x-3 gap-y-1">
              <dt className="text-fg-2">{t("dup.method")}</dt>
              <dd className="m-0">{t("dead.method")}</dd>
              <dt className="text-fg-2">{t("dead.zone")}</dt>
              <dd className="m-0">{t(`dead.zone.${item.zone}`)}</dd>
              <dt className="text-fg-2">{t("dead.visibility")}</dt>
              <dd className="m-0">{t(`dead.visibility.${item.visibility}`)}</dd>
            </dl>
            <h4 className="m-0 mt-3 text-xs font-semibold">{t("dead.limits")}</h4>
            <ul className="m-0 pl-4">
              {LIMITS.map((limit) => (
                <li key={limit}>{t(`dead.limit.${limit}`)}</li>
              ))}
            </ul>
          </Section>
          <Section title={`${t("code.notes")} (${item.detail.notes.length})`}>
            {item.detail.notes.length ? (
              item.detail.notes.map((note) => (
                <p key={note.path} className="m-0">
                  <a href={bridge.noteHref(note.path)}>{note.title}</a>
                </p>
              ))
            ) : (
              <p className="m-0 text-fg-2">{t("code.noNotes")}</p>
            )}
          </Section>
          <Section title={`${t("review.history")} (${item.history?.length ?? 0})`}>
            <History events={item.history} bridge={bridge} />
          </Section>
        </div>
        <p className="m-0 text-xs text-fg-2">{t("dead.neverDelete")}</p>
      </div>
    </PaneBody>
  );

  return (
    <Workspace
      bridge={bridge}
      icon={<Ban size={15} />}
      heading={item.number ? t("find.number.dead-code", { n: item.number }) : t("common.deadCode")}
      title={item.title}
      layoutId="review-dead-code"
      summary={
        <>
          <span>
            {symbol.language || item.language} · {item.kind} · {t(`dead.visibility.${item.visibility}`)} ·{" "}
            {t(`dead.zone.${item.zone}`)}
          </span>
          <Badge tone={confidenceTone}>{t(`dead.confidence.${item.confidence}`)}</Badge>
        </>
      }
      legend={
        <Swatch className="bg-match" glyph="▸">
          {t("dead.unreferenced", { n: end - symbol.line + 1 })}
        </Swatch>
      }
      panes={[
        { id: "files", label: t("rw.file"), minSize: 150, defaultSize: "14", content: files },
        { id: "code", label: t("rw.code"), minSize: 360, defaultSize: "62", content: code, stacked: "h-[75vh]" },
        { id: "review", label: t("rw.review"), minSize: 260, defaultSize: "24", content: review },
      ]}
    />
  );
}
