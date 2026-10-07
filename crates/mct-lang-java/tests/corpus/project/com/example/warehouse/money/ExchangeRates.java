package com.example.warehouse.money;

import com.example.warehouse.money.Money.Currency;

import java.lang.annotation.Documented;
import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;
import java.math.BigDecimal;
import java.math.MathContext;
import java.time.Clock;
import java.time.Duration;
import java.time.Instant;
import java.time.LocalDate;
import java.util.EnumMap;
import java.util.HashMap;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.locks.ReentrantReadWriteLock;
import java.util.function.Function;
import java.util.function.Supplier;
import java.util.logging.Logger;

/**
 * Converts {@link Money} between currencies at the reference rate of a date.
 * Implementations must be safe to share between threads.
 */
@FunctionalInterface
public interface ExchangeRates {

    /** The rate that turns one unit of {@code from} into {@code to} on {@code date}. */
    BigDecimal rate(Currency from, Currency to, LocalDate date);

    default Money convert(Money amount, Currency to, LocalDate date) {
        if (amount.currency() == to) {
            return amount;
        }
        BigDecimal rate = rate(amount.currency(), to, date);
        return Money.of(amount.amount().multiply(rate, MathContext.DECIMAL64), to);
    }

    default Money convertToday(Money amount, Currency to, Clock clock) {
        return convert(amount, to, LocalDate.now(clock));
    }

    static ExchangeRates fixed(Map<Currency, BigDecimal> perEuro) {
        return new FixedRates(perEuro);
    }

    static ExchangeRates identity() {
        return (from, to, date) -> {
            if (from != to) {
                throw new UnknownRateException(from, to, date);
            }
            return BigDecimal.ONE;
        };
    }

    // ----------------------------------------------------------------------
    // Annotations used by the implementations below
    // ----------------------------------------------------------------------

    /** Documents that every method of the annotated type is thread-safe. */
    @Documented
    @Retention(RetentionPolicy.CLASS)
    @Target(ElementType.TYPE)
    @interface ThreadSafe {
        String value() default "";
    }

    /** Marks a method that performs network I/O. */
    @Retention(RetentionPolicy.RUNTIME)
    @Target(ElementType.METHOD)
    @interface Remote {
        int timeoutMillis() default 2_500;
    }

    // ----------------------------------------------------------------------
    // Sources
    // ----------------------------------------------------------------------

    /** Where reference rates come from: the ECB feed, a file, a test stub. */
    @FunctionalInterface
    interface RateSource {
        Map<Currency, BigDecimal> eurRates(LocalDate date) throws RateSourceException;

        default RateSource withFallback(RateSource fallback) {
            RateSource primary = this;
            return date -> {
                try {
                    return primary.eurRates(date);
                } catch (RateSourceException e) {
                    Logger.getLogger("rates").warning("primary source failed: " + e.getMessage());
                    return fallback.eurRates(date);
                }
            };
        }
    }

    /** A source failure; checked so callers decide whether to fall back. */
    class RateSourceException extends Exception {
        public RateSourceException(String message, Throwable cause) {
            super(message, cause);
        }
    }

    /** No rate between two currencies for a date. */
    class UnknownRateException extends IllegalStateException {
        public UnknownRateException(Currency from, Currency to, LocalDate date) {
            super("no rate " + from + "→" + to + " on " + date);
        }
    }

    // ----------------------------------------------------------------------
    // Implementations
    // ----------------------------------------------------------------------

    /** Rates fixed at construction, quoted per euro. */
    @ThreadSafe("immutable")
    final class FixedRates implements ExchangeRates {
        private final Map<Currency, BigDecimal> perEuro;

        FixedRates(Map<Currency, BigDecimal> perEuro) {
            EnumMap<Currency, BigDecimal> copy = new EnumMap<>(Currency.class);
            copy.putAll(perEuro);
            copy.put(Currency.EUR, BigDecimal.ONE);
            this.perEuro = copy;
        }

        @Override
        public BigDecimal rate(Currency from, Currency to, LocalDate date) {
            BigDecimal fromRate = lookup(from, to, date);
            BigDecimal toRate = lookup(to, from, date);
            return toRate.divide(fromRate, MathContext.DECIMAL64);
        }

        private BigDecimal lookup(Currency currency, Currency other, LocalDate date) {
            BigDecimal rate = perEuro.get(currency);
            if (rate == null) {
                throw new UnknownRateException(currency, other, date);
            }
            return rate;
        }
    }

    /**
     * Rates from a {@link RateSource}, cached per date for {@code ttl}. Reads
     * share a lock; a refresh takes it exclusively.
     */
    @ThreadSafe
    final class CachedRates implements ExchangeRates, AutoCloseable {
        private final RateSource source;
        private final Clock clock;
        private final Duration ttl;
        private final Map<LocalDate, Entry> cache = new HashMap<>();
        private final ReentrantReadWriteLock lock = new ReentrantReadWriteLock();
        private final Map<String, Long> stats = new ConcurrentHashMap<>();

