package com.example.warehouse.money;

import java.math.BigDecimal;
import java.math.RoundingMode;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.List;
import java.util.Locale;
import java.util.Objects;
import java.text.NumberFormat;
import java.util.function.BinaryOperator;
import java.util.stream.Collector;
import java.util.stream.Collectors;

import static java.util.Objects.requireNonNull;

/**
 * An amount of money in one currency, as a decimal with the currency's scale.
 *
 * <p>Arithmetic between different currencies is rejected; conversion goes
 * through {@link ExchangeRates}. Rounding is banker's rounding applied once,
 * when a {@code Money} is created from an arbitrary decimal (ADR-005).
 */
public final class Money implements Comparable<Money> {

    /** ISO 4217 currencies the warehouse trades in, with their minor units. */
    public enum Currency {
        EUR(2, "€"),
        USD(2, "$"),
        GBP(2, "£"),
        JPY(0, "¥");

        private final int scale;
        private final String symbol;

        Currency(int scale, String symbol) {
            this.scale = scale;
            this.symbol = symbol;
        }

        public int scale() {
            return scale;
        }

        public String symbol() {
            return symbol;
        }

        public static Currency parse(String code) {
            for (Currency c : values()) {
                if (c.name().equalsIgnoreCase(code.trim())) {
                    return c;
                }
            }
            throw new IllegalArgumentException("unsupported currency: " + code);
        }
    }

    /** How a remainder is distributed when an amount is split. */
    public enum RoundingPolicy {
        HALF_EVEN {
            @Override
            RoundingMode mode() {
                return RoundingMode.HALF_EVEN;
            }
        },
        HALF_UP {
            @Override
            RoundingMode mode() {
                return RoundingMode.HALF_UP;
            }
        },
        DOWN {
            @Override
            RoundingMode mode() {
                return RoundingMode.DOWN;
            }
        };

        abstract RoundingMode mode();
    }

    private static final RoundingPolicy DEFAULT_ROUNDING = RoundingPolicy.HALF_EVEN;

    private final BigDecimal amount;
    private final Currency currency;

    private Money(BigDecimal amount, Currency currency) {
        this.currency = requireNonNull(currency, "currency");
        this.amount = requireNonNull(amount, "amount").setScale(currency.scale(), DEFAULT_ROUNDING.mode());
    }

    // ------------------------------------------------------------------
    // Factories
    // ------------------------------------------------------------------

    public static Money of(BigDecimal amount, Currency currency) {
        return new Money(amount, currency);
    }

    public static Money of(String amount, Currency currency) {
        return of(new BigDecimal(amount), currency);
    }

    public static Money of(long minorUnits, Currency currency) {
        return of(BigDecimal.valueOf(minorUnits, currency.scale()), currency);
    }

    public static Money zero(Currency currency) {
        return of(BigDecimal.ZERO, currency);
    }

    /** Parses {@code "24.90 EUR"} or {@code "EUR 24.90"}. */
    public static Money parse(String text) {
        String[] parts = text.trim().split("\\s+");
        if (parts.length != 2) {
            throw new MoneyFormatException(text, "expected an amount and a currency");
        }
        try {
            if (Character.isLetter(parts[0].charAt(0))) {
                return of(parts[1], Currency.parse(parts[0]));
            }
            return of(parts[0], Currency.parse(parts[1]));
        } catch (NumberFormatException e) {
            throw new MoneyFormatException(text, e.getMessage());
        }
    }

    // ------------------------------------------------------------------
    // Accessors
    // ------------------------------------------------------------------

    public BigDecimal amount() {
        return amount;
    }

    public Currency currency() {
        return currency;
    }

    public long minorUnits() {
        return amount.movePointRight(currency.scale()).longValueExact();
    }

    public boolean isZero() {
        return amount.signum() == 0;
    }

    public boolean isNegative() {
        return amount.signum() < 0;
    }

    public boolean isPositive() {
        return amount.signum() > 0;
    }

    // ------------------------------------------------------------------
    // Arithmetic
    // ------------------------------------------------------------------

    public Money plus(Money other) {
        requireSameCurrency(other);
        return new Money(amount.add(other.amount), currency);
    }

    public Money minus(Money other) {
        requireSameCurrency(other);
        return new Money(amount.subtract(other.amount), currency);
    }

    public Money times(long factor) {
        return new Money(amount.multiply(BigDecimal.valueOf(factor)), currency);
    }

    public Money times(BigDecimal factor) {
        return new Money(amount.multiply(factor), currency);
    }

    public Money negate() {
        return new Money(amount.negate(), currency);
    }

    public Money percent(BigDecimal rate) {
        return times(rate.movePointLeft(2));
    }

    public Money max(Money other) {
        return compareTo(other) >= 0 ? this : other;
    }

