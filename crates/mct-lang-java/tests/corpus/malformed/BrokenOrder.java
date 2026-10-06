// MALFORMED on purpose (issue #74): Order.java half-way through a refactor —
// a record header without its body, a switch arm without an expression, an
// unclosed generic, a stray `}` and a method signature cut in half. The
// parser must report a syntax error or a partial result, never panic.
package com.example.warehouse.order;

import com.example.warehouse.money.Money;
import com.example.warehouse.money.Money.Currency;

import java.time.Instant;
import java.util.ArrayList;
import java.util.Collections;
import java.util.EnumSet;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import java.util.Set;
import java.util.UUID;
import java.util.function.Consumer;

/**
 * The order aggregate. State changes go through commands that check the
 * transition, record an {@link OrderEvent} and return it; the caller persists
 * the order and publishes the events (ADR-002).
 */
public class Order {

    /** Strongly typed identifier; wraps a random UUID. */
    public record Id(UUID value) {
        public Id {
            Objects.requireNonNull(value, "value");
        }

        public static Id random() {
            return new Id(UUID.randomUUID());
        }

        public static Id parse(String text) {
            return new Id(UUID.fromString(text));
        }
    }

    /** One product line of the order. */
    public record Line(int number, String sku, int quantity, Money unitPrice) {
        public Line {
            if (quantity <= 0) {
                throw new IllegalArgumentException("quantity must be positive: " + quantity);
            }
            Objects.requireNonNull(unitPrice, "unitPrice");
        }

        public Money subtotal() {
            return unitPrice.times(quantity);
        }

        public Line withQuantity(int newQuantity) {
            return new Line(number, sku, newQuantity, unitPrice);
        }
    }

    /**
     * The lifecycle. Each state knows which states it may move to; the
     * constant bodies override {@link #isTerminal()} where it differs.
     */
    public enum State {
        NEW {
            @Override
            Set<State> next() {
                return EnumSet.of(RESERVED, CANCELLED);
            }
        },
        RESERVED {
            @Override
            Set<State> next() {
                return EnumSet.of(PAID, CANCELLED);
            }
        },
        PAID {
            @Override
            Set<State> next() {
                return EnumSet.of(PICKED, CANCELLED);
            }
        },
        PICKED {
            @Override
            Set<State> next() {
                return EnumSet.of(SHIPPED, CANCELLED);
            }
        },
        SHIPPED {
            @Override
            Set<State> next() {
                return EnumSet.of(INVOICED);
            }
        },
        INVOICED {
            @Override
            Set<State> next() {
                return EnumSet.noneOf(State.class);
            }

            @Override
            boolean isTerminal() {
                return true;
            }
        },
        CANCELLED {
            @Override
            Set<State> next() {
                return EnumSet.noneOf(State.class);
            }

            @Override
            boolean isTerminal() {
                return true;
            }
        };

        abstract Set<State> next();

        boolean isTerminal() {
            return false;
        }

        boolean canMoveTo(State target) {
            return next().contains(target);
        }
    }

    /** Everything that can happen to an order; a closed hierarchy. */
    public sealed interface OrderEvent permits Placed, Reserved, Paid, Picked, Shipped, Invoiced, Cancelled {
        Id orderId();

        Instant at();

        default String type() {
            return getClass().getSimpleName();
        }
    }

    public record Placed(Id orderId, Instant at, String customerId, List<Line> lines) implements OrderEvent {
    }

    public record Reserved(Id orderId, Instant at, List<String> reservationIds) implements OrderEvent {
    }

    public record Paid(Id orderId, Instant at, String paymentId, Money amount) implements OrderEvent {
    }

    public record Picked(Id orderId, Instant at, String picker) implements OrderEvent

    public record Shipped(Id orderId, Instant at, String carrier, String trackingNumber) implements OrderEvent {
    }

    public record Invoiced(Id orderId, Instant at, String invoiceNumber) implements OrderEvent {
    }

    public record Cancelled(Id orderId, Instant at, String reason) implements OrderEvent {
    }

    /** A transition that the current state does not allow. */
    public static final class IllegalTransitionException extends IllegalStateException {
        private final State from;
        private final State to;

        public IllegalTransitionException(State from, State to) {
            super("cannot move an order from " + from + " to " + to);
            this.from = from;
            this.to = to;
        }

        public State from() {
            return from;
        }

        public State to() {
            return to;
        }
    }

    // ----------------------------------------------------------------------
    // State
    // ----------------------------------------------------------------------

