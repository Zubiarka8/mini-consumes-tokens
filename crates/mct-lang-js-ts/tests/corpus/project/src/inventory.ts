import type { Order, OrderId, Line } from "./order";
import { WarehouseClient, ApiError, sleep } from "./api/client";

/**
 * The back office's view of stock: a cache of the server's stock levels per
 * SKU and location, the cycle-count workflow and picking routes. Reservation
 * itself happens on the server; this module only mirrors and validates.
 */

export interface Location {
  readonly zone: string;
  readonly aisle: number;
  readonly level: number;
}

export function parseLocation(code: string): Location {
  const match = /^([A-Z]+(?:-BULK)?)-(\d+)-(\d+)$/.exec(code);
  if (!match) {
    throw new Error(`bad location: ${code}`);
  }
  const [, zone, aisle, level] = match;
  return { zone, aisle: Number(aisle), level: Number(level) };
}

export function formatLocation({ zone, aisle, level }: Location): string {
  return `${zone}-${String(aisle).padStart(2, "0")}-${level}`;
}

export const compareLocations = (a: Location, b: Location): number =>
  a.zone.localeCompare(b.zone) || a.aisle - b.aisle || a.level - b.level;

/** Sorts locations into a walking route: zone, aisle, snaking levels. */
export function pickingRoute(locations: Location[]): Location[] {
  return [...locations].sort((a, b) => {
    const byAisle = a.zone.localeCompare(b.zone) || a.aisle - b.aisle;
    if (byAisle !== 0) return byAisle;
    return a.aisle % 2 === 0 ? b.level - a.level : a.level - b.level;
  });
}

export interface StockLevel {
  sku: string;
  location: Location;
  onHand: number;
  reserved: number;
}

export const available = (level: StockLevel): number => level.onHand - level.reserved;

export class OutOfStockError extends Error {
  constructor(
    readonly sku: string,
    readonly requested: number,
    readonly available: number,
  ) {
    super(`${sku}: requested ${requested}, available ${available}`);
  }

  get shortfall(): number {
    return this.requested - this.available;
  }
}

// ---------------------------------------------------------------------------
// Cache
// ---------------------------------------------------------------------------

export interface StockSource {
  levels(sku: string): Promise<StockLevel[]>;
}

/** A store that notifies subscribers when a SKU's levels change. */
export abstract class Observable<K, V> {
  #subscribers = new Map<K, Set<(value: V) => void>>();

  subscribe(key: K, callback: (value: V) => void): () => void {
    let set = this.#subscribers.get(key);
    if (!set) {
      set = new Set();
      this.#subscribers.set(key, set);
    }
    set.add(callback);
    return () => set?.delete(callback);
  }

  protected notify(key: K, value: V): void {
    this.#subscribers.get(key)?.forEach((callback) => callback(value));
  }

  abstract get(key: K): V | undefined;
}

export class StockCache extends Observable<string, StockLevel[]> {
  readonly #levels = new Map<string, StockLevel[]>();
  readonly #loadedAt = new Map<string, number>();
  #inflight = new Map<string, Promise<StockLevel[]>>();

  constructor(
    private readonly source: StockSource,
    private readonly ttlMs = 15_000,
    private readonly now: () => number = Date.now,
  ) {
    super();
  }

  get(sku: string): StockLevel[] | undefined {
    return this.#levels.get(sku);
  }

  async levels(sku: string): Promise<StockLevel[]> {
    const loadedAt = this.#loadedAt.get(sku);
    if (loadedAt !== undefined && this.now() - loadedAt < this.ttlMs) {
      return this.#levels.get(sku) ?? [];
    }
    let pending = this.#inflight.get(sku);
    if (!pending) {
      pending = this.source.levels(sku).finally(() => this.#inflight.delete(sku));
      this.#inflight.set(sku, pending);
    }
    const levels = await pending;
    this.#store(sku, levels);
    return levels;
  }

  async available(sku: string): Promise<number> {
    const levels = await this.levels(sku);
    return levels.reduce((total, level) => total + available(level), 0);
  }

  /** Checks a basket against cached stock before placing the order. */
  async check(lines: readonly Line[]): Promise<OutOfStockError[]> {
    const results = await Promise.all(
      lines.map(async (line) => {
        const free = await this.available(line.sku);
        return free < line.quantity ? new OutOfStockError(line.sku, line.quantity, free) : undefined;
      }),
    );
    return results.filter((e): e is OutOfStockError => e !== undefined);
  }

  invalidate(sku?: string): void {
    if (sku === undefined) {
      this.#loadedAt.clear();
      return;
    }
    this.#loadedAt.delete(sku);
  }

  #store(sku: string, levels: StockLevel[]): void {
    this.#levels.set(sku, levels);
    this.#loadedAt.set(sku, this.now());
    this.notify(sku, levels);
  }
}

