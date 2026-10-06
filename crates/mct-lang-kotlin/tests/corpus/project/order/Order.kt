package com.example.warehouse.order

import com.example.warehouse.money.Currency
import com.example.warehouse.money.CurrencyMismatchException
import com.example.warehouse.money.Money
import com.example.warehouse.money.sum
import java.time.Instant
import java.util.UUID

/**
 * The order aggregate and its events. Commands check the transition, record
 * an event and return it; the application service persists the order and
 * publishes the events through the outbox (ADR-002).
 */

@JvmInline
value class OrderId(val value: UUID) {
    override fun toString(): String = value.toString()

    companion object {
        fun random() = OrderId(UUID.randomUUID())
        fun parse(text: String) = OrderId(UUID.fromString(text))
    }
}

/** One product line. */
data class Line(val number: Int, val sku: String, val quantity: Int, val unitPrice: Money) {
    init {
        require(quantity > 0) { "quantity must be positive: $quantity" }
    }

    val subtotal: Money get() = unitPrice * quantity

    fun withQuantity(newQuantity: Int) = copy(quantity = newQuantity)
}

/** The lifecycle of an order. */
enum class State {
    NEW, RESERVED, PAID, PICKED, SHIPPED, INVOICED, CANCELLED;

    val isTerminal: Boolean get() = this == INVOICED || this == CANCELLED

    fun next(): Set<State> = when (this) {
        NEW -> setOf(RESERVED, CANCELLED)
        RESERVED -> setOf(PAID, CANCELLED)
        PAID -> setOf(PICKED, CANCELLED)
        PICKED -> setOf(SHIPPED, CANCELLED)
        SHIPPED -> setOf(INVOICED)
        INVOICED, CANCELLED -> emptySet()
    }

    fun canMoveTo(target: State): Boolean = target in next()
}

/** Everything that can happen to an order; a closed hierarchy. */
sealed class OrderEvent {
    abstract val orderId: OrderId
    abstract val at: Instant

    val type: String get() = this::class.simpleName ?: "OrderEvent"

    data class Placed(override val orderId: OrderId, override val at: Instant, val customerId: String, val lines: List<Line>) : OrderEvent()
    data class Reserved(override val orderId: OrderId, override val at: Instant, val reservationIds: List<String>) : OrderEvent()
    data class Paid(override val orderId: OrderId, override val at: Instant, val paymentId: String, val amount: Money) : OrderEvent()
    data class Picked(override val orderId: OrderId, override val at: Instant, val picker: String) : OrderEvent()
    data class Shipped(override val orderId: OrderId, override val at: Instant, val carrier: String, val trackingNumber: String) : OrderEvent()
    data class Invoiced(override val orderId: OrderId, override val at: Instant, val invoiceNumber: String) : OrderEvent()
    data class Cancelled(override val orderId: OrderId, override val at: Instant, val reason: String) : OrderEvent()
}

/** A transition that the current state does not allow. */
class IllegalTransitionException(val from: State, val to: State) :
    IllegalStateException("cannot move an order from $from to $to")

/** Receives drained events; implemented by the outbox. */
fun interface EventSink {
    fun accept(event: OrderEvent)
}

class Order private constructor(
    val id: OrderId,
    val number: String,
    val customerId: String,
    val currency: Currency,
) {
    private val _lines = mutableListOf<Line>()
    private val pending = mutableListOf<OrderEvent>()

    var state: State = State.NEW
        private set

    var trackingNumber: String? = null
        private set

    val lines: List<Line> get() = _lines.toList()

    val total: Money get() = _lines.map { it.subtotal }.sum(currency)

    // -----------------------------------------------------------------------
    // Commands
    // -----------------------------------------------------------------------

    fun reserve(reservationIds: List<String>, at: Instant): OrderEvent.Reserved {
        moveTo(State.RESERVED)
        return record(OrderEvent.Reserved(id, at, reservationIds.toList()))
    }

    fun pay(paymentId: String, amount: Money, at: Instant): OrderEvent.Paid {
        require(amount == total) { "paid $amount for a total of $total" }
        moveTo(State.PAID)
        return record(OrderEvent.Paid(id, at, paymentId, amount))
    }

    fun pick(picker: String, at: Instant) = record(OrderEvent.Picked(id, at, picker).also { moveTo(State.PICKED) })

    fun ship(carrier: String, trackingNumber: String, at: Instant): OrderEvent.Shipped {
        moveTo(State.SHIPPED)
        this.trackingNumber = trackingNumber
        return record(OrderEvent.Shipped(id, at, carrier, trackingNumber))
    }

    fun invoice(invoiceNumber: String, at: Instant): OrderEvent.Invoiced {
        moveTo(State.INVOICED)
        return record(OrderEvent.Invoiced(id, at, invoiceNumber))
    }

    fun cancel(reason: String, at: Instant): OrderEvent.Cancelled {
        moveTo(State.CANCELLED)
        return record(OrderEvent.Cancelled(id, at, reason))
    }

    private fun moveTo(target: State) {
        if (!state.canMoveTo(target)) throw IllegalTransitionException(state, target)
        state = target
    }

    private fun <E : OrderEvent> record(event: E): E {
        pending += event
        return event
    }

    private fun addLine(line: Line) {
        if (line.unitPrice.currency != currency) {
            throw CurrencyMismatchException(currency, line.unitPrice.currency)
        }
        _lines += line
    }

    // -----------------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------------

    fun isLarge(threshold: Money): Boolean = total > threshold

    fun drainEvents(sink: EventSink) {
        pending.forEach(sink::accept)
        pending.clear()
    }

    fun summary(): String = """
        |Order $number ($state)
        |  customer: $customerId
        |  lines:    ${_lines.size}
        |  total:    $total
        """.trimMargin()

    override fun toString() = "Order[$number, $state, $total]"

    companion object {
        /** Places a new order; the `Placed` event is pending. */
        fun place(number: String, customerId: String, currency: Currency, lines: List<Line>, at: Instant): Order {
            require(lines.isNotEmpty()) { "an order needs at least one line" }
            return Order(OrderId.random(), number, customerId, currency).apply {
                lines.forEach(::addLine)
                record(OrderEvent.Placed(id, at, customerId, lines.toList()))
            }
        }

        /** Rebuilds an order from storage without emitting events. */
        fun restore(id: OrderId, number: String, customerId: String, currency: Currency, state: State, lines: List<Line>): Order =
            Order(id, number, customerId, currency).also { order ->
                lines.forEach { order.addLine(it) }
                order.state = state
            }
    }
}

