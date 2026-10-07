import { Currency, Money, sum, CurrencyMismatchError } from "./money";
import type { MoneyJson } from "./money";
import * as clock from "./clock";

/**
 * The order aggregate as the back office sees it: lines, state machine and
 * the events the server streams back. Mirrors the server's `Order` so the
 * UI can validate transitions before calling the API.
 */

export type OrderId = string & { readonly __brand: "OrderId" };

export const asOrderId = (value: string): OrderId => value as OrderId;

export enum State {
  New = "NEW",
  Reserved = "RESERVED",
  Paid = "PAID",
  Picked = "PICKED",
  Shipped = "SHIPPED",
  Invoiced = "INVOICED",
  Cancelled = "CANCELLED",
}

const NEXT: Record<State, readonly State[]> = {
  [State.New]: [State.Reserved, State.Cancelled],
  [State.Reserved]: [State.Paid, State.Cancelled],
  [State.Paid]: [State.Picked, State.Cancelled],
  [State.Picked]: [State.Shipped, State.Cancelled],
  [State.Shipped]: [State.Invoiced],
  [State.Invoiced]: [],
  [State.Cancelled]: [],
};

export function canMoveTo(from: State, to: State): boolean {
  return NEXT[from].includes(to);
}

export function isTerminal(state: State): boolean {
  return NEXT[state].length === 0;
}

export interface Line {
  readonly number: number;
  readonly sku: string;
  readonly quantity: number;
  readonly unitPrice: Money;
}

export interface LineJson {
  number: number;
  sku: string;
  quantity: number;
  unitPrice: MoneyJson;
}

export const subtotal = (line: Line): Money => line.unitPrice.times(line.quantity);

// ---------------------------------------------------------------------------
// Events: a discriminated union on `type`
// ---------------------------------------------------------------------------

interface EventBase {
  readonly orderId: OrderId;
  readonly at: Date;
}

export interface Placed extends EventBase {
  readonly type: "placed";
  readonly customerId: string;
  readonly lines: readonly Line[];
}

export interface Reserved extends EventBase {
  readonly type: "reserved";
  readonly reservationIds: readonly string[];
}

export interface Paid extends EventBase {
  readonly type: "paid";
  readonly paymentId: string;
  readonly amount: Money;
}

export interface Shipped extends EventBase {
  readonly type: "shipped";
  readonly carrier: string;
  readonly trackingNumber: string;
}

export interface Cancelled extends EventBase {
  readonly type: "cancelled";
  readonly reason: string;
}

export type OrderEvent = Placed | Reserved | Paid | Shipped | Cancelled;

export type EventOfType<T extends OrderEvent["type"]> = Extract<OrderEvent, { type: T }>;

export function isEvent<T extends OrderEvent["type"]>(event: OrderEvent, type: T): event is EventOfType<T> {
  return event.type === type;
}

export function describe(event: OrderEvent): string {
  switch (event.type) {
    case "placed":
      return `placed with ${event.lines.length} line(s)`;
    case "reserved":
      return `reserved ${event.reservationIds.length} reservation(s)`;
    case "paid":
      return `paid ${event.amount.toString()}`;
    case "shipped":
      return `shipped with ${event.carrier} (${event.trackingNumber})`;
    case "cancelled":
      return `cancelled: ${event.reason}`;
    default: {
      const unreachable: never = event;
      return unreachable;
    }
  }
}

export class IllegalTransitionError extends Error {
  constructor(
    readonly from: State,
    readonly to: State,
  ) {
    super(`cannot move an order from ${from} to ${to}`);
  }
}

// ---------------------------------------------------------------------------
// Aggregate
// ---------------------------------------------------------------------------

type Listener = (event: OrderEvent) => void;

export class Order {
  #state: State = State.New;
  readonly #lines: Line[] = [];
  readonly #pending: OrderEvent[] = [];
  #listeners: Listener[] = [];
  trackingNumber?: string;

  private constructor(
    readonly id: OrderId,
    readonly number: string,
    readonly customerId: string,
    readonly currency: Currency,
  ) {}

