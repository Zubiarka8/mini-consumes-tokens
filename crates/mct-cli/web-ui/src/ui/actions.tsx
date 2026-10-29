import type { ReactNode } from "react";
import { AlertTriangle, Loader2, RotateCcw } from "lucide-react";
import { Button } from "./button";
import type { Loaded } from "../hooks";

/** A row of secondary actions, wrapping instead of overflowing. */
export function ActionGroup({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div role="group" aria-label={label} className="flex flex-wrap gap-1.5">
      {children}
    </div>
  );
}

/** A caution that must stay in view next to the decision it informs. */
export function Caution({ children }: { children: ReactNode }) {
  return (
    <p role="note" className="m-0 flex gap-2 rounded-ui border border-line bg-warning-soft p-2 text-xs text-fg">
      <AlertTriangle aria-hidden size={14} className="mt-px shrink-0 text-warning" />
      <span>{children}</span>
    </p>
  );
}

/** Loading, failure with Retry, or the loaded content. */
export function LoadState<T>({
  state,
  loading,
  retry,
  children,
}: {
  state: Loaded<T>;
  loading: string;
  retry: string;
  children(data: T): ReactNode;
}) {
  if (state.status === "ready") return <>{children(state.data)}</>;
  if (state.status === "loading")
    return (
      <p role="status" className="m-0 flex items-center gap-2 p-3 text-fg-2">
        <Loader2 aria-hidden size={14} className="animate-spin motion-reduce:animate-none" />
        {loading}
      </p>
    );
  return (
    <div role="alert" className="flex flex-col items-start gap-2 p-3 text-xs">
      <span>{state.message}</span>
      <Button size="sm" onClick={state.retry}>
        <RotateCcw aria-hidden size={14} />
        {retry}
      </Button>
    </div>
  );
}