/** A stock source backed by the HTTP API. */
export function httpStockSource(client: WarehouseClient): StockSource {
  return {
    async levels(sku) {
      const rows = await client.get<{ location: string; onHand: number; reserved: number }[]>(
        `/stock/${encodeURIComponent(sku)}/locations`,
      );
      return rows.map((row) => ({ sku, location: parseLocation(row.location), onHand: row.onHand, reserved: row.reserved }));
    },
  };
}

// ---------------------------------------------------------------------------
// Cycle counts
// ---------------------------------------------------------------------------

export interface CycleCount {
  sku: string;
  location: Location;
  expected: number;
  counted: number;
  reason?: string;
}

export const difference = (count: CycleCount): number => count.counted - count.expected;

export function describeCount(count: CycleCount): string {
  const diff = difference(count);
  const where = `${count.sku} at ${formatLocation(count.location)}`;
  if (diff === 0) return `${where}: ok`;
  if (diff > 0) return `${where}: ${diff} over`;
  return `${where}: ${-diff} short (${count.reason ?? "no reason"})`;
}

/** Splits counts into those that need a supervisor and the rest. */
export function partitionForReview(counts: CycleCount[], tolerance = 2): [CycleCount[], CycleCount[]] {
  const review: CycleCount[] = [];
  const fine: CycleCount[] = [];
  for (const count of counts) {
    const needsReason = difference(count) !== 0 && !count.reason;
    (Math.abs(difference(count)) > tolerance || needsReason ? review : fine).push(count);
  }
  return [review, fine];
}

/** Posts counts one by one, retrying conflicts with a fresh expectation. */
export async function postCounts(client: WarehouseClient, cache: StockCache, counts: CycleCount[]): Promise<number> {
  let posted = 0;
  for (const count of counts) {
    for (let attempt = 0; attempt < 3; attempt++) {
      try {
        await client.post(`/stock/${encodeURIComponent(count.sku)}/counts`, {
          location: formatLocation(count.location),
          counted: count.counted,
          reason: count.reason,
        });
        posted++;
        cache.invalidate(count.sku);
        break;
      } catch (e) {
        if (!(e instanceof ApiError) || !e.isConflict) throw e;
        await sleep(250 * (attempt + 1));
      }
    }
  }
  return posted;
}

// ---------------------------------------------------------------------------
// Reservations as the server reports them
// ---------------------------------------------------------------------------

export interface Reservation {
  id: string;
  orderId: OrderId;
  sku: string;
  location: Location;
  quantity: number;
  expiresAt: Date;
}

export function expiringSoon(reservations: Reservation[], withinMs: number, now = new Date()): Reservation[] {
  return reservations.filter((r) => r.expiresAt.getTime() - now.getTime() < withinMs);
}

// ---------------------------------------------------------------------------
// Pick lists
// ---------------------------------------------------------------------------

export interface PickTask {
  orderNumber: string;
  sku: string;
  location: Location;
  quantity: number;
}

/**
 * Turns a wave of orders into pick tasks per zone, each zone's tasks in
 * walking order. A line is taken from the location with the most stock.
 */
export class PickListBuilder {
  readonly #tasks = new Map<string, PickTask[]>();
  readonly #missing: OutOfStockError[] = [];

  constructor(private readonly levels: Map<string, StockLevel[]>) {}

  add(order: Order): this {
    for (const line of order.lines) {
      const source = this.#bestLocation(line.sku, line.quantity);
      if (!source) {
        const free = Math.max(0, ...(this.levels.get(line.sku) ?? []).map(available));
        this.#missing.push(new OutOfStockError(line.sku, line.quantity, free));
        continue;
      }
      const zone = source.location.zone;
      const tasks = this.#tasks.get(zone) ?? [];
      tasks.push({ orderNumber: order.number, sku: line.sku, location: source.location, quantity: line.quantity });
      this.#tasks.set(zone, tasks);
    }
    return this;
  }

  #bestLocation(sku: string, quantity: number): StockLevel | undefined {
    return (this.levels.get(sku) ?? [])
      .filter((level) => available(level) >= quantity)
      .sort((a, b) => available(b) - available(a))[0];
  }

  build(): { byZone: Map<string, PickTask[]>; missing: OutOfStockError[] } {
    const byZone = new Map<string, PickTask[]>();
    for (const [zone, tasks] of this.#tasks) {
      const route = pickingRoute(tasks.map((t) => t.location));
      byZone.set(zone, route.flatMap((loc) => tasks.filter((t) => compareLocations(t.location, loc) === 0)));
    }
    return { byZone, missing: [...this.#missing] };
  }
}

/** Lines of `order` that cannot be fully served from a single location. */
export function splitLines(order: Order, levels: Map<string, StockLevel[]>): Line[] {
  return order.lines.filter((line) => {
    const best = Math.max(0, ...(levels.get(line.sku) ?? []).map(available));
    return best < line.quantity;
  });
}
