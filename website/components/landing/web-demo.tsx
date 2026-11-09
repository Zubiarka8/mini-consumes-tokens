"use client";
import { useState } from "react";
import {
  baseName,
  demoDeadCode,
  demoDuplicates,
  demoFiles,
  fileOf,
  importedBy,
} from "./demo-data";

const tabs = [
  "Dashboard",
  "Code",
  "Duplicates",
  "Dead code",
  "Search",
] as const;
type Tab = (typeof tabs)[number];
type Review = "Pending" | "Reviewed" | "Discarded";
const totalSymbols = demoFiles.reduce((n, f) => n + f.symbols.length, 0);

// Clickable stand-in for `mct-cli web`. All state lives in the browser.
export function WebDemo() {
  const [tab, setTab] = useState<Tab>("Dashboard");
  const [file, setFile] = useState(demoFiles[1].path);
  const [query, setQuery] = useState("");
  const [reviews, setReviews] = useState<Review[]>(
    demoDuplicates.map(() => "Pending"),
  );
  const pending = reviews.filter((r) => r === "Pending").length;
  const open = (path: string) => {
    setFile(path);
    setTab("Code");
  };
  const current = demoFiles.find((f) => f.path === file) ?? demoFiles[0];
  const needle = query.trim().toLowerCase();
  const hits = needle
    ? demoFiles.flatMap((f) =>
        f.symbols
          .filter(([n]) => n.toLowerCase().includes(needle))
          .map(([n, l]) => ({ n, l, path: f.path })),
      )
    : [];
  const mark = (i: number, to: Review) =>
    setReviews((all) =>
      all.map((r, k) => (k === i ? (r === to ? "Pending" : to) : r)),
    );
  return (
    <div className="mock-web">
      <nav aria-label="Dashboard sections">
        {tabs.map((x) => (
          <button
            key={x}
            type="button"
            aria-current={x === tab ? "page" : undefined}
            onClick={() => setTab(x)}
          >
            {x}
          </button>
        ))}
      </nav>
      <div className="mock-pane" key={tab}>
        {tab === "Dashboard" && (
          <>
            <div className="mock-tiles">
              <div>
                <small>Files</small>
                <b>{demoFiles.length}</b>
              </div>
              <div>
                <small>Symbols</small>
                <b>{totalSymbols}</b>
              </div>
              <button type="button" onClick={() => setTab("Duplicates")}>
                <small>To review</small>
                <b key={pending} className="tick">
                  {pending}
                </b>
              </button>
            </div>
            <p className="mock-h">Symbols per file (click to open)</p>
            <div className="mock-bars">
              {demoFiles.map((f) => (
                <button key={f.path} type="button" onClick={() => open(f.path)}>
                  <small>{baseName(f.path)}</small>
                  <span style={{ width: `${f.symbols.length * 40}%` }} />
                </button>
              ))}
            </div>
          </>
        )}
        {tab === "Code" && (
          <>
            <div className="mock-chips" role="group" aria-label="Files">
              {demoFiles.map((f) => (
                <button
                  key={f.path}
                  type="button"
                  aria-pressed={f.path === file}
                  onClick={() => setFile(f.path)}
                >
                  {baseName(f.path)}
                </button>
              ))}
            </div>
            <p className="mock-h">{current.path}</p>
            <ul className="mock-rows">
              {current.symbols.map(([n, l]) => (
                <li key={n}>
                  <code>{n}</code>
                  <em>{l}</em>
                </li>
              ))}
            </ul>
            <p className="mock-h">Depends on</p>
            <div className="mock-chips">
              {current.imports.length === 0 && <small>Nothing</small>}
              {current.imports.map((p) => (
                <button key={p} type="button" onClick={() => setFile(p)}>
                  {baseName(p)}
                </button>
              ))}
            </div>
            <p className="mock-h">Used by</p>
            <div className="mock-chips">
              {importedBy(current.path).length === 0 && <small>Nothing</small>}
              {importedBy(current.path).map((p) => (
                <button key={p} type="button" onClick={() => setFile(p)}>
                  {baseName(p)}
                </button>
              ))}
            </div>
          </>
        )}
        {tab === "Duplicates" && (
          <>
            <p className="mock-h">
              {pending} to review. Mark each pair, click again to undo.
            </p>
            <ul className="mock-rows">
              {demoDuplicates.map(([a, b], i) => (
                <li key={a}>
                  <code>
                    {a} ≈ {b}
                  </code>
                  <span className="mock-actions">
                    <em data-s={reviews[i]}>{reviews[i]}</em>
                    <button type="button" onClick={() => mark(i, "Reviewed")}>
                      Review
                    </button>
                    <button type="button" onClick={() => mark(i, "Discarded")}>
                      Discard
                    </button>
                  </span>
                </li>
              ))}
            </ul>
          </>
        )}
        {tab === "Dead code" && (
          <>
            <p className="mock-h">Symbols nothing references</p>
            <ul className="mock-rows">
              {demoDeadCode.map(([n, p]) => (
                <li key={n}>
                  <code>{n}</code>
                  <button type="button" onClick={() => open(p)}>
                    {baseName(p)}
                  </button>
                </li>
              ))}
            </ul>
          </>
        )}
        {tab === "Search" && (
          <>
            <input
              type="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Try: format, parse, cents"
              aria-label="Search symbols"
            />
            <ul className="mock-rows" aria-live="polite">
              {hits.map(({ n, l, path }) => (
                <li key={path + n}>
                  <code>{n}</code>
                  <button type="button" onClick={() => open(fileOf(n))}>
                    {baseName(path)} {l}
                  </button>
                </li>
              ))}
              {needle && hits.length === 0 && <li>No match</li>}
            </ul>
          </>
        )}
      </div>
    </div>
  );
}
