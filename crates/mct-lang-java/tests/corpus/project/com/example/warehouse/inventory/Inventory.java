package com.example.warehouse.inventory;

import com.example.warehouse.order.Order;
import com.example.warehouse.order.Order.Line;

import java.time.Clock;
import java.time.Duration;
import java.time.Instant;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.UUID;
import java.util.concurrent.locks.Lock;
import java.util.concurrent.locks.ReentrantLock;
import java.util.function.Predicate;
import java.util.stream.Collectors;
import java.util.stream.Stream;

/**
 * Stock per product and location, and the reservations that promise it to
 * orders. Reservation is atomic per location; a line is split over several
 * locations only when no single location can serve it.
 */
public final class Inventory {

    /** A shelf address: zone, aisle, level ({@code A-03-2}). */
    public record Location(String zone, int aisle, int level) implements Comparable<Location> {
        public static Location parse(String code) {
            String[] parts = code.split("-");
            if (parts.length != 3) {
                throw new IllegalArgumentException("bad location: " + code);
            }
            return new Location(parts[0], Integer.parseInt(parts[1]), Integer.parseInt(parts[2]));
        }

        public boolean isChilled() {
            return zone.startsWith("F");
        }

        @Override
        public int compareTo(Location other) {
            return Comparator.comparing(Location::zone)
                .thenComparingInt(Location::aisle)
                .thenComparingInt(Location::level)
                .compare(this, other);
        }

        @Override
        public String toString() {
            return "%s-%02d-%d".formatted(zone, aisle, level);
        }
    }

    /** On hand and reserved quantity of one product at one location. */
    public static final class StockLevel {
        private final String sku;
        private final Location location;
        private int onHand;
        private int reserved;

        StockLevel(String sku, Location location, int onHand) {
            this.sku = sku;
            this.location = location;
            this.onHand = onHand;
        }

        public String sku() {
            return sku;
        }

        public Location location() {
            return location;
        }

        public int onHand() {
            return onHand;
        }

        public int reserved() {
            return reserved;
        }

        public int available() {
            return onHand - reserved;
        }

        boolean tryReserve(int quantity) {
            if (available() < quantity) {
                return false;
            }
            reserved += quantity;
            return true;
        }

        void release(int quantity) {
            reserved = Math.max(0, reserved - quantity);
        }

        void consume(int quantity) {
            release(quantity);
            onHand -= quantity;
        }

        void receive(int quantity) {
            onHand += quantity;
        }
    }

    /** A promise of {@code quantity} units of a SKU at a location to an order. */
    public record Reservation(String id, Order.Id orderId, String sku, Location location, int quantity, Instant expiresAt) {
        public boolean isExpired(Instant now) {
            return !expiresAt.isAfter(now);
        }
    }

    /** No location can serve a line, even split. */
    public static class OutOfStockException extends RuntimeException {
        private final String sku;
        private final int requested;
        private final int available;

        public OutOfStockException(String sku, int requested, int available) {
            super(sku + ": requested " + requested + ", available " + available);
            this.sku = sku;
            this.requested = requested;
            this.available = available;
        }

        public String sku() {
            return sku;
        }

        public int shortfall() {
            return requested - available;
        }
    }

    /** Receives inventory events; the outbox writer implements it. */
    public interface Listener {
        void stockReserved(Reservation reservation);

        void stockReleased(Reservation reservation, String reason);

        default void reorderPointReached(String sku, int available) {
        }

        static Listener noop() {
            return new Listener() {
                @Override
                public void stockReserved(Reservation reservation) {
                }

                @Override
                public void stockReleased(Reservation reservation, String reason) {
                }
            };
        }
    }

    // ----------------------------------------------------------------------
    // State
    // ----------------------------------------------------------------------

    private final Map<String, List<StockLevel>> levels = new HashMap<>();
    private final Map<String, Reservation> reservations = new HashMap<>();
    private final Map<String, Integer> reorderPoints = new HashMap<>();
    private final Lock lock = new ReentrantLock();
    private final Clock clock;
    private final Duration reservationTtl;
    private final Listener listener;

    public Inventory(Clock clock, Duration reservationTtl, Listener listener) {
        this.clock = clock;
        this.reservationTtl = reservationTtl;
        this.listener = listener;
    }

    // ----------------------------------------------------------------------
    // Stock
    // ----------------------------------------------------------------------

    public void receive(String sku, Location location, int quantity) {
        withLock(() -> levelAt(sku, location).receive(quantity));
    }

    public void setReorderPoint(String sku, int point) {
        reorderPoints.put(sku, point);
    }

    public int available(String sku) {
        return levelsOf(sku).stream().mapToInt(StockLevel::available).sum();
    }

    public Map<Location, Integer> availableByLocation(String sku) {
        return levelsOf(sku).stream()
            .filter(level -> level.available() > 0)
            .collect(Collectors.toMap(StockLevel::location, StockLevel::available));
    }

