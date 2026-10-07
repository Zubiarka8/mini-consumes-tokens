package com.example.warehouse.app;

import com.example.warehouse.inventory.Inventory;
import com.example.warehouse.inventory.Inventory.OutOfStockException;
import com.example.warehouse.money.ExchangeRates;
import com.example.warehouse.money.Money;
import com.example.warehouse.money.Money.Currency;
import com.example.warehouse.order.Order;
import com.example.warehouse.order.Order.Line;
import com.example.warehouse.order.Order.OrderEvent;
import com.example.warehouse.persistence.Repository;
import com.example.warehouse.persistence.Repository.Transactional;

import java.time.Clock;
import java.time.Duration;
import java.time.Instant;
import java.time.LocalDate;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Optional;
import java.util.concurrent.atomic.AtomicLong;
import java.util.function.Consumer;
import java.util.logging.Level;
import java.util.logging.Logger;

import static java.util.stream.Collectors.groupingBy;
import static java.util.stream.Collectors.counting;

/**
 * Application service for orders: places, approves, pays and cancels them,
 * coordinating {@link Inventory}, pricing and the {@link Repository}. It is
 * the only class that calls commands on {@link Order}.
 */
public class OrderService {

    private static final Logger LOG = Logger.getLogger(OrderService.class.getName());

    /** A request from the shop, before prices are known. */
    public record PlaceOrder(String customerId, Currency currency, Map<String, Integer> quantities) {
        public PlaceOrder {
            Objects.requireNonNull(customerId, "customerId");
            if (quantities.isEmpty()) {
                throw new IllegalArgumentException("empty basket");
            }
        }
    }

    /** What the shop gets back. */
    public sealed interface Outcome permits Accepted, PendingApproval, Rejected {
    }

    public record Accepted(Order.Id id, String number, Money total) implements Outcome {
    }

    public record PendingApproval(Order.Id id, String number, Money total) implements Outcome {
    }

    public record Rejected(String reason) implements Outcome {
    }

    /** Looks up current unit prices; implemented by the pricing context. */
    @FunctionalInterface
    public interface PriceList {
        Money unitPrice(String sku, Currency currency);
    }

    /** Generates gapless order numbers per year. */
    public static final class OrderNumbers {
        private final AtomicLong sequence = new AtomicLong();
        private final Clock clock;

        public OrderNumbers(Clock clock) {
            this.clock = clock;
        }

        public String next() {
            int year = LocalDate.now(clock).getYear();
            return "PED-%d-%06d".formatted(year, sequence.incrementAndGet());
        }
    }

    private final Repository.Orders orders;
    private final Inventory inventory;
    private final PriceList prices;
    private final ExchangeRates rates;
    private final OrderNumbers numbers;
    private final Clock clock;
    private final Money largeOrderThreshold;
    private final List<Consumer<OrderEvent>> subscribers = new ArrayList<>();

    public OrderService(Repository.Orders orders, Inventory inventory, PriceList prices,
                        ExchangeRates rates, Clock clock, Money largeOrderThreshold) {
        this.orders = orders;
        this.inventory = inventory;
        this.prices = prices;
        this.rates = rates;
        this.clock = clock;
        this.numbers = new OrderNumbers(clock);
        this.largeOrderThreshold = largeOrderThreshold;
    }

    public void subscribe(Consumer<OrderEvent> subscriber) {
        subscribers.add(subscriber);
    }

    // ----------------------------------------------------------------------
    // Commands
    // ----------------------------------------------------------------------

    @Transactional
    public Outcome place(PlaceOrder request) {
        List<Line> lines = priceLines(request);
        Instant now = clock.instant();
        Order order = Order.place(numbers.next(), request.customerId(), request.currency(), lines, now);
        Money totalInEuro = rates.convert(order.total(), Currency.EUR, LocalDate.now(clock));
        if (totalInEuro.compareTo(largeOrderThreshold) > 0) {
            save(order);
            LOG.info(() -> "order " + order.number() + " waits for approval");
            return new PendingApproval(order.id(), order.number(), order.total());
        }
        try {
            reserve(order, now);
        } catch (OutOfStockException e) {
            order.cancel("out of stock: " + e.sku(), now);
            save(order);
            return new Rejected(e.getMessage());
        }
        save(order);
        return new Accepted(order.id(), order.number(), order.total());
    }

    @Transactional
    public Outcome approve(Order.Id id, boolean approved, String supervisor) {
        Order order = orders.getById(id);
        Instant now = clock.instant();
        if (!approved) {
            order.cancel("rejected by " + supervisor, now);
            save(order);
            return new Rejected("rejected by " + supervisor);
        }
        try {
            reserve(order, now);
        } catch (OutOfStockException e) {
            order.cancel("out of stock after approval: " + e.sku(), now);
            save(order);
            return new Rejected(e.getMessage());
        }
        save(order);
        return new Accepted(order.id(), order.number(), order.total());
    }

