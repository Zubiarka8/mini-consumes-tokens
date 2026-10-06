import React, { Component, useEffect, useMemo, useRef, useState } from "react";
import { PickListBuilder, StockCache, formatLocation, httpStockSource, parseLocation, pickingRoute } from "../inventory";
import * as orders from "../order";
import { ApiError, createClient } from "../api/client";

/**
 * The picking board shown on the warehouse handhelds, in plain JavaScript:
 * a picker scans a location barcode, then each item, and the board walks
 * them through the route zone by zone. Pick lists come from
 * `PickListBuilder`; stock from the shared `StockCache`.
 */

const SCAN_PREFIX_LOCATION = "L:";
const SCAN_PREFIX_ITEM = "I:";
const IDLE_MS = 90_000;

/** Classifies a raw barcode as a location, an item or noise. */
export function classifyScan(raw) {
  const code = String(raw || "").trim();
  if (code.startsWith(SCAN_PREFIX_LOCATION)) {
    return { kind: "location", location: parseLocation(code.slice(SCAN_PREFIX_LOCATION.length)) };
  }
  if (code.startsWith(SCAN_PREFIX_ITEM)) {
    const [sku, quantity = "1"] = code.slice(SCAN_PREFIX_ITEM.length).split("*");
    return { kind: "item", sku, quantity: Number.parseInt(quantity, 10) || 1 };
  }
  return { kind: "noise", code };
}

/**
 * One picker's walk through a zone. Not React state: the session lives as
 * long as the handheld is logged in and survives re-renders.
 */
export class PickSession {
  static #nextId = 1;
  static sessions = new Map();

  id = PickSession.#nextId++;
  picked = [];
  #tasks = [];
  #position = 0;
  #listeners = new Set();

  constructor(zone, tasks) {
    this.zone = zone;
    this.#tasks = pickingRoute(tasks.map((task) => task.location)).map((location) =>
      tasks.find((task) => formatLocation(task.location) === formatLocation(location)),
    );
    PickSession.sessions.set(this.id, this);
  }

  static forZone(zone) {
    for (const session of PickSession.sessions.values()) {
      if (session.zone === zone && !session.done) {
        return session;
      }
    }
    return undefined;
  }

  get current() {
    return this.#tasks[this.#position];
  }

  get done() {
    return this.#position >= this.#tasks.length;
  }

  get progress() {
    return { done: this.#position, total: this.#tasks.length };
  }

  /** Bound as a field so it can be handed to the scanner as-is. */
  handleScan = (raw) => {
    const scan = classifyScan(raw);
    const task = this.current;
    if (!task || scan.kind === "noise") {
      return this.#emit({ type: "ignored", raw });
    }
    if (scan.kind === "location") {
      const expected = formatLocation(task.location);
      const actual = formatLocation(scan.location);
      return this.#emit(actual === expected ? { type: "atLocation", task } : { type: "wrongLocation", expected, actual });
    }
    if (scan.sku !== task.sku) {
      return this.#emit({ type: "wrongItem", expected: task.sku, actual: scan.sku });
    }
    this.picked.push({ ...task, quantity: Math.min(scan.quantity, task.quantity) });
    this.#advance();
  };

  skip = (reason) => {
    const task = this.current;
    if (task) {
      this.#emit({ type: "skipped", task, reason });
      this.#advance();
    }
  };

  onChange(listener) {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  #advance() {
    this.#position++;
    this.#emit(this.done ? { type: "finished", picked: this.picked } : { type: "next", task: this.current });
  }

  #emit(event) {
    this.#listeners.forEach((listener) => listener(event, this.progress));
  }

  close() {
    PickSession.sessions.delete(this.id);
    this.#listeners.clear();
  }
}

/** Keeps a scanning error from blanking the whole handheld. */
export class ScanErrorBoundary extends Component {
  state = { error: null };

  static getDerivedStateFromError(error) {
    return { error };
  }

  componentDidCatch(error, info) {
    this.props.onError?.(error, info.componentStack);
  }

  retry = () => {
    this.setState({ error: null });
  };

  render() {
    if (this.state.error) {
      const message = this.state.error instanceof ApiError ? this.state.error.message : "Unexpected error";
      return (
        <div role="alert" className="scan-error">
          <p>{message}</p>
          <button onClick={this.retry}>Retry</button>
        </div>
      );
    }
    return this.props.children;
  }
}

/** Wraps the browser's keyboard-wedge scanner: digits arrive as keystrokes. */
export function createScanner(target, onScan) {
  let buffer = "";
  let lastKey = 0;

  function onKeyDown(event) {
    const now = Date.now();
    if (now - lastKey > 50) {
      buffer = "";
    }
    lastKey = now;
    if (event.key === "Enter") {
      if (buffer.length > 2) {
        onScan(buffer);
      }
      buffer = "";
      return;
    }
    if (event.key.length === 1) {
      buffer += event.key;
    }
  }

  return {
    start() {
      target.addEventListener("keydown", onKeyDown);
    },
    stop() {
      target.removeEventListener("keydown", onKeyDown);
    },
    get pending() {
      return buffer;
    },
  };
}

