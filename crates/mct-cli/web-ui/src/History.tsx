import { Badge, stateTone } from "./ui/badge";
import type { Bridge, HistoryEvent } from "./types";

/** Every recorded change of a review, newest first. */
export function History({ events, bridge }: { events: HistoryEvent[] | undefined; bridge: Bridge }) {
  const { t } = bridge;
  if (!events?.length) return <p className="m-0 text-fg-2">{t("review.noHistory")}</p>;
  return (
    <ol className="m-0 flex list-none flex-col gap-2 p-0">
      {[...events].reverse().map((event, index) => (
        <li key={index} className="border-l-2 border-line pl-2">
          <div className="flex flex-wrap items-center gap-1">
            <Badge tone={stateTone(event.from)}>{bridge.stateLabel(event.from)}</Badge>
            <span aria-label={t("rw.becomes")}>→</span>
            <Badge tone={stateTone(event.to)}>{bridge.stateLabel(event.to)}</Badge>
          </div>
          <div className="text-xs text-fg-2">{event.at ? bridge.dateTime(event.at) : t("review.unknownTime")}</div>
          {event.note && <div className="break-words">“{event.note}”</div>}
        </li>
      ))}
    </ol>
  );
}
