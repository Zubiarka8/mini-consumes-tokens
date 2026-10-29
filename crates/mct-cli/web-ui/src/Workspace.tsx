import { Fragment, type ReactNode } from "react";
import { ArrowLeft } from "lucide-react";
import { m } from "motion/react";
import { Group, Panel, Separator, useDefaultLayout } from "react-resizable-panels";
import { Button } from "./ui/button";
import { layoutStorage, useMedia } from "./hooks";
import { cn } from "./lib/cn";
import type { Bridge } from "./types";

export interface Pane {
  id: string;
  label: string;
  /** Pixels: below this a pane would cut its content. */
  minSize: number;
  /** Percent of the width, before the person resizes. */
  defaultSize: string;
  content: ReactNode;
  /** Classes of the pane when stacked in a narrow window (the code needs a height). */
  stacked?: string;
}

/** Below this width the panes are stacked and nothing is resizable. */
export const WIDE = "(min-width: 900px)";

/**
 * An open finding laid out like an editor: files, code and review side by
 * side with keyboard-resizable separators (arrow keys), or stacked in a
 * narrow window, over a bar that names the finding and explains the colours.
 * Pane widths are remembered per finding type.
 */
export function Workspace({
  bridge,
  icon,
  heading,
  title,
  summary,
  legend,
  layoutId,
  panes,
}: {
  bridge: Bridge;
  icon: ReactNode;
  heading: string;
  title: string;
  summary: ReactNode;
  legend: ReactNode;
  layoutId: string;
  panes: Pane[];
}) {
  const { t } = bridge;
  const wide = useMedia(WIDE);
  const layout = useDefaultLayout({ id: layoutId, storage: layoutStorage, onlySaveAfterUserInteractions: true });
  return (
    <m.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      transition={{ duration: 0.12, ease: "easeOut" }}
      className="flex h-full min-h-0 flex-col overflow-hidden bg-bg font-sans text-[13px] text-fg"
    >
      {wide ? (
        <Group
          orientation="horizontal"
          className="min-h-0 flex-1"
          defaultLayout={layout.defaultLayout}
          onLayoutChanged={layout.onLayoutChanged}
        >
          {panes.map((pane, index) => (
            <Fragment key={pane.id}>
              {index > 0 && (
                <Separator
                  className="rw-separator"
                  aria-label={t("rw.resize", { name: pane.label })}
                  title={t("rw.resizeHint")}
                />
              )}
              <Panel id={pane.id} minSize={pane.minSize} defaultSize={pane.defaultSize} className="min-w-0">
                {pane.content}
              </Panel>
            </Fragment>
          ))}
        </Group>
      ) : (
        <div className="flex min-h-0 flex-1 flex-col divide-y divide-line">
          {panes.map((pane) => (
            <div key={pane.id} className={cn("min-w-0 shrink-0", pane.stacked)}>
              {pane.content}
            </div>
          ))}
        </div>
      )}
      <footer
        aria-label={t("rw.statusBar")}
        className="flex min-h-10 shrink-0 flex-wrap items-center gap-x-4 gap-y-1 border-t border-line bg-surface px-3 py-1.5 text-xs"
      >
        <div className="flex min-w-0 items-center gap-2">
          <span aria-hidden className="text-fg-2">
            {icon}
          </span>
          <h2 className="m-0 shrink-0 text-[13px] font-semibold">{heading}</h2>
          <span className="truncate font-mono text-fg-2" title={title}>
            {title}
          </span>
        </div>
        <span aria-hidden className="h-4 w-px bg-line max-sm:hidden" />
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-fg-2">{summary}</div>
        <div className="flex flex-wrap items-center gap-x-4 gap-y-1 sm:ml-auto">{legend}</div>
      </footer>
    </m.div>
  );
}

/** A colour of the code and what it means; the swatch repeats the glyph, so colour is never alone. */
export function Swatch({ className, glyph, children }: { className: string; glyph?: string; children: ReactNode }) {
  return (
    <span className="flex items-center gap-1.5">
      <span
        aria-hidden
        className={cn("grid h-3.5 w-3.5 place-items-center rounded-sm text-[10px] font-bold text-danger", className)}
      >
        {glyph}
      </span>
      {children}
    </span>
  );
}

/** The way back to the list, at the foot of the files pane. */
export function BackButton({ bridge, onBack }: { bridge: Bridge; onBack(): void }) {
  return (
    <Button className="w-full" onClick={onBack}>
      <ArrowLeft aria-hidden size={14} />
      {bridge.t("dup.backToList")}
    </Button>
  );
}

/**
 * A pane with a title, like an editor's side bars: the landmark is `nav`,
 * `section` or `aside` (complementary). Its body scrolls; the footer stays.
 */
export function PaneBody({
  as: Tag,
  label,
  footer,
  className,
  children,
}: {
  as: "nav" | "section" | "aside";
  label: string;
  footer?: ReactNode;
  className?: string;
  children: ReactNode;
}) {
  return (
    <Tag aria-label={label} className={cn("flex h-full min-h-0 flex-col bg-surface", className)}>
      <h3 className="m-0 shrink-0 px-3 pb-1 pt-2.5 text-[13px] font-semibold">{label}</h3>
      <div className="min-h-0 flex-1 overflow-auto">{children}</div>
      {footer && <div className="shrink-0 border-t border-line p-2.5">{footer}</div>}
    </Tag>
  );
}