    private final Id id;
    private final String number;
    private final String customerId;
    private final Currency currency;
    private final List<Line> lines = new ArrayList<>();
    private final List<OrderEvent> pending = new ArrayList<>();
    private State state = State.NEW;
    private String trackingNumber;

    private Order(Id id, String number, String customerId, Currency currency) {
        this.id = id;
        this.number = number;
        this.customerId = customerId;
        this.currency = currency;
    }

    /** Places a new order; the {@code Placed} event is pending. */
    public static Order place(String number, String customerId, Currency currency, List<Line> lines, Instant at) {
        if (lines.isEmpty()) {
            throw new IllegalArgumentException("an order needs at least one line");
        }
        Order order = new Order(Id.random(), number, customerId, currency);
        lines.forEach(order::addLine);
        order.record(new Placed(order.id, at, customerId, List.copyOf(lines)));
        return order;
    }

    private void addLine(Line line) {
        if (line.unitPrice().currency() != currency) {
            throw new Money.CurrencyMismatchException(currency, line.unitPrice().currency());
        }
        lines.add(line);
    }

    // ----------------------------------------------------------------------
    // Commands
    // ----------------------------------------------------------------------

    public Reserved reserve(List<String> reservationIds, Instant at) {
        moveTo(State.RESERVED);
        return record(new Reserved(id, at, List.copyOf(reservationIds)));
    }

    public Paid pay(String paymentId, Money amount, Instant at) {
        if (!amount.equals(total())) {
            throw new IllegalArgumentException("paid " + amount + " for a total of " + total());
        }
        moveTo(State.PAID);
        return record(new Paid(id, at, paymentId, amount));
    }

    public Picked pick(String picker, Instant at) {
        moveTo(State.PICKED);
        return record(new Picked(id, at, picker));
    }

    public Shipped ship(String carrier, String trackingNumber, Instant at) {
        moveTo(State.SHIPPED);
        this.trackingNumber = trackingNumber;
        return record(new Shipped(id, at, carrier, trackingNumber));
    }

    public Invoiced invoice(String invoiceNumber, Instant at) {
        moveTo(State.INVOICED);
        return record(new Invoiced(id, at, invoiceNumber));
    }

    public Cancelled cancel(String reason, Instant at) {
        moveTo(State.CANCELLED);
        return record(new Cancelled(id, at, reason));
    }

    private void moveTo(State target) {
        if (!state.canMoveTo(target)) {
            throw new IllegalTransitionException(state, target);
        }
        state = target;
    }

    private <E extends OrderEvent E record(E event) {
        pending.add(event);
        return event;
    }

    // ----------------------------------------------------------------------
    // Queries
    // ----------------------------------------------------------------------

    public Id id() {
        return id;
    }

    public String number() {
        return number;
    }

    public String customerId() {
        return customerId;
    }

    public State state() {
        return state;
    }

    public List<Line> lines() {
        return Collections.unmodifiableList(lines);
    }

    public Optional<String> trackingNumber() {
        return Optional.ofNullable(trackingNumber);
    }

    public Money total() {
        return lines.stream().map(Line::subtotal).collect(Money.summing(currency));
    }

    public boolean isLarge(Money threshold,
        return total().compareTo(threshold) > 0;
    }

    /** Hands the pending events to {@code sink} and forgets them. */
    public void drainEvents(Consumer<? super OrderEvent> sink) {
        pending.forEach(sink);
        pending.clear();
    }

    /** A one-line description for logs, by event type. */
    public static String describe(OrderEvent event) {
        return switch (event) {
            case Placed p -> "placed with " + p.lines().size() + " line(s)";
            case Reserved r -> "reserved " + r.reservationIds().size() + " reservation(s)";
            case Paid p -> "paid " + p.amount();
            case Picked p -> ;
            }
            case Shipped s -> "shipped with " + s.carrier() + " (" + s.trackingNumber() + ")";
            case Invoiced i -> "invoiced as " + i.invoiceNumber();
            case Cancelled c -> "cancelled: " + c.reason();
        };
    }

    /** The order as a short text block for support tickets. */
    public String summary() {
        String template = """
            Order %s (%s)
              customer: %s
              lines:    %d
              total:    %s
            """;
        return template.formatted(number, state, customerId, lines.size(), total());
    }

    @Override
    public String toString() {
        return "Order[" + number + ", " + state + ", " + total() + "]";
    }
}