/** A one-line description for logs, by event type. */
fun describe(event: OrderEvent): String = when (event) {
    is OrderEvent.Placed -> "placed with ${event.lines.size} line(s)"
    is OrderEvent.Reserved -> "reserved ${event.reservationIds.size} reservation(s)"
    is OrderEvent.Paid -> "paid ${event.amount}"
    is OrderEvent.Picked -> "picked by ${event.picker}"
    is OrderEvent.Shipped -> "shipped with ${event.carrier} (${event.trackingNumber})"
    is OrderEvent.Invoiced -> "invoiced as ${event.invoiceNumber}"
    is OrderEvent.Cancelled -> "cancelled: ${event.reason}"
}

/** Orders grouped by state, for the supervisors' dashboard. */
fun Collection<Order>.countByState(): Map<State, Int> = groupingBy { it.state }.eachCount()

/** The largest orders first. */
fun Collection<Order>.largest(n: Int): List<Order> = sortedByDescending { it.total.amount }.take(n)

/** Orders that have waited for payment longer than [limitMinutes]. */
fun Collection<Order>.stalePayments(now: Instant, placedAt: (Order) -> Instant, limitMinutes: Long = 30): List<Order> {
    fun isStale(order: Order): Boolean {
        val waited = java.time.Duration.between(placedAt(order), now).toMinutes()
        return order.state == State.RESERVED && waited > limitMinutes
    }
    return filter(::isStale)
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/** One problem found by [OrderValidator]. */
data class Violation(val field: String, val message: String)

/** Checks a basket before an order is placed; every rule runs. */
class OrderValidator(
    private val maxLines: Int = 100,
    private val maxQuantity: Int = 999,
    private val restrictedSkus: Set<String> = emptySet(),
) {
    fun validate(customerId: String, lines: List<Line>): List<Violation> = buildList {
        if (customerId.isBlank()) add(Violation("customerId", "required"))
        if (lines.isEmpty()) add(Violation("lines", "at least one line"))
        if (lines.size > maxLines) add(Violation("lines", "at most $maxLines lines"))
        lines.forEach { line -> addAll(validateLine(line)) }
        addAll(duplicateSkus(lines))
    }

    private fun validateLine(line: Line): List<Violation> {
        val field = "lines[${line.number}]"
        return listOfNotNull(
            Violation(field, "quantity above $maxQuantity").takeIf { line.quantity > maxQuantity },
            Violation(field, "${line.sku} needs age verification").takeIf { line.sku in restrictedSkus },
            Violation(field, "negative price").takeIf { line.unitPrice.isNegative },
        )
    }

    private fun duplicateSkus(lines: List<Line>): List<Violation> =
        lines.groupBy { it.sku }
            .filterValues { it.size > 1 }
            .keys
            .map { Violation("lines", "$it appears more than once") }

    fun requireValid(customerId: String, lines: List<Line>) {
        val violations = validate(customerId, lines)
        if (violations.isNotEmpty()) {
            throw IllegalArgumentException(violations.joinToString("; ") { "${it.field}: ${it.message}" })
        }
    }
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

/** The append-only history of an order (ADR-007: no event sourcing). */
class OrderHistory(private val orderId: OrderId) : EventSink {
    private val entries = mutableListOf<OrderEvent>()

    override fun accept(event: OrderEvent) {
        require(event.orderId == orderId) { "event of another order: ${event.orderId}" }
        entries += event
    }

    val size: Int get() = entries.size

    fun lastState(): State? = entries.lastOrNull()?.let(::stateAfter)

    fun lines(): List<String> = entries.map { "${it.at} ${describe(it)}" }

    private fun stateAfter(event: OrderEvent): State = when (event) {
        is OrderEvent.Placed -> State.NEW
        is OrderEvent.Reserved -> State.RESERVED
        is OrderEvent.Paid -> State.PAID
        is OrderEvent.Picked -> State.PICKED
        is OrderEvent.Shipped -> State.SHIPPED
        is OrderEvent.Invoiced -> State.INVOICED
        is OrderEvent.Cancelled -> State.CANCELLED
    }
}

/** Renders a packing slip; nested helper functions keep the layout readable. */
fun packingSlip(order: Order): String {
    val width = 48

    fun rule() = "-".repeat(width)

    fun row(left: String, right: String): String = left.padEnd(width - right.length) + right

    return buildString {
        appendLine(rule())
        appendLine(row("Albarán", order.number))
        appendLine(rule())
        for (line in order.lines) {
            appendLine(row("${line.quantity} × ${line.sku}", line.subtotal.toString()))
        }
        appendLine(rule())
        appendLine(row("Total", order.total.toString()))
    }
}
