import { useId, useRef, useState, type ReactNode } from "react";
import { Loader2, RotateCcw, X } from "lucide-react";
import { Badge, stateTone } from "./ui/badge";
import { Button } from "./ui/button";
import { drafts, type Draft } from "./drafts";
import * as notify from "./notify";
import type { ApiFailure, Bridge, Finding, Review } from "./types";

// Any decision can replace any other; the server keeps every change in the history.
const DECISIONS = ["in-review", "confirmed", "false-positive", "dismissed", "resolved"];
const DECIDED = ["confirmed", "false-positive", "resolved", "dismissed"];
const actionsFor = (state: string) => [
  ...DECISIONS.filter((decision) => decision !== state),
  ...(state === "unreviewed" ? [] : ["reopen"]),
];

interface Payload {
  state: string | null;
  note: string;
}

interface Problem {
  message: string;
  /** What to resend; absent after a conflict, whose current state must be read first. */
  retry?: Payload;
}

export interface ReviewPanelProps {
  item: Finding;
  bridge: Bridge;
  onSaved(review: Review): void;
  /** What is being reviewed, above the decision (the selected file of a duplicate). */
  children?: ReactNode;
}

/** The decision of one finding. Each finding has its own draft, so remount by key. */
export function ReviewPanel(props: ReviewPanelProps) {
  return <Decision key={props.item.key} {...props} />;
}

function Decision({ item, bridge, onSaved, children }: ReviewPanelProps) {
  const { t } = bridge;
  const [draft, setDraftState] = useState<Draft>(() => drafts.get(item.key) ?? { target: "", note: item.note });
  const [saving, setSaving] = useState(false);
  const [problem, setProblem] = useState<Problem | null>(null);
  const busy = useRef(false);
  const ids = { state: useId(), note: useId() };

  const setDraft = (next: Draft) => {
    setDraftState(next);
    // Only a draft that differs from what is saved needs keeping.
    if (next.target || next.note !== item.note) drafts.set(item.key, next);
    else drafts.delete(item.key);
  };

  async function send(payload: Payload) {
    if (busy.current) return;
    busy.current = true;
    setSaving(true);
    try {
      const saved = await bridge.save(item.key, payload.state, payload.note, item.revision);
      drafts.delete(item.key);
      setDraftState({ target: "", note: saved.note ?? payload.note });
      setProblem(null);
      notify.ok(t("review.saved", { state: bridge.stateLabel(saved.state, item.type) }));
      onSaved(saved);
    } catch (error) {
      const failure = error as ApiFailure;
      if (failure.status === 409 && failure.body?.key) {
        // Someone else decided meanwhile: show theirs, keep what was typed here.
        setProblem({ message: t("review.conflict") });
        onSaved(failure.body as Review);
      } else {
        setProblem({ message: t("review.failed", { message: failure.message }), retry: payload });
      }
    } finally {
      busy.current = false;
      setSaving(false);
    }
  }

  async function apply(target: string) {
    const label = (state: string) => bridge.stateLabel(state, item.type);
    if (target === "false-positive") {
      const ok = await bridge.confirm({
        title: t("review.fpTitle"),
        body: t("review.fpBody"),
        confirm: t("review.markFp"),
      });
      if (!ok) return;
    }
    if (DECIDED.includes(item.state) && target !== "dismissed" && target !== "false-positive") {
      const ok = await bridge.confirm({
        title: t("review.changeTitle"),
        body: t("review.changeBody", {
          from: label(item.state),
          to: target === "reopen" ? t("state.unreviewed") : label(target),
        }),
        confirm: t("review.changeConfirm"),
      });
      if (!ok) return;
    }
    await send({ state: target === "reopen" ? null : target, note: draft.note.trim() });
  }

  const targets = actionsFor(item.state);
  const chosen = targets.includes(draft.target) ? draft.target : "";
  const dirty = Boolean(chosen) || draft.note !== item.note;

  return (
    <section aria-label={t("review.title")} className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-xs text-fg-2">{t("review.current")}</span>
        <Badge tone={stateTone(item.state)}>{bridge.stateLabel(item.state, item.type)}</Badge>
        {item.updated ? (
          <span className="text-xs text-fg-2">
            {t("review.updated")} {bridge.ago(item.updated)}
          </span>
        ) : null}
      </div>
      {item.state === "needs-reevaluation" && (
        <p className="m-0 text-xs text-fg-2">
          {t("review.previous", { state: bridge.stateLabel(item.previousState, item.type) })}
        </p>
      )}
      {children}
      {item.readOnly ? (
        <p className="m-0 rounded-ui border border-line bg-warning-soft p-2 text-xs">{t("review.readOnly")}</p>
      ) : (
        <form
          className="flex flex-col gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            if (chosen) void apply(chosen);
          }}
        >
          <label htmlFor={ids.state} className="text-xs font-medium text-fg-2">
            {t("review.stateLabel")}
          </label>
          <select
            id={ids.state}
            value={chosen}
            disabled={saving}
            onChange={(event) => setDraft({ ...draft, target: event.target.value })}
            className="h-8 w-full rounded-ui border border-control bg-surface px-2 text-[13px] text-fg focus-visible:outline-2 focus-visible:outline-focus"
          >
            <option value="">{t("review.choose")}</option>
            {targets.map((target) => (
              <option key={target} value={target}>
                {target === "reopen" ? t("review.reopenOption") : bridge.stateLabel(target, item.type)}
              </option>
            ))}
          </select>
          <label htmlFor={ids.note} className="text-xs font-medium text-fg-2">
            {t("review.addNote")}
          </label>
          <textarea
            id={ids.note}
            rows={3}
            maxLength={500}
            value={draft.note}
            readOnly={saving}
            placeholder={t(`review.notePlaceholder.${item.type}`)}
            onChange={(event) => setDraft({ ...draft, note: event.target.value })}
            className="min-h-16 w-full resize-y rounded-ui border border-control bg-surface p-2 font-sans text-[13px] text-fg focus-visible:outline-2 focus-visible:outline-focus"
          />
          <div className="flex flex-wrap items-center gap-2">
            <Button type="submit" variant="primary" disabled={!chosen || saving} aria-busy={saving}>
              {saving && <Loader2 aria-hidden size={14} className="animate-spin motion-reduce:animate-none" />}
              {saving ? t("review.saving") : t("review.save")}
            </Button>
            {item.state !== "dismissed" && (
              <Button
                size="sm"
                disabled={saving}
                onClick={() => void send({ state: "dismissed", note: draft.note.trim() })}
              >
                <X aria-hidden size={14} />
                {t("review.ignoreNow")}
              </Button>
            )}
            {dirty && !saving && <span className="text-xs text-warning">{t("review.unsaved")}</span>}
          </div>
          {problem && (
            <div role="alert" className="flex flex-col gap-2 rounded-ui border border-danger bg-danger-soft p-2 text-xs text-fg">
              <span>{problem.message}</span>
              {problem.retry && (
                <Button size="sm" className="self-start" disabled={saving} onClick={() => void send(problem.retry!)}>
                  <RotateCcw aria-hidden size={14} />
                  {t("common.retry")}
                </Button>
              )}
            </div>
          )}
        </form>
      )}
      <p className="m-0 text-xs text-fg-2">{t("review.reviewerNote")}</p>
    </section>
  );
}
