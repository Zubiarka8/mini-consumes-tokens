import { Currency, Money } from "../money";
import type { ExchangeRates, MoneyJson } from "../money";
import Order, { describe as describeEvent, OrderJson, OrderEvent, asOrderId } from "../order";
import type { OrderId } from "../order";

/**
 * Typed client for the warehouse HTTP API (see docs/api/reference.md). Every
 * call carries a bearer token and a request id; errors arrive as RFC 9457
 * problem details and are thrown as `ApiError`.
 */

export interface Problem {
  type: string;
  title: string;
  status: number;
  detail?: string;
  instance?: string;
  errors?: Record<string, string[]>;
}

export class ApiError extends Error {
  constructor(
    readonly problem: Problem,
    readonly requestId: string,
  ) {
    super(`${problem.status} ${problem.title}${problem.detail ? `: ${problem.detail}` : ""}`);
    this.name = "ApiError";
  }

  get isRetryable(): boolean {
    return this.problem.status === 429 || this.problem.status >= 500;
  }

  get isConflict(): boolean {
    return this.problem.status === 409;
  }
}

export interface Page<T> {
  items: T[];
  nextCursor?: string;
}

export interface ClientOptions {
  baseUrl: string;
  token: () => Promise<string>;
  fetch?: typeof fetch;
  retries?: number;
  onRequest?: (method: string, path: string, requestId: string) => void;
}

export interface PlaceOrderRequest {
  customerId: string;
  lines: { sku: string; quantity: number }[];
  shipping?: { method: "standard" | "express" | "pickup"; postalCode: string; country: string };
}

export type PlaceOrderResponse =
  | { status: "accepted"; order: OrderJson }
  | { status: "pending-approval"; order: OrderJson }
  | { status: "rejected"; problem: Problem };

export interface StockJson {
  sku: string;
  onHand: number;
  reserved: number;
  available: number;
}

/** Waits `ms`, or rejects early when the signal aborts. */
export function sleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(resolve, ms);
    signal?.addEventListener("abort", () => {
      clearTimeout(timer);
      reject(signal.reason);
    });
  });
}

/**
 * A token bucket shared by every client in the tab, so a burst of UI actions
 * stays under the API's per-client limit instead of collecting 429s.
 */
export class RateLimiter {
  #tokens: number;
  #last: number;

  constructor(
    private readonly capacity = 10,
    private readonly refillPerSecond = 10,
    private readonly now: () => number = () => performance.now(),
  ) {
    this.#tokens = capacity;
    this.#last = now();
  }

