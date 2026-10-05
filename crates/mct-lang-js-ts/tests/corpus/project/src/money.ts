/**
 * Money for the warehouse front end: integer minor units plus an explicit
 * currency, so `0.1 + 0.2` never reaches a price (ADR-005). Formatting goes
 * through `Intl.NumberFormat`, cached per locale and currency.
 */

export enum Currency {
  EUR = "EUR",
  USD = "USD",
  GBP = "GBP",
  JPY = "JPY",
}

/** Minor units per currency (JPY has none). */
export const SCALE: Readonly<Record<Currency, number>> = {
  [Currency.EUR]: 2,
  [Currency.USD]: 2,
  [Currency.GBP]: 2,
  [Currency.JPY]: 0,
};

export type MoneyJson = { amount: string; currency: Currency };

export type RoundingMode = "half-even" | "half-up" | "down";

export interface Comparable<T> {
  compareTo(other: T): number;
  equals(other: T): boolean;
}

/** Thrown when two amounts of different currencies meet. */
export class CurrencyMismatchError extends Error {
  readonly expected: Currency;
  readonly actual: Currency;

  constructor(expected: Currency, actual: Currency) {
    super(`currency mismatch: expected ${expected}, got ${actual}`);
    this.name = "CurrencyMismatchError";
    this.expected = expected;
    this.actual = actual;
  }
}

/** Thrown by `Money.parse` for unreadable input. */
export class MoneyFormatError extends Error {
  constructor(input: string, reason: string) {
    super(`cannot parse "${input}" as money: ${reason}`);
    this.name = "MoneyFormatError";
  }
}

const formatters = new Map<string, Intl.NumberFormat>();

function formatterFor(locale: string, currency: Currency): Intl.NumberFormat {
  const key = `${locale}|${currency}`;
  let formatter = formatters.get(key);
  if (!formatter) {
    formatter = new Intl.NumberFormat(locale, { style: "currency", currency });
    formatters.set(key, formatter);
  }
  return formatter;
}

/** Banker's rounding of a non-negative or negative number to an integer. */
export function roundHalfEven(value: number): number {
  const floor = Math.floor(value);
  const diff = value - floor;
  if (diff > 0.5) return floor + 1;
  if (diff < 0.5) return floor;
  return floor % 2 === 0 ? floor : floor + 1;
}

export function round(value: number, mode: RoundingMode = "half-even"): number {
  switch (mode) {
    case "half-even":
      return roundHalfEven(value);
    case "half-up":
      return Math.round(value);
    case "down":
      return Math.trunc(value);
  }
}

export class Money implements Comparable<Money> {
  readonly #minor: number;
  readonly currency: Currency;

  private constructor(minor: number, currency: Currency) {
    if (!Number.isSafeInteger(minor)) {
      throw new RangeError(`amount out of range: ${minor}`);
    }
    this.#minor = minor;
    this.currency = currency;
  }

  // -------------------------------------------------------------------------
  // Factories
  // -------------------------------------------------------------------------

  static ofMinor(minor: number, currency: Currency): Money {
    return new Money(minor, currency);
  }

  static of(amount: number | string, currency: Currency, mode: RoundingMode = "half-even"): Money {
    const value = typeof amount === "string" ? Number(amount) : amount;
    if (Number.isNaN(value)) {
      throw new MoneyFormatError(String(amount), "not a number");
    }
    return new Money(round(value * 10 ** SCALE[currency], mode), currency);
  }

  static zero(currency: Currency): Money {
    return Money.ofMinor(0, currency);
  }

  /** Parses `"24.90 EUR"` or `"EUR 24.90"`. */
  static parse(text: string): Money {
    const parts = text.trim().split(/\s+/);
    if (parts.length !== 2) {
      throw new MoneyFormatError(text, "expected an amount and a currency");
    }
    const [a, b] = parts;
    const [amount, code] = /^[A-Z]{3}$/.test(a) ? [b, a] : [a, b];
    if (!isCurrency(code)) {
      throw new MoneyFormatError(text, `unknown currency ${code}`);
    }
    return Money.of(amount, code);
  }

  static fromJSON(json: MoneyJson): Money {
    return Money.of(json.amount, json.currency);
  }

  // -------------------------------------------------------------------------
  // Accessors
  // -------------------------------------------------------------------------

  get minor(): number {
    return this.#minor;
  }

  get amount(): number {
    return this.#minor / 10 ** SCALE[this.currency];
  }

  get isZero(): boolean {
    return this.#minor === 0;
  }

  get isNegative(): boolean {
    return this.#minor < 0;
  }

  // -------------------------------------------------------------------------
  // Arithmetic
  // -------------------------------------------------------------------------

