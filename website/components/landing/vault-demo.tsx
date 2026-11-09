"use client";
import { useState } from "react";
import { baseName, demoFiles, importedBy } from "./demo-data";

const INDEX = "index.md";
const MINE = "My notes.md";
type Page = string;
type View = "notes" | "graph";
const spots: Record<string, [number, number]> = {
  [INDEX]: [160, 115],
  "src/cli.js": [55, 45],
  "src/invoice.js": [265, 45],
  "src/format.js": [265, 185],
  "src/currency.js": [55, 185],
  [MINE]: [160, 215],
};
const edges = [
  ...demoFiles.map((f) => [INDEX, f.path] as const),
  ...demoFiles.flatMap((f) => f.imports.map((i) => [f.path, i] as const)),
];

function Graph({ page, onPick }: { page: Page; onPick: (p: Page) => void }) {
  const near = (p: string) =>
    p === page ||
    edges.some(([a, b]) => (a === page && b === p) || (b === page && a === p));
  return (
    <svg
      viewBox="0 0 320 256"
      className="mock-graph"
      role="group"
      aria-label="Link graph"
    >
      {edges.map(([a, b]) => (
        <line
          key={a + b}
          x1={spots[a][0]}
          y1={spots[a][1]}
          x2={spots[b][0]}
          y2={spots[b][1]}
          data-hot={a === page || b === page}
          data-soft={a === INDEX}
        />
      ))}
      {Object.entries(spots).map(([p, [x, y]]) => (
        <g
          key={p}
          tabIndex={0}
          role="button"
          aria-label={p}
          aria-pressed={p === page}
          data-on={p === page}
          data-dim={!near(p)}
          transform={`translate(${x} ${y})`}
          onClick={() => onPick(p)}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              onPick(p);
            }
          }}
        >
          <circle r={p === INDEX ? 15 : 11} />
          <text y={p === INDEX ? 30 : 25} textAnchor="middle">
            {p === INDEX || p === MINE ? p.replace(".md", "") : baseName(p)}
          </text>
        </g>
      ))}
    </svg>
  );
}

// Clickable stand-in for the generated Obsidian vault.
export function VaultDemo() {
  const [page, setPage] = useState<Page>(demoFiles[1].path);
  const [runs, setRuns] = useState(1);
  const [busy, setBusy] = useState(false);
  const [view, setView] = useState<View>("notes");
  const [mine, setMine] = useState("Ask finance if tax rate changes in 2027.");
  const note = demoFiles.find((f) => f.path === page);
  const reindex = () => {
    if (busy) return;
    setBusy(true);
    setTimeout(() => {
      setRuns((n) => n + 1);
      setBusy(false);
    }, 700);
  };
  const link = (p: string) => (
    <button
      key={p}
      type="button"
      className="wikilink"
      onClick={() => setPage(p)}
    >
      [[{baseName(p)}]]
    </button>
  );
  return (
    <>
      <div className="mock-bar vault-bar">
        <span className="mock-live" data-busy={busy}>
          {busy ? "indexing…" : `synced · run ${runs}`}
        </span>
        <span className="mock-views" role="group" aria-label="View">
          {(["notes", "graph"] as const).map((v) => (
            <button
              key={v}
              type="button"
              aria-pressed={view === v}
              onClick={() => setView(v)}
            >
              {v === "notes" ? "Notes" : "Graph"}
            </button>
          ))}
        </span>
        <button type="button" onClick={reindex} disabled={busy}>
          Reindex
        </button>
      </div>
      <div className="mock-vault">
        <ul className="mock-tree" aria-label="Vault files">
          {[INDEX, ...demoFiles.map((f) => f.path), MINE].map((p) => (
            <li key={p}>
              <button
                type="button"
                aria-current={p === page ? "page" : undefined}
                onClick={() => setPage(p)}
              >
                {p === INDEX || p === MINE ? p : baseName(p)}
              </button>
            </li>
          ))}
        </ul>
        {view === "graph" ? (
          <div className="mock-note">
            <Graph page={page} onPick={setPage} />
            <p className="mock-h">
              {page}
              <button type="button" onClick={() => setView("notes")}>
                Open note
              </button>
            </p>
          </div>
        ) : (
          <div className="mock-note" key={page}>
            {page === INDEX && (
              <>
                <pre>{`---\ntitle: Code map\nfiles: ${demoFiles.length}\n---`}</pre>
                <h4>Files</h4>
                <p className="wikilinks">
                  {demoFiles.map((f) => link(f.path))}
                </p>
              </>
            )}
            {note && (
              <>
                <pre>{`---\ntitle: ${baseName(note.path)}\npath: ${note.path}\nlanguage: ${note.lang}\nsymbol_count: ${note.symbols.length}\n---`}</pre>
                <h4>Imports</h4>
                <p className="wikilinks">
                  {note.imports.length ? (
                    note.imports.map(link)
                  ) : (
                    <small>None</small>
                  )}
                </p>
                <h4>Imported by</h4>
                <p className="wikilinks">
                  {importedBy(note.path).length ? (
                    importedBy(note.path).map(link)
                  ) : (
                    <small>None</small>
                  )}
                </p>
                <h4>Symbols</h4>
                <ul>
                  {note.symbols.map(([n, l]) => (
                    <li key={n}>
                      <code>{n}</code> <span>{l}</span>
                    </li>
                  ))}
                </ul>
              </>
            )}
            {page === MINE && (
              <>
                <h4>Yours</h4>
                <textarea
                  value={mine}
                  onChange={(e) => setMine(e.target.value)}
                  aria-label="My notes"
                  rows={4}
                />
                <small>
                  Reindex never touches this file. Edit it, then press Reindex.
                </small>
              </>
            )}
          </div>
        )}
      </div>
    </>
  );
}
