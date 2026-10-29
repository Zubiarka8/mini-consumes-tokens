// The shapes the Rust API returns (api/finding, api/source, api/review) and
// the bridge the vanilla findings page hands to the workspace.

export type FindingType = "duplicate" | "dead-code";

export interface HistoryEvent {
  from: string;
  to: string;
  at: number | null;
  note: string;
  source: string;
}

/** Review fields shared by every finding (`review_json` in api/findings.rs). */
export interface Review {
  key: string;
  number: number | null;
  state: string;
  previousState: string;
  note: string;
  updated: number | null;
  revision: string;
  readOnly: boolean;
  held: boolean;
  history?: HistoryEvent[];
}

export interface Member {
  path: string;
  start: number;
  end: number;
  name: string;
  kind: string;
  lines?: number;
  tokens?: number;
  excerpt: string[];
  /** The full body when it fit the server's budget. */
  code?: string[] | null;
  codeTruncated?: boolean;
}

export interface Difference {
  kind: string;
  /** One value and one line per member, in member order. */
  values: string[];
  lines: number[];
}

export interface Evidence {
  identifierPattern: string;
  distinct: number;
  differences: Difference[];
  warnings: string[];
}

export interface DuplicateFinding extends Review {
  type: "duplicate";
  title: string;
  language: string;
  verdict: string;
  priority: string;
  priorityReason?: string;
  scope: string;
  lines: number;
  tokens: number;
  members: Member[];
  reasons?: string[];
  method?: { normalization?: string };
  analysis?: string;
  evidence?: Evidence;
}

export interface DeadSymbol {
  path: string;
  line: number;
  endLine?: number | null;
  language: string;
}

export interface DeadFinding extends Review {
  type: "dead-code";
  title: string;
  kind: string;
  language: string;
  confidence: string;
  zone: string;
  visibility: string;
  detail: {
    symbol: DeadSymbol;
    imports: string[];
    importedBy: string[];
    notes: { path: string; title: string }[];
  };
}

export type Finding = DuplicateFinding | DeadFinding;

export interface Source {
  start?: number;
  lines: string[];
  truncated?: boolean;
  language?: string;
}

/** A failed request; a 409 carries the current review in `body`. */
export interface ApiFailure extends Error {
  status?: number;
  body?: Partial<Review> | null;
}

/** What the vanilla page provides: translations, API calls and dialogs. */
export interface Bridge {
  t(key: string, vars?: Record<string, string | number>): string;
  stateLabel(state: string, type?: FindingType): string;
  ago(unixSeconds: number | null): string;
  dateTime(unixSeconds: number | null): string;
  source(params: { path: string; start: number; end: number }, key: string): Promise<Source>;
  cancel(key: string): void;
  save(key: string, state: string | null, note: string, revision: string): Promise<Review>;
  openSource(target: { path: string; line: number; language: string }): void;
  confirm(dialog: { title: string; body: string; confirm: string }): Promise<boolean>;
  /** Context, information, paths, code, the validations to run and the report to write. */
  aiBrief(item: Finding): string;
  noteHref(path: string): string;
}

export interface WorkspaceProps {
  item: Finding;
  /** Index of the selected member of a duplicate. */
  file: number;
  /** Context lines around a dead-code candidate. */
  context: number;
  bridge: Bridge;
  onBack(): void;
  onSelectFile(index: number): void;
  onContext(lines: number): void;
  /** A decision was saved (or a conflict returned the current one). */
  onSaved(review: Review): void;
}
