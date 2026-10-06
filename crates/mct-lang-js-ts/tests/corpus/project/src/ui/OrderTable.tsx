import React, { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import type { ReactNode } from "react";
import { Money, Currency, sum } from "../money";
import Order, { State, countByState, describe, packingSlip } from "../order";
import { WarehouseClient, ApiError, validatePlaceOrder } from "../api/client";
import { StockCache, httpStockSource, describeCount, pickingRoute, parseLocation } from "../inventory";

/**
 * The back office's order table: filters, sorting, selection with bulk
 * actions, a detail pane and the packing-slip preview. Data comes from
 * `WarehouseClient`; stock badges from a shared `StockCache`.
 */

type SortKey = "number" | "total" | "state";

interface Filters {
  state: State | "ALL";
  query: string;
  onlyLarge: boolean;
}

type Action =
  | { type: "loaded"; orders: Order[] }
  | { type: "failed"; error: string }
  | { type: "select"; number: string; additive: boolean }
  | { type: "clearSelection" }
  | { type: "sort"; key: SortKey };

interface TableState {
  orders: Order[];
  error?: string;
  loading: boolean;
  selected: Set<string>;
  sortKey: SortKey;
  ascending: boolean;
}

const initialState: TableState = {
  orders: [],
  loading: true,
  selected: new Set(),
  sortKey: "number",
  ascending: true,
};

export function reducer(state: TableState, action: Action): TableState {
  switch (action.type) {
    case "loaded":
      return { ...state, orders: action.orders, loading: false, error: undefined };
    case "failed":
      return { ...state, loading: false, error: action.error };
    case "select": {
      const selected = new Set(action.additive ? state.selected : []);
      if (selected.has(action.number)) {
        selected.delete(action.number);
      } else {
        selected.add(action.number);
      }
      return { ...state, selected };
    }
    case "clearSelection":
      return { ...state, selected: new Set() };
    case "sort":
      return {
        ...state,
        sortKey: action.key,
        ascending: state.sortKey === action.key ? !state.ascending : true,
      };
  }
}

const LARGE_ORDER = Money.of(5000, Currency.EUR);

function compareBy(key: SortKey): (a: Order, b: Order) => number {
  switch (key) {
    case "number":
      return (a, b) => a.number.localeCompare(b.number);
    case "total":
      return (a, b) => a.total.compareTo(b.total);
    case "state":
      return (a, b) => a.state.localeCompare(b.state);
  }
}

export function applyFilters(orders: Order[], filters: Filters): Order[] {
  const query = filters.query.trim().toLowerCase();
  return orders.filter(
    (o) =>
      (filters.state === "ALL" || o.state === filters.state) &&
      (!filters.onlyLarge || o.isLarge(LARGE_ORDER)) &&
      (!query || o.number.toLowerCase().includes(query) || o.customerId.toLowerCase().includes(query)),
  );
}

// ---------------------------------------------------------------------------
// Hooks
// ---------------------------------------------------------------------------

export function useOrders(client: WarehouseClient) {
  const [state, dispatch] = useReducer(reducer, initialState);

  const reload = useCallback(async () => {
    try {
      const orders: Order[] = [];
      for await (const order of client.orders(100)) {
        orders.push(order);
      }
      dispatch({ type: "loaded", orders });
    } catch (e) {
      dispatch({ type: "failed", error: e instanceof ApiError ? e.message : String(e) });
    }
  }, [client]);

  useEffect(() => {
    void reload();
  }, [reload]);

  return { state, dispatch, reload };
}

export function useStock(cache: StockCache, sku: string): number | undefined {
  const [free, setFree] = useState<number>();
  useEffect(() => {
    let cancelled = false;
    cache.available(sku).then((n) => {
      if (!cancelled) setFree(n);
    });
    const unsubscribe = cache.subscribe(sku, () => {
      cache.available(sku).then(setFree);
    });
    return () => {
      cancelled = true;
      unsubscribe();
    };
  }, [cache, sku]);
  return free;
}

function useDebounced<T>(value: T, ms: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setDebounced(value), ms);
    return () => clearTimeout(timer);
  }, [value, ms]);
  return debounced;
}

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

export const StatusPill = ({ state }: { state: State }) => (
  <span className={`pill pill--${state.toLowerCase()}`}>{state}</span>
);

function StockBadge({ cache, sku }: { cache: StockCache; sku: string }) {
  const free = useStock(cache, sku);
  if (free === undefined) return <span className="badge">…</span>;
  return <span className={free < 10 ? "badge badge--danger" : "badge"}>{free}</span>;
}