    public Money min(Money other) {
        return compareTo(other) <= 0 ? this : other;
    }

    /**
     * Splits this amount into {@code parts} shares that add up exactly,
     * giving the leftover minor units to the first shares.
     */
    public List<Money> split(int parts) {
        if (parts <= 0) {
            throw new IllegalArgumentException("parts must be positive: " + parts);
        }
        long total = minorUnits();
        long share = total / parts;
        long remainder = total % parts;
        List<Money> shares = new ArrayList<>(parts);
        for (int i = 0; i < parts; i++) {
            long units = share + (i < remainder ? 1 : 0);
            shares.add(of(units, currency));
        }
        return Collections.unmodifiableList(shares);
    }

    /** Splits proportionally to {@code weights}; see {@link Allocation}. */
    public Allocation allocate(int... weights) {
        return Allocation.of(this, weights);
    }

    private void requireSameCurrency(Money other) {
        if (other.currency != currency) {
            throw new CurrencyMismatchException(currency, other.currency);
        }
    }

    // ------------------------------------------------------------------
    // Comparison, equality, formatting
    // ------------------------------------------------------------------

    @Override
    public int compareTo(Money other) {
        requireSameCurrency(other);
        return amount.compareTo(other.amount);
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof Money other)) {
            return false;
        }
        return currency == other.currency && amount.compareTo(other.amount) == 0;
    }

    @Override
    public int hashCode() {
        return Objects.hash(amount.stripTrailingZeros(), currency);
    }

    @Override
    public String toString() {
        return amount.toPlainString() + " " + currency.name();
    }

    public String format(Locale locale) {
        NumberFormat format = NumberFormat.getCurrencyInstance(locale);
        format.setCurrency(java.util.Currency.getInstance(currency.name()));
        return format.format(amount);
    }

    // ------------------------------------------------------------------
    // Collectors
    // ------------------------------------------------------------------

    /** Sums a stream of amounts of {@code currency}; empty streams give zero. */
    public static Collector<Money, ?, Money> summing(Currency currency) {
        BinaryOperator<Money> add = Money::plus;
        return Collectors.reducing(zero(currency), add);
    }

    public static Comparator<Money> byAmount() {
        return Comparator.comparing(Money::amount);
    }

    // ------------------------------------------------------------------
    // Nested types
    // ------------------------------------------------------------------

    /** The result of {@link #allocate}: shares in the order of the weights. */
    public static final class Allocation {
        private final List<Money> shares;

        private Allocation(List<Money> shares) {
            this.shares = shares;
        }

        static Allocation of(Money total, int... weights) {
            int sum = 0;
            for (int w : weights) {
                if (w < 0) {
                    throw new IllegalArgumentException("negative weight: " + w);
                }
                sum += w;
            }
            if (sum == 0) {
                throw new IllegalArgumentException("weights add up to zero");
            }
            long units = total.minorUnits();
            long given = 0;
            List<Money> shares = new ArrayList<>(weights.length);
            for (int w : weights) {
                long share = units * w / sum;
                shares.add(Money.of(share, total.currency()));
                given += share;
            }
            for (int i = 0; given < units; i = (i + 1) % shares.size()) {
                shares.set(i, shares.get(i).plus(Money.of(1, total.currency())));
                given++;
            }
            return new Allocation(Collections.unmodifiableList(shares));
        }

        public List<Money> shares() {
            return shares;
        }

        public Money share(int index) {
            return shares.get(index);
        }

        public Money total() {
            return shares.stream().collect(summing(shares.get(0).currency()));
        }
    }

    /** A closed range of amounts, used by price filters. */
    public record Range(Money min, Money max) {
        public Range {
            requireNonNull(min, "min");
            requireNonNull(max, "max");
            if (min.compareTo(max) > 0) {
                throw new IllegalArgumentException(min + " > " + max);
            }
        }

        public boolean contains(Money value) {
            return value.compareTo(min) >= 0 && value.compareTo(max) <= 0;
        }

        public Money width() {
            return max.minus(min);
        }
    }

    /** Thrown when two amounts of different currencies meet. */
    public static class CurrencyMismatchException extends IllegalArgumentException {
        private final Currency expected;
        private final Currency actual;

        public CurrencyMismatchException(Currency expected, Currency actual) {
            super("currency mismatch: expected " + expected + ", got " + actual);
            this.expected = expected;
            this.actual = actual;
        }

        public Currency expected() {
            return expected;
        }

        public Currency actual() {
            return actual;
        }
    }

    /** Thrown by {@link #parse} for unreadable input. */
    public static class MoneyFormatException extends IllegalArgumentException {
        public MoneyFormatException(String input, String reason) {
            super("cannot parse \"" + input + "\" as money: " + reason);
        }
    }
}
