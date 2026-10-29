import { vi } from "vitest";
import type { Bridge, DeadFinding, DuplicateFinding, Review } from "./types";

/** `t` returns the key, plus `k=v` pairs, so tests can find any text by its key. */
export function bridge(overrides: Partial<Bridge> = {}): Bridge {
  return {
    t: (key, vars) =>
      vars
        ? `${key}(${Object.entries(vars)
            .map(([name, value]) => `${name}=${value}`)
            .join(",")})`
        : key,
    stateLabel: (state) => `state:${state}`,
    ago: () => "moments ago",
    dateTime: () => "today",
    source: vi.fn(async ({ start, end }) => ({
      start,
      lines: Array.from({ length: end - start + 1 }, (_, n) => `line ${start + n}`),
      truncated: false,
      language: "rust",
    })),
    cancel: vi.fn(),
    save: vi.fn(),
    openSource: vi.fn(),
    confirm: vi.fn(async () => true),
    aiBrief: () => "ai brief",
    noteHref: (path) => `#/obsidian?note=${path}`,
    ...overrides,
  };
}

const review: Review = {
  key: "k1",
  number: 7,
  state: "unreviewed",
  previousState: "unreviewed",
  note: "",
  updated: null,
  revision: "fp|r1",
  readOnly: false,
  held: false,
  history: [],
};

export const duplicate = (fields: Partial<DuplicateFinding> = {}): DuplicateFinding => ({
  ...review,
  type: "duplicate",
  title: "alpha ≈ beta",
  language: "rust",
  verdict: "suspicious",
  priority: "normal",
  scope: "cross-file",
  lines: 3,
  tokens: 12,
  members: [
    { path: "src/a.rs", start: 1, end: 3, name: "alpha", kind: "function", excerpt: [], code: ["fn alpha() {", "  1", "}"] },
    { path: "src/b.rs", start: 10, end: 12, name: "beta", kind: "function", excerpt: [], code: ["fn beta() {", "  2", "}"] },
    { path: "src/c.rs", start: 20, end: 22, name: "gamma", kind: "function", excerpt: [], code: ["fn gamma() {", "  1", "}"] },
  ],
  evidence: {
    identifierPattern: "one-to-one",
    distinct: 2,
    differences: [
      { kind: "declaration", values: ["alpha", "beta", "gamma"], lines: [1, 10, 20] },
      { kind: "literal-value", values: ["1", "2", "1"], lines: [2, 11, 21] },
    ],
    warnings: ["literal-values-differ"],
  },
  ...fields,
});

export const dead = (fields: Partial<DeadFinding> = {}): DeadFinding => ({
  ...review,
  key: "d1",
  number: 3,
  type: "dead-code",
  title: "unused_helper",
  kind: "function",
  language: "rust",
  confidence: "high",
  zone: "source",
  visibility: "private",
  detail: {
    symbol: { path: "src/lib.rs", line: 10, endLine: 12, language: "rust" },
    imports: ["src/a.rs"],
    importedBy: ["src/main.rs"],
    notes: [],
  },
  ...fields,
});

export function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((ok, fail) => {
    resolve = ok;
    reject = fail;
  });
  return { promise, resolve, reject };
}

export const failure = (status: number, message: string, body: Partial<Review> | null = null) =>
  Object.assign(new Error(message), { status, body });
