/** A decision being written but not saved yet. */
export interface Draft {
  /** The chosen decision, `reopen`, or "" for none. */
  target: string;
  note: string;
}

/**
 * Unsaved drafts by review key. The module stays loaded while the page is
 * open, so a draft survives switching files, findings and pages; only a
 * confirmed save removes it.
 */
export const drafts = new Map<string, Draft>();
