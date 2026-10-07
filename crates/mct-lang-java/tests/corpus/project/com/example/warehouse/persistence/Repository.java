package com.example.warehouse.persistence;

import com.example.warehouse.money.Money;
import com.example.warehouse.money.Money.Currency;
import com.example.warehouse.order.Order;
import com.example.warehouse.order.Order.OrderEvent;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;
import java.sql.Connection;
import java.sql.PreparedStatement;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.sql.Timestamp;
import java.time.Instant;
import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;
import java.util.function.Function;
import javax.sql.DataSource;

/**
 * Persistence for aggregates. One generic contract, an in-memory version for
 * tests and a JDBC base class that concrete repositories extend.
 *
 * @param <T>  the aggregate type
 * @param <ID> its identifier type
 */
public interface Repository<T, ID> extends AutoCloseable {

    Optional<T> findById(ID id);

    List<T> findAll(int limit, int offset);

    void save(T aggregate);

    boolean delete(ID id);

    default T getById(ID id) {
        return findById(id).orElseThrow(() -> new NotFoundException(id));
    }

    default boolean exists(ID id) {
        return findById(id).isPresent();
    }

    default void saveAll(Collection<? extends T> aggregates) {
        aggregates.forEach(this::save);
    }

    @Override
    default void close() {
    }

    /** Marks a method that must run inside a transaction. */
    @Retention(RetentionPolicy.RUNTIME)
    @Target({ElementType.METHOD, ElementType.TYPE})
    @interface Transactional {
        boolean readOnly() default false;

        int timeoutSeconds() default 30;
    }

    /** No aggregate with the given id. */
    class NotFoundException extends RuntimeException {
        public NotFoundException(Object id) {
            super("not found: " + id);
        }
    }

    /** A database error surfaced unchecked, with the statement that failed. */
    class PersistenceException extends RuntimeException {
        private final String sql;

        public PersistenceException(String sql, SQLException cause) {
            super(cause.getMessage() + " [" + sql + "]", cause);
            this.sql = sql;
        }

        public String sql() {
            return sql;
        }
    }

    /** One page of results and whether more follow. */
    record Page<T>(List<T> items, int offset, boolean hasMore) {
        public static <T> Page<T> of(List<T> fetched, int limit, int offset) {
            boolean more = fetched.size() > limit;
            return new Page<>(more ? fetched.subList(0, limit) : fetched, offset, more);
        }

        public int nextOffset() {
            return offset + items.size();
        }
    }

    /** Maps one row to a value. */
    @FunctionalInterface
    interface RowMapper<R> {
        R map(ResultSet rs) throws SQLException;
    }

    // ----------------------------------------------------------------------
    // In-memory
    // ----------------------------------------------------------------------

    /** A thread-safe map-backed repository for tests. */
    class InMemory<T, ID> implements Repository<T, ID> {
        private final Map<ID, T> rows = new ConcurrentHashMap<>();
        private final Function<? super T, ? extends ID> idOf;

        public InMemory(Function<? super T, ? extends ID> idOf) {
            this.idOf = idOf;
        }

        @Override
        public Optional<T> findById(ID id) {
            return Optional.ofNullable(rows.get(id));
        }

        @Override
        public List<T> findAll(int limit, int offset) {
            return rows.values().stream().skip(offset).limit(limit).toList();
        }

        @Override
        public void save(T aggregate) {
            rows.put(idOf.apply(aggregate), aggregate);
        }

        @Override
        public boolean delete(ID id) {
            return rows.remove(id) != null;
        }

        public int size() {
            return rows.size();
        }
    }

    // ----------------------------------------------------------------------
    // JDBC
    // ----------------------------------------------------------------------

    /** Shared JDBC plumbing: connections, statements, error translation. */
    abstract class Jdbc<T, ID> implements Repository<T, ID> {
        protected final DataSource dataSource;
        protected final String table;

        protected Jdbc(DataSource dataSource, String table) {
            this.dataSource = dataSource;
            this.table = table;
        }

        protected abstract RowMapper<T> mapper();

        protected abstract Object[] insertValues(T aggregate);

        protected abstract String insertSql();