function Toolbar({ filters, onChange, children }: { filters: Filters; onChange: (f: Filters) => void; children?: ReactNode }) {
  return (
    <div className="toolbar">
      <select value={filters.state} onChange={(e) => onChange({ ...filters, state: e.target.value as Filters["state"] })}>
        <option value="ALL">Todos</option>
        {Object.values(State).map((s) => (
          <option key={s} value={s}>
            {s}
          </option>
        ))}
      </select>
      <input
        type="search"
        placeholder="Buscar por número o cliente…"
        value={filters.query}
        onChange={(e) => onChange({ ...filters, query: e.target.value })}
      />
      <label>
        <input type="checkbox" checked={filters.onlyLarge} onChange={(e) => onChange({ ...filters, onlyLarge: e.target.checked })} />
        Solo pedidos grandes
      </label>
      {children}
    </div>
  );
}

/** The detail pane of the selected order, with its history streamed live. */
export function OrderDetail({ client, order }: { client: WarehouseClient; order: Order }) {
  const [history, setHistory] = useState<string[]>([]);

  useEffect(() => {
    setHistory([]);
    const stop = client.events(order.id, (line) => setHistory((h) => [...h, line]));
    return stop;
  }, [client, order.id]);

  return (
    <aside className="order-detail">
      <h2>
        {order.number} <StatusPill state={order.state} />
      </h2>
      <ul>
        {order.lines.map((line) => (
          <li key={line.number}>
            {line.quantity} × {line.sku} — {line.unitPrice.format()}
          </li>
        ))}
      </ul>
      <p className="total">Total: {order.total.format()}</p>
      <ol className="history">{history.map((h, i) => <li key={i}>{h}</li>)}</ol>
    </aside>
  );
}

export function OrderTable({ client, cache }: { client: WarehouseClient; cache: StockCache }) {
  const { state, dispatch, reload } = useOrders(client);
  const [filters, setFilters] = useState<Filters>({ state: "ALL", query: "", onlyLarge: false });
  const query = useDebounced(filters.query, 300);
  const preview = useRef<HTMLPreElement>(null);

  const visible = useMemo(() => {
    const filtered = applyFilters(state.orders, { ...filters, query });
    const sorted = filtered.sort(compareBy(state.sortKey));
    return state.ascending ? sorted : sorted.reverse();
  }, [state.orders, state.sortKey, state.ascending, filters, query]);

  const counts = useMemo(() => countByState(state.orders), [state.orders]);
  const selectedTotal = useMemo(
    () => sum(visible.filter((o) => state.selected.has(o.number)).map((o) => o.total), Currency.EUR),
    [visible, state.selected],
  );

  const cancelSelected = async () => {
    for (const number of state.selected) {
      await client.cancel(number, "cancelled from the back office");
    }
    dispatch({ type: "clearSelection" });
    await reload();
  };

  const showSlip = (order: Order) => {
    if (preview.current) {
      preview.current.textContent = packingSlip(order);
    }
  };

  if (state.loading) return <p>Cargando pedidos…</p>;
  if (state.error) return <p role="alert">{state.error}</p>;

  return (
    <section className="orders">
      <Toolbar filters={filters} onChange={setFilters}>
        <span className="counts">
          {[...counts].map(([s, n]) => `${s}: ${n}`).join(" · ")}
        </span>
      </Toolbar>
      <table>
        <thead>
          <tr>
            <th onClick={() => dispatch({ type: "sort", key: "number" })}>Número</th>
            <th onClick={() => dispatch({ type: "sort", key: "state" })}>Estado</th>
            <th onClick={() => dispatch({ type: "sort", key: "total" })}>Total</th>
            <th>Stock</th>
          </tr>
        </thead>
        <tbody>
          {visible.map((order) => (
            <tr
              key={order.number}
              className={state.selected.has(order.number) ? "is-selected" : undefined}
              onClick={(e) => dispatch({ type: "select", number: order.number, additive: e.metaKey || e.ctrlKey })}
              onDoubleClick={() => showSlip(order)}
            >
              <td>{order.number}</td>
              <td>
                <StatusPill state={order.state} />
              </td>
              <td className="num">{order.total.format()}</td>
              <td>
                {order.lines.map((line) => (
                  <StockBadge key={line.sku} cache={cache} sku={line.sku} />
                ))}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {state.selected.size > 0 && (
        <div className="bulk-actions">
          {state.selected.size} seleccionados · {selectedTotal.format()}
          <button onClick={cancelSelected}>Cancelar</button>
          <button onClick={() => dispatch({ type: "clearSelection" })}>✕</button>
        </div>
      )}
      <pre ref={preview} className="slip-preview" />
    </section>
  );
}

/** Wires the table with a client and an HTTP-backed stock cache. */
export function createOrderTable(baseUrl: string, token: string) {
  const client = new WarehouseClient({ baseUrl, token: async () => token });
  const cache = new StockCache(httpStockSource(client));
  return () => <OrderTable client={client} cache={cache} />;
}

export { describe, describeCount, pickingRoute, parseLocation, validatePlaceOrder };