  static place(number: string, customerId: string, currency: Currency, lines: Line[]): Order {
    if (lines.length === 0) {
      throw new Error("an order needs at least one line");
    }
    const order = new Order(asOrderId(crypto.randomUUID()), number, customerId, currency);
    lines.forEach((line) => order.#addLine(line));
    order.#record({ type: "placed", orderId: order.id, at: clock.now(), customerId, lines: [...lines] });
    return order;
  }

  static fromJSON(json: OrderJson): Order {
    const order = new Order(asOrderId(json.id), json.number, json.customerId, json.currency);
    json.lines.forEach((l) =>
      order.#addLine({ number: l.number, sku: l.sku, quantity: l.quantity, unitPrice: Money.fromJSON(l.unitPrice) }),
    );
    order.#state = json.state;
    return order;
  }

  get state(): State {
    return this.#state;
  }

  get lines(): readonly Line[] {
    return this.#lines;
  }

  get total(): Money {
    return sum(this.#lines.map(subtotal), this.currency);
  }

  isLarge(threshold: Money): boolean {
    return this.total.compareTo(threshold) > 0;
  }

  // -------------------------------------------------------------------------
  // Commands
  // -------------------------------------------------------------------------

  reserve(reservationIds: string[]): Reserved {
    this.#moveTo(State.Reserved);
    return this.#record({ type: "reserved", orderId: this.id, at: clock.now(), reservationIds });
  }

  pay(paymentId: string, amount: Money): Paid {
    if (!amount.equals(this.total)) {
      throw new Error(`paid ${amount} for a total of ${this.total}`);
    }
    this.#moveTo(State.Paid);
    return this.#record({ type: "paid", orderId: this.id, at: clock.now(), paymentId, amount });
  }

  ship(carrier: string, trackingNumber: string): Shipped {
    this.#moveTo(State.Shipped);
    this.trackingNumber = trackingNumber;
    return this.#record({ type: "shipped", orderId: this.id, at: clock.now(), carrier, trackingNumber });
  }

  cancel(reason: string): Cancelled {
    this.#moveTo(State.Cancelled);
    return this.#record({ type: "cancelled", orderId: this.id, at: clock.now(), reason });
  }

  onEvent(listener: Listener): () => void {
    this.#listeners.push(listener);
    return () => {
      this.#listeners = this.#listeners.filter((l) => l !== listener);
    };
  }

  drainEvents(): OrderEvent[] {
    return this.#pending.splice(0, this.#pending.length);
  }

  #moveTo(target: State): void {
    if (!canMoveTo(this.#state, target)) {
      throw new IllegalTransitionError(this.#state, target);
    }
    this.#state = target;
  }

  #record<E extends OrderEvent>(event: E): E {
    this.#pending.push(event);
    this.#listeners.forEach((listener) => listener(event));
    return event;
  }

  #addLine(line: Line): void {
    if (line.unitPrice.currency !== this.currency) {
      throw new CurrencyMismatchError(this.currency, line.unitPrice.currency);
    }
    this.#lines.push(line);
  }

  toJSON(): OrderJson {
    return {
      id: this.id,
      number: this.number,
      customerId: this.customerId,
      currency: this.currency,
      state: this.#state,
      lines: this.#lines.map((l) => ({ ...l, unitPrice: l.unitPrice.toJSON() })),
    };
  }
}

export interface OrderJson {
  id: string;
  number: string;
  customerId: string;
  currency: Currency;
  state: State;
  lines: LineJson[];
}

// ---------------------------------------------------------------------------
// Collections of orders
// ---------------------------------------------------------------------------

export function countByState(orders: Iterable<Order>): Map<State, number> {
  const counts = new Map<State, number>();
  for (const order of orders) {
    counts.set(order.state, (counts.get(order.state) ?? 0) + 1);
  }
  return counts;
}

export function largest(orders: readonly Order[], n: number): Order[] {
  return [...orders].sort((a, b) => b.total.compareTo(a.total)).slice(0, n);
}

/** Orders waiting for payment longer than `limitMinutes`. */
export function stalePayments(orders: readonly Order[], placedAt: (o: Order) => Date, limitMinutes = 30): Order[] {
  const now = clock.now().getTime();
  function waitedMinutes(order: Order): number {
    return (now - placedAt(order).getTime()) / 60_000;
  }
  return orders.filter((o) => o.state === State.Reserved && waitedMinutes(o) > limitMinutes);
}

/** Groups orders into pages for the back-office table. */
export function* pages<T>(items: readonly T[], size: number): Generator<T[], void, undefined> {
  for (let i = 0; i < items.length; i += size) {
    yield items.slice(i, i + size);
  }
}

/** Renders a packing slip as plain text. */
export function packingSlip(order: Order, width = 48): string {
  const rule = () => "-".repeat(width);
  const row = (left: string, right: string) => left.padEnd(width - right.length) + right;
  const out = [rule(), row("Albarán", order.number), rule()];
  for (const line of order.lines) {
    out.push(row(`${line.quantity} × ${line.sku}`, subtotal(line).format()));
  }
  out.push(rule(), row("Total", order.total.format()));
  return out.join("\n");
}

export default Order;
