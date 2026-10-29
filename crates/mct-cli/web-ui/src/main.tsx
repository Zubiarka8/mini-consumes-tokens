// Entry of review.js: the vanilla findings page imports it when a finding is
// opened and drives it through `update`, so the list and the URL stay its own.
import { createRoot } from "react-dom/client";
import { App } from "./App";
import type { WorkspaceProps } from "./types";
import "./styles.css";

export function mountReview(host: HTMLElement, props: WorkspaceProps) {
  const root = createRoot(host);
  root.render(<App {...props} />);
  return {
    update(next: WorkspaceProps) {
      root.render(<App {...next} />);
    },
    unmount() {
      root.unmount();
    },
  };
}