        private record Entry(Map<Currency, BigDecimal> rates, Instant loadedAt) {
            boolean isFresh(Instant now, Duration ttl) {
                return loadedAt.plus(ttl).isAfter(now);
            }
        }

        public CachedRates(RateSource source, Clock clock, Duration ttl) {
            this.source = source;
            this.clock = clock;
            this.ttl = ttl;
        }

        @Override
        @Remote(timeoutMillis = 5_000)
        public BigDecimal rate(Currency from, Currency to, LocalDate date) {
            Map<Currency, BigDecimal> rates = ratesFor(date);
            BigDecimal fromRate = Optional.ofNullable(rates.get(from))
                .orElseThrow(() -> new UnknownRateException(from, to, date));
            BigDecimal toRate = Optional.ofNullable(rates.get(to))
                .orElseThrow(() -> new UnknownRateException(from, to, date));
            count("hits");
            return toRate.divide(fromRate, MathContext.DECIMAL64);
        }

        private Map<Currency, BigDecimal> ratesFor(LocalDate date) {
            Instant now = clock.instant();
            lock.readLock().lock();
            try {
                Entry entry = cache.get(date);
                if (entry != null && entry.isFresh(now, ttl)) {
                    return entry.rates();
                }
            } finally {
                lock.readLock().unlock();
            }
            return refresh(date, now);
        }

        private Map<Currency, BigDecimal> refresh(LocalDate date, Instant now) {
            lock.writeLock().lock();
            try {
                Entry entry = cache.get(date);
                if (entry != null && entry.isFresh(now, ttl)) {
                    return entry.rates();
                }
                Map<Currency, BigDecimal> loaded = load(date);
                cache.put(date, new Entry(loaded, now));
                count("loads");
                return loaded;
            } finally {
                lock.writeLock().unlock();
            }
        }

        private Map<Currency, BigDecimal> load(LocalDate date) {
            try {
                Map<Currency, BigDecimal> rates = new EnumMap<>(source.eurRates(date));
                rates.put(Currency.EUR, BigDecimal.ONE);
                return rates;
            } catch (RateSourceException e) {
                count("failures");
                throw new IllegalStateException("rates unavailable for " + date, e);
            }
        }

        /** Loads today's rates ahead of the first request. */
        public void warmUp() {
            refresh(LocalDate.now(clock), clock.instant());
        }

        public Map<String, Long> stats() {
            return Map.copyOf(stats);
        }

        private void count(String key) {
            stats.merge(key, 1L, Long::sum);
        }

        @Override
        public void close() {
            lock.writeLock().lock();
            try {
                cache.clear();
            } finally {
                lock.writeLock().unlock();
            }
        }
    }

    /**
     * Memoizes any {@code Function} per key; used for the per-currency
     * formatter cache. Generic over the key and value types.
     */
    final class Memo<K, V> implements Function<K, V> {
        private final Map<K, V> values = new ConcurrentHashMap<>();
        private final Function<? super K, ? extends V> compute;

        public Memo(Function<? super K, ? extends V> compute) {
            this.compute = compute;
        }

        @Override
        public V apply(K key) {
            return values.computeIfAbsent(key, compute);
        }

        public static <K, V> Memo<K, V> of(Function<? super K, ? extends V> compute) {
            return new Memo<>(compute);
        }

        public <R> Memo<K, R> andThenMemo(Function<? super V, ? extends R> next) {
            return Memo.of(key -> next.apply(apply(key)));
        }
    }

    /** A source that always answers the same rates; handy in tests. */
    static RateSource constant(Map<Currency, BigDecimal> rates) {
        return new RateSource() {
            private int calls;

            @Override
            public Map<Currency, BigDecimal> eurRates(LocalDate date) {
                calls++;
                return rates;
            }

            @Override
            public String toString() {
                return "constant(" + rates + ", calls=" + calls + ")";
            }
        };
    }

    /** Lazily creates the default rates of the application. */
    static Supplier<ExchangeRates> lazyDefault(Clock clock) {
        return new Supplier<>() {
            private volatile ExchangeRates instance;

            @Override
            public ExchangeRates get() {
                ExchangeRates local = instance;
                if (local == null) {
                    synchronized (this) {
                        local = instance;
                        if (local == null) {
                            RateSource ecb = constant(Map.of(Currency.USD, new BigDecimal("1.0870")));
                            instance = local = new CachedRates(ecb, clock, Duration.ofHours(1));
                        }
                    }
                }
                return local;
            }
        };
    }
}