  #refill(): void {
    const t = this.now();
    this.#tokens = Math.min(this.capacity, this.#tokens + ((t - this.#last) / 1000) * this.refillPerSecond);
    this.#last = t;
  }

  async take(): Promise<void> {
    this.#refill();
    while (this.#tokens < 1) {
      await sleep(((1 - this.#tokens) / this.refillPerSecond) * 1000);
      this.#refill();
    }
    this.#tokens -= 1;
  }
}

/** Rejects with a timeout error when `promise` takes longer than `ms`. */
export function withTimeout<T>(promise: Promise<T>, ms: number, what = "request"): Promise<T> {
  return Promise.race([
    promise,
    new Promise<never>((_, reject) => setTimeout(() => reject(new Error(`${what} timed out after ${ms} ms`)), ms)),
  ]);
}

function backoff(attempt: number): number {
  const base = 200 * 2 ** attempt;
  return base + Math.floor(Math.random() * base * 0.2);
}

export class WarehouseClient implements ExchangeRates {
  readonly #options: Required<Omit<ClientOptions, "onRequest">> & Pick<ClientOptions, "onRequest">;
  #rates: Partial<Record<Currency, number>> = { [Currency.EUR]: 1 };

  constructor(options: ClientOptions) {
    this.#options = { fetch: globalThis.fetch.bind(globalThis), retries: 3, ...options };
  }

  // -------------------------------------------------------------------------
  // Transport
  // -------------------------------------------------------------------------

  async #request<T>(method: string, path: string, body?: unknown, signal?: AbortSignal): Promise<T> {
    const requestId = crypto.randomUUID();
    this.#options.onRequest?.(method, path, requestId);
    for (let attempt = 0; ; attempt++) {
      const response = await this.#options.fetch(`${this.#options.baseUrl}${path}`, {
        method,
        signal,
        headers: {
          Authorization: `Bearer ${await this.#options.token()}`,
          "Content-Type": "application/json",
          "X-Request-Id": requestId,
          ...(method === "POST" ? { "Idempotency-Key": requestId } : {}),
        },
        body: body === undefined ? undefined : JSON.stringify(body),
      });
      if (response.ok) {
        return response.status === 204 ? (undefined as T) : ((await response.json()) as T);
      }
      const problem = (await response.json().catch(() => ({ status: response.status, title: response.statusText, type: "about:blank" }))) as Problem;
      const error = new ApiError(problem, requestId);
      if (!error.isRetryable || attempt >= this.#options.retries) {
        throw error;
      }
      const retryAfter = Number(response.headers.get("Retry-After"));
      await sleep(retryAfter > 0 ? retryAfter * 1000 : backoff(attempt), signal);
    }
  }

  get<T>(path: string, signal?: AbortSignal): Promise<T> {
    return this.#request<T>("GET", path, undefined, signal);
  }

  post<T>(path: string, body: unknown): Promise<T> {
    return this.#request<T>("POST", path, body);
  }

  // -------------------------------------------------------------------------
  // Orders
  // -------------------------------------------------------------------------

  async placeOrder(request: PlaceOrderRequest): Promise<PlaceOrderResponse> {
    try {
      const order = await this.post<OrderJson>("/orders", request);
      return { status: order.state === "NEW" ? "pending-approval" : "accepted", order };
    } catch (e) {
      if (e instanceof ApiError && e.isConflict) {
        return { status: "rejected", problem: e.problem };
      }
      throw e;
    }
  }

  async order(number: string): Promise<Order> {
    return Order.fromJSON(await this.get<OrderJson>(`/orders/${encodeURIComponent(number)}`));
  }

  async cancel(number: string, reason: string): Promise<void> {
    await this.post(`/orders/${encodeURIComponent(number)}/cancel`, { reason });
  }

  async approve(number: string, approved: boolean, comment = ""): Promise<OrderJson> {
    return this.post<OrderJson>(`/orders/${encodeURIComponent(number)}/approve`, { approved, comment });
  }

  /** Iterates every page of orders, following `nextCursor`. */
  async *orders(limit = 50): AsyncGenerator<Order> {
    let cursor: string | undefined;
    do {
      const query = new URLSearchParams({ limit: String(limit), ...(cursor ? { cursor } : {}) });
      const page = await this.get<Page<OrderJson>>(`/orders?${query}`);
      for (const json of page.items) {
        yield Order.fromJSON(json);
      }
      cursor = page.nextCursor;
    } while (cursor);
  }

  /** Streams order events from the server-sent events endpoint. */
  events(orderId: OrderId, onEvent: (description: string) => void): () => void {
    const source = new EventSource(`${this.#options.baseUrl}/orders/${orderId}/events/stream`);
    source.onmessage = (message) => {
      const event = JSON.parse(message.data) as OrderEvent;
      onEvent(describeEvent(event));
    };
    return () => source.close();
  }

  // -------------------------------------------------------------------------
  // Stock and money
  // -------------------------------------------------------------------------

  async stock(...skus: string[]): Promise<Map<string, StockJson>> {
    const query = skus.map((s) => `sku=${encodeURIComponent(s)}`).join("&");
    const rows = await this.get<StockJson[]>(`/stock?${query}`);
    return new Map(rows.map((row) => [row.sku, row]));
  }

  async price(sku: string): Promise<Money> {
    const json = await this.get<{ price: MoneyJson }>(`/products/${encodeURIComponent(sku)}`);
    return Money.fromJSON(json.price);
  }

  async refreshRates(): Promise<void> {
    const rates = await this.get<Record<string, number>>("/rates/eur");
    this.#rates = { ...rates, [Currency.EUR]: 1 };
  }

  rate(from: Currency, to: Currency): number {
    const fromRate = this.#rates[from];
    const toRate = this.#rates[to];
    if (fromRate === undefined || toRate === undefined) {
      throw new Error(`no rate ${from}→${to}; call refreshRates() first`);
    }
    return toRate / fromRate;
  }
}

// ---------------------------------------------------------------------------
// Polling helpers
// ---------------------------------------------------------------------------

/** Polls an order until it reaches a terminal or shipped state. */
export async function waitUntilShipped(
  client: WarehouseClient,
  number: string,
  { intervalMs = 30_000, signal }: { intervalMs?: number; signal?: AbortSignal } = {},
): Promise<Order> {
  for (;;) {
    const order = await client.order(number);
    if (["SHIPPED", "INVOICED", "CANCELLED"].includes(order.state)) {
      return order;
    }
    await sleep(intervalMs, signal);
  }
}

/** Fetches every order and returns them indexed by number. */
export async function snapshot(client: WarehouseClient): Promise<Map<string, Order>> {
  const all = new Map<string, Order>();
  for await (const order of client.orders(200)) {
    all.set(order.number, order);
  }
  return all;
}

/** Validates a place-order request locally before sending it. */
export const validatePlaceOrder = (request: PlaceOrderRequest): string[] => {
  const errors: string[] = [];
  if (!request.customerId.trim()) errors.push("customerId is required");
  if (request.lines.length === 0) errors.push("at least one line");
  request.lines.forEach((line, i) => {
    if (!/^sku-\d{4}/.test(line.sku)) errors.push(`lines[${i}].sku is not a SKU`);
    if (line.quantity <= 0) errors.push(`lines[${i}].quantity must be positive`);
  });
  return errors;
};

export function createClient(baseUrl: string, token: string): WarehouseClient {
  return new WarehouseClient({ baseUrl, token: async () => token });
}

export { asOrderId };