function useIdleLogout(onIdle) {
  const timer = useRef(0);
  useEffect(() => {
    const reset = () => {
      clearTimeout(timer.current);
      timer.current = setTimeout(onIdle, IDLE_MS);
    };
    reset();
    window.addEventListener("pointerdown", reset);
    window.addEventListener("keydown", reset);
    return () => {
      clearTimeout(timer.current);
      window.removeEventListener("pointerdown", reset);
      window.removeEventListener("keydown", reset);
    };
  }, [onIdle]);
}

function usePickSession(session) {
  const [state, setState] = useState(() => ({ event: { type: "next", task: session.current }, progress: session.progress }));
  useEffect(() => session.onChange((event, progress) => setState({ event, progress })), [session]);
  useEffect(() => {
    const scanner = createScanner(window, session.handleScan);
    scanner.start();
    return () => scanner.stop();
  }, [session]);
  return state;
}

function TaskCard({ task, stock }) {
  const onHand = useMemo(() => (stock || []).reduce((total, level) => total + level.onHand, 0), [stock]);
  return (
    <section className="task-card">
      <h2>{formatLocation(task.location)}</h2>
      <p className="sku">{task.sku}</p>
      <p className="quantity">
        Pick {task.quantity} <small>({onHand} on hand)</small>
      </p>
    </section>
  );
}

function Feedback({ event }) {
  switch (event.type) {
    case "wrongLocation":
      return <p className="feedback error">Wrong shelf: {event.actual}, go to {event.expected}</p>;
    case "wrongItem":
      return <p className="feedback error">Wrong item: {event.actual}, expected {event.expected}</p>;
    case "skipped":
      return <p className="feedback warn">Skipped {event.task.sku}: {event.reason}</p>;
    case "atLocation":
      return <p className="feedback ok">Now scan the item</p>;
    default:
      return null;
  }
}

function ProgressBar({ done, total }) {
  const percent = total === 0 ? 100 : Math.round((done / total) * 100);
  return (
    <div className="progress" role="progressbar" aria-valuenow={percent} aria-valuemin={0} aria-valuemax={100}>
      <div className="progress__fill" style={{ width: `${percent}%` }} />
      <span>
        {done}/{total}
      </span>
    </div>
  );
}

function Board({ session, cache, onFinish }) {
  const { event, progress } = usePickSession(session);
  const [stock, setStock] = useState(null);
  const task = session.current;

  useEffect(() => {
    if (!task) {
      return undefined;
    }
    let cancelled = false;
    cache.levels(task.sku).then((levels) => {
      if (!cancelled) {
        setStock(levels);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [cache, task]);

  useEffect(() => {
    if (event.type === "finished") {
      onFinish(event.picked);
    }
  }, [event, onFinish]);

  if (!task) {
    return <p className="done">Zone {session.zone} picked</p>;
  }
  return (
    <>
      <ProgressBar {...progress} />
      <TaskCard task={task} stock={stock} />
      <Feedback event={event} />
      <button className="skip" onClick={() => session.skip("not found")}>
        Not on the shelf
      </button>
    </>
  );
}

/** Builds the zone's session from the open orders, or reuses a running one. */
export async function openSession(client, cache, zone) {
  const existing = PickSession.forZone(zone);
  if (existing) {
    return existing;
  }
  const open = [];
  for await (const order of client.orders(200)) {
    if (order.state === orders.State.Paid) {
      open.push(order);
    }
  }
  const levels = new Map();
  for (const sku of new Set(open.flatMap((order) => order.lines.map((line) => line.sku)))) {
    levels.set(sku, await cache.levels(sku));
  }
  const builder = new PickListBuilder(levels);
  open.forEach((order) => builder.add(order));
  const { byZone } = builder.build();
  return new PickSession(zone, byZone.get(zone) || []);
}

export default function PickBoard({ baseUrl, token, zone, onLogout }) {
  const client = useMemo(() => createClient(baseUrl, token), [baseUrl, token]);
  const cache = useMemo(() => new StockCache(httpStockSource(client)), [client]);
  const [session, setSession] = useState(null);
  useIdleLogout(onLogout);

  useEffect(() => {
    openSession(client, cache, zone).then(setSession);
  }, [client, cache, zone]);

  async function finish(picked) {
    picked.forEach((task) => cache.invalidate(task.sku));
    session.close();
    setSession(await openSession(client, cache, zone));
  }

  return (
    <ScanErrorBoundary onError={(error) => console.error("pick board", error)}>
      {session ? <Board session={session} cache={cache} onFinish={finish} /> : <p>Loading zone {zone}…</p>}
    </ScanErrorBoundary>
  );
}