    private List<StockLevel> levelsOf(String sku) {
        return levels.getOrDefault(sku, List.of());
    }

    private StockLevel levelAt(String sku, Location location) {
        List<StockLevel> list = levels.computeIfAbsent(sku, k -> new ArrayList<>());
        for (StockLevel level : list) {
            if (level.location().equals(location)) {
                return level;
            }
        }
        StockLevel created = new StockLevel(sku, location, 0);
        list.add(created);
        return created;
    }

    // ----------------------------------------------------------------------
    // Reservations
    // ----------------------------------------------------------------------

    /**
     * Reserves every line of {@code order}, all or nothing. Returns the
     * reservation ids, to be passed to {@link Order#reserve}.
     */
    public List<String> reserve(Order order) {
        lock.lock();
        try {
            List<Reservation> made = new ArrayList<>();
            try {
                for (Line line : order.lines()) {
                    made.addAll(reserveLine(order.id(), line));
                }
            } catch (OutOfStockException e) {
                made.forEach(r -> undo(r, "rollback"));
                throw e;
            }
            made.forEach(listener::stockReserved);
            made.forEach(r -> checkReorderPoint(r.sku()));
            return made.stream().map(Reservation::id).toList();
        } finally {
            lock.unlock();
        }
    }

    private List<Reservation> reserveLine(Order.Id orderId, Line line) {
        Instant expiresAt = clock.instant().plus(reservationTtl);
        Optional<StockLevel> single = levelsOf(line.sku()).stream()
            .filter(level -> level.available() >= line.quantity())
            .max(Comparator.comparingInt(StockLevel::available));
        if (single.isPresent() && single.get().tryReserve(line.quantity())) {
            return List.of(newReservation(orderId, line.sku(), single.get().location(), line.quantity(), expiresAt));
        }
        return splitLine(orderId, line, expiresAt);
    }

    private List<Reservation> splitLine(Order.Id orderId, Line line, Instant expiresAt) {
        int remaining = line.quantity();
        List<Reservation> parts = new ArrayList<>();
        Iterator<StockLevel> candidates = levelsOf(line.sku()).stream()
            .sorted(Comparator.comparingInt(StockLevel::available).reversed())
            .iterator();
        while (remaining > 0 && candidates.hasNext()) {
            StockLevel level = candidates.next();
            int take = Math.min(remaining, level.available());
            if (take > 0 && level.tryReserve(take)) {
                parts.add(newReservation(orderId, line.sku(), level.location(), take, expiresAt));
                remaining -= take;
            }
        }
        if (remaining > 0) {
            parts.forEach(r -> undo(r, "split failed"));
            throw new OutOfStockException(line.sku(), line.quantity(), line.quantity() - remaining);
        }
        return parts;
    }

    private Reservation newReservation(Order.Id orderId, String sku, Location location, int quantity, Instant expiresAt) {
        Reservation reservation = new Reservation(UUID.randomUUID().toString(), orderId, sku, location, quantity, expiresAt);
        reservations.put(reservation.id(), reservation);
        return reservation;
    }

    /** Releases every reservation of an order (cancellation). */
    public void release(Order.Id orderId, String reason) {
        withLock(() -> reservationsOf(orderId).forEach(r -> {
            undo(r, reason);
            listener.stockReleased(r, reason);
        }));
    }

    /** Turns an order's reservations into stock movements (picking). */
    public void consume(Order.Id orderId) {
        withLock(() -> reservationsOf(orderId).forEach(r -> {
            levelAt(r.sku(), r.location()).consume(r.quantity());
            reservations.remove(r.id());
        }));
    }

    /** Releases expired reservations; run every minute by the scheduler. */
    public int expire() {
        Instant now = clock.instant();
        class Expired implements Predicate<Reservation> {
            int count;

            @Override
            public boolean test(Reservation reservation) {
                boolean expired = reservation.isExpired(now);
                if (expired) {
                    count++;
                }
                return expired;
            }
        }
        Expired expired = new Expired();
        lock.lock();
        try {
            List<Reservation> due = reservations.values().stream().filter(expired).toList();
            for (Reservation r : due) {
                undo(r, "expired");
                listener.stockReleased(r, "expired");
            }
        } finally {
            lock.unlock();
        }
        return expired.count;
    }

    private Stream<Reservation> reservationsOf(Order.Id orderId) {
        return reservations.values().stream()
            .filter(r -> r.orderId().equals(orderId))
            .toList()
            .stream();
    }

    private void undo(Reservation reservation, String reason) {
        levelAt(reservation.sku(), reservation.location()).release(reservation.quantity());
        reservations.remove(reservation.id());
    }

    private void checkReorderPoint(String sku) {
        Integer point = reorderPoints.get(sku);
        int available = available(sku);
        if (point != null && available < point) {
            listener.reorderPointReached(sku, available);
        }
    }

    private void withLock(Runnable action) {
        lock.lock();
        try {
            action.run();
        } finally {
            lock.unlock();
        }
    }
}