  plus(other: Money): Money {
    this.#requireSameCurrency(other);
    return new Money(this.#minor + other.minor, this.currency);
  }

  minus(other: Money): Money {
    this.#requireSameCurrency(other);
    return new Money(this.#minor - other.minor, this.currency);
  }

  times(factor: number, mode: RoundingMode = "half-even"): Money {
    return new Money(round(this.#minor * factor, mode), this.currency);
  }

  negate(): Money {
    return new Money(-this.#minor, this.currency);
  }

  percent(rate: number): Money {
    return this.times(rate / 100);
  }

  /** Splits into `parts` shares that add up exactly. */
  split(parts: number): Money[] {
    if (parts <= 0 || !Number.isInteger(parts)) {
      throw new RangeError(`parts must be a positive integer: ${parts}`);
    }
    const share = Math.trunc(this.#minor / parts);
    const remainder = this.#minor - share * parts;
    return Array.from({ length: parts }, (_, i) =>
      Money.ofMinor(share + (i < remainder ? 1 : 0), this.currency),
    );
  }

  /** Splits proportionally to `weights`, leftover units to the first shares. */
  allocate(...weights: number[]): Money[] {
    const total = weights.reduce((sum, w) => sum + w, 0);
    if (total <= 0 || weights.some((w) => w < 0)) {
      throw new RangeError("weights must be non-negative and add up to more than zero");
    }
    const shares = weights.map((w) => Math.trunc((this.#minor * w) / total));
    let leftover = this.#minor - shares.reduce((a, b) => a + b, 0);
    for (let i = 0; leftover > 0; i = (i + 1) % shares.length) {
      shares[i] += 1;
      leftover -= 1;
    }
    return shares.map((minor) => Money.ofMinor(minor, this.currency));
  }

  #requireSameCurrency(other: Money): void {
    if (other.currency !== this.currency) {
      throw new CurrencyMismatchError(this.currency, other.currency);
    }
  }

  // -------------------------------------------------------------------------
  // Comparison and formatting
  // -------------------------------------------------------------------------

  compareTo(other: Money): number {
    this.#requireSameCurrency(other);
    return Math.sign(this.#minor - other.minor);
  }

  equals(other: Money): boolean {
    return other.currency === this.currency && other.minor === this.#minor;
  }

  max(other: Money): Money {
    return this.compareTo(other) >= 0 ? this : other;
  }

  format(locale = "es-ES"): string {
    return formatterFor(locale, this.currency).format(this.amount);
  }

  toJSON(): MoneyJson {
    return { amount: this.amount.toFixed(SCALE[this.currency]), currency: this.currency };
  }

  toString(): string {
    return `${this.amount.toFixed(SCALE[this.currency])} ${this.currency}`;
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

export function isCurrency(code: string): code is Currency {
  return (Object.values(Currency) as string[]).includes(code);
}

/** Sums amounts of one currency; an empty list gives zero. */
export const sum = (amounts: readonly Money[], currency: Currency): Money =>
  amounts.reduce((acc, m) => acc.plus(m), Money.zero(currency));

export const maxOf = (amounts: readonly Money[]): Money | undefined =>
  amounts.reduce<Money | undefined>((best, m) => (best === undefined ? m : best.max(m)), undefined);

export function vat(net: Money, ratePercent: number): Money {
  return net.percent(ratePercent);
}

export function gross(net: Money, ratePercent: number): Money {
  return net.plus(vat(net, ratePercent));
}

/** Distributes a discount over lines proportionally to their subtotals. */
export function distributeDiscount(subtotals: Money[], discount: Money): Money[] {
  if (subtotals.length === 0) return [];
  return discount.allocate(...subtotals.map((s) => s.minor));
}

/** A closed range of amounts, used by the catalog's price filter. */
export class MoneyRange {
  constructor(
    readonly min: Money,
    readonly max: Money,
  ) {
    if (min.compareTo(max) > 0) {
      throw new RangeError(`${min} > ${max}`);
    }
  }

  contains(value: Money): boolean {
    return value.compareTo(this.min) >= 0 && value.compareTo(this.max) <= 0;
  }

  get width(): Money {
    return this.max.minus(this.min);
  }

  static parse(text: string): MoneyRange {
    const [min, max] = text.split("..").map((part) => Money.parse(part));
    return new MoneyRange(min, max);
  }
}

/** Exchange rates per euro, refreshed by the API client. */
export interface ExchangeRates {
  rate(from: Currency, to: Currency): number;
}

export const identityRates: ExchangeRates = {
  rate(from, to) {
    if (from !== to) {
      throw new Error(`no rate ${from}→${to}`);
    }
    return 1;
  },
};

export function convert(amount: Money, to: Currency, rates: ExchangeRates): Money {
  if (amount.currency === to) return amount;
  return Money.of(amount.amount * rates.rate(amount.currency, to), to);
}

/** Formats a list of amounts for an audit line. */
export function auditLine(label: string, ...amounts: Money[]): string {
  return `${label}: ${amounts.map((m) => m.format("en-GB")).join(", ")}`;
}

/** Namespaced helpers kept for the legacy back office. */
export namespace Legacy {
  export function fromCents(cents: number): Money {
    return Money.ofMinor(cents, Currency.EUR);
  }

  export function toCents(money: Money): number {
    if (money.currency !== Currency.EUR) {
      throw new CurrencyMismatchError(Currency.EUR, money.currency);
    }
    return money.minor;
  }
}