        @Override
        @Transactional(readOnly = true)
        public Optional<T> findById(ID id) {
            List<T> found = query("SELECT * FROM " + table + " WHERE id = ?", mapper(), id);
            return found.stream().findFirst();
        }

        @Override
        @Transactional(readOnly = true)
        public List<T> findAll(int limit, int offset) {
            return query("SELECT * FROM " + table + " ORDER BY id LIMIT ? OFFSET ?", mapper(), limit, offset);
        }

        @Override
        @Transactional
        public void save(T aggregate) {
            update(insertSql(), insertValues(aggregate));
        }

        @Override
        @Transactional
        public boolean delete(ID id) {
            return update("DELETE FROM " + table + " WHERE id = ?", id) > 0;
        }

        protected <R> List<R> query(String sql, RowMapper<R> rowMapper, Object... params) {
            try (Connection c = dataSource.getConnection();
                 PreparedStatement ps = prepare(c, sql, params);
                 ResultSet rs = ps.executeQuery()) {
                List<R> out = new ArrayList<>();
                while (rs.next()) {
                    out.add(rowMapper.map(rs));
                }
                return out;
            } catch (SQLException e) {
                throw new PersistenceException(sql, e);
            }
        }

        protected int update(String sql, Object... params) {
            try (Connection c = dataSource.getConnection();
                 PreparedStatement ps = prepare(c, sql, params)) {
                return ps.executeUpdate();
            } catch (SQLException e) {
                throw new PersistenceException(sql, e);
            }
        }

        private static PreparedStatement prepare(Connection c, String sql, Object... params) throws SQLException {
            PreparedStatement ps = c.prepareStatement(sql);
            for (int i = 0; i < params.length; i++) {
                ps.setObject(i + 1, params[i]);
            }
            return ps;
        }
    }

    /** Orders, with their lines in a child table and an outbox for events. */
    final class Orders extends Jdbc<Order, Order.Id> {
        private static final String INSERT = """
            INSERT INTO orders (id, number, customer_id, state, total, currency)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT (id) DO UPDATE SET state = EXCLUDED.state, total = EXCLUDED.total
            """;

        private final Outbox outbox;

        public Orders(DataSource dataSource, Outbox outbox) {
            super(dataSource, "orders");
            this.outbox = outbox;
        }

        @Override
        protected RowMapper<Order> mapper() {
            return rs -> {
                throw new UnsupportedOperationException("orders are rebuilt by OrderLoader, not mapped row by row");
            };
        }

        @Override
        protected String insertSql() {
            return INSERT;
        }

        @Override
        protected Object[] insertValues(Order order) {
            Money total = order.total();
            return new Object[] {
                order.id().value(), order.number(), order.customerId(), order.state().name(),
                total.amount(), total.currency().name()
            };
        }

        @Override
        @Transactional
        public void save(Order order) {
            super.save(order);
            order.drainEvents(outbox::append);
        }

        @Transactional(readOnly = true)
        public Optional<Order.Id> findIdByNumber(String number) {
            List<Order.Id> ids = query(
                "SELECT id FROM orders WHERE number = ?",
                rs -> new Order.Id(rs.getObject("id", java.util.UUID.class)),
                number);
            return ids.stream().findFirst();
        }

        @Transactional(readOnly = true)
        public Money revenue(Currency currency, Instant from, Instant to) {
            List<Money> totals = query(
                "SELECT total FROM orders WHERE currency = ? AND created_at BETWEEN ? AND ?",
                rs -> Money.of(rs.getBigDecimal("total"), currency),
                currency.name(), Timestamp.from(from), Timestamp.from(to));
            return totals.stream().collect(Money.summing(currency));
        }
    }

    /** Appends events to the outbox table in the caller's transaction. */
    final class Outbox {
        private final List<OrderEvent> buffer = new ArrayList<>();

        public void append(OrderEvent event) {
            buffer.add(event);
        }

        public List<OrderEvent> drain() {
            List<OrderEvent> out = List.copyOf(buffer);
            buffer.clear();
            return out;
        }

        public int size() {
            return buffer.size();
        }
    }
}