    @Transactional
    public void confirmPayment(String orderNumber, String paymentId, Money amount) {
        Order order = orders.findIdByNumber(orderNumber)
            .map(orders::getById)
            .orElseThrow(() -> new Repository.NotFoundException(orderNumber));
        order.pay(paymentId, amount, clock.instant());
        save(order);
    }

    @Transactional
    public void cancel(Order.Id id, String reason) {
        Order order = orders.getById(id);
        order.cancel(reason, clock.instant());
        inventory.release(id, reason);
        save(order);
    }

    private void reserve(Order order, Instant now) {
        List<String> reservationIds = inventory.reserve(order);
        order.reserve(reservationIds, now);
    }

    private List<Line> priceLines(PlaceOrder request) {
        List<Line> lines = new ArrayList<>();
        int number = 1;
        for (Map.Entry<String, Integer> entry : new LinkedHashMap<>(request.quantities()).entrySet()) {
            Money price = prices.unitPrice(entry.getKey(), request.currency());
            lines.add(new Line(number++, entry.getKey(), entry.getValue(), price));
        }
        return lines;
    }

    private void save(Order order) {
        List<OrderEvent> published = new ArrayList<>();
        order.drainEvents(published::add);
        orders.save(order);
        for (OrderEvent event : published) {
            LOG.log(Level.FINE, "{0}: {1}", new Object[] {order.number(), Order.describe(event)});
            subscribers.forEach(s -> s.accept(event));
        }
    }

    // ----------------------------------------------------------------------
    // Queries and reports
    // ----------------------------------------------------------------------

    public Optional<Order> find(String number) {
        return orders.findIdByNumber(number).flatMap(orders::findById);
    }

    public Map<Order.State, Long> countByState(int limit) {
        return orders.findAll(limit, 0).stream().collect(groupingBy(Order::state, counting()));
    }

    public List<Order> largest(int n) {
        return orders.findAll(1_000, 0).stream()
            .sorted(Comparator.comparing(Order::total, Money.byAmount()).reversed())
            .limit(n)
            .toList();
    }

    public Money revenueSince(Duration window) {
        Instant to = clock.instant();
        return orders.revenue(Currency.EUR, to.minus(window), to);
    }

    /** The figures the supervisors' dashboard shows for one day. */
    public static final class DailyReport {
        private final Map<Order.State, Long> byState;
        private final Money revenue;
        private final List<Order> largest;

        private DailyReport(Map<Order.State, Long> byState, Money revenue, List<Order> largest) {
            this.byState = byState;
            this.revenue = revenue;
            this.largest = largest;
        }

        public static DailyReport build(OrderService service) {
            return new DailyReport(
                service.countByState(10_000),
                service.revenueSince(Duration.ofDays(1)),
                service.largest(5));
        }

        public long count(Order.State state) {
            return byState.getOrDefault(state, 0L);
        }

        public Money revenue() {
            return revenue;
        }

        public String render() {
            StringBuilder out = new StringBuilder();
            out.append("Revenue: ").append(revenue).append('\n');
            byState.forEach((state, n) -> out.append(state).append(": ").append(n).append('\n'));
            largest.forEach(order -> out.append("  ").append(order.number()).append(' ').append(order.total()).append('\n'));
            return out.toString();
        }
    }

    /** A stale-reservation sweeper the scheduler runs every minute. */
    public Runnable expiryJob() {
        return new Runnable() {
            private int runs;

            @Override
            public void run() {
                runs++;
                int expired = inventory.expire();
                if (expired > 0) {
                    LOG.info("run " + runs + ": released " + expired + " expired reservation(s)");
                }
            }
        };
    }

    /** Wires a service with in-memory collaborators, for the demo and tests. */
    public static OrderService demo(Clock clock) {
        Repository.Orders orders = new Repository.Orders(null, new Repository.Outbox());
        Inventory inventory = new Inventory(clock, Duration.ofMinutes(30), Inventory.Listener.noop());
        inventory.receive("sku-1001", Inventory.Location.parse("A-03-2"), 40);
        inventory.receive("sku-2001", Inventory.Location.parse("C-02-1"), 960);
        PriceList prices = (sku, currency) -> switch (sku) {
            case "sku-1001" -> Money.of("24.90", currency);
            case "sku-2001" -> Money.of("9.80", currency);
            default -> throw new IllegalArgumentException("unknown sku " + sku);
        };
        ExchangeRates rates = ExchangeRates.identity();
        return new OrderService(orders, inventory, prices, rates, clock, Money.of("5000", Currency.EUR));
    }

    public static void main(String[] args) {
        OrderService service = demo(Clock.systemUTC());
        service.subscribe(event -> System.out.println(Order.describe(event)));
        Outcome outcome = service.place(new PlaceOrder("cus_8812", Currency.EUR, Map.of("sku-1001", 2, "sku-2001", 6)));
        String message = switch (outcome) {
            case Accepted a -> "accepted " + a.number() + " for " + a.total();
            case PendingApproval p -> "waiting for approval: " + p.number();
            case Rejected r -> "rejected: " + r.reason();
        };
        System.out.println(message);
        if (args.length > 0 && args[0].equals("--expire")) {
            service.expiryJob().run();
        }
    }
}
