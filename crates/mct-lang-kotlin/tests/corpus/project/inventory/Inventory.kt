package com.example.warehouse.inventory

import com.example.warehouse.order.Line
import com.example.warehouse.order.Order
import com.example.warehouse.order.OrderId
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import java.time.Clock
import java.time.Duration
import java.time.Instant
import java.util.UUID
import kotlin.properties.Delegates

/**
 * Stock per product and location, and the reservations that promise it to
 * orders. All mutations go through one [Mutex]: reservation is atomic per
 * order, and a line is split over locations only when no single location
 * can serve it.
 */

/** A shelf address: zone, aisle, level (`A-03-2`). */
data class Location(val zone: String, val aisle: Int, val level: Int) : Comparable<Location> {
    val isChilled: Boolean get() = zone.startsWith("F")

    override fun compareTo(other: Location): Int =
        compareValuesBy(this, other, Location::zone, Location::aisle, Location::level)

    override fun toString() = "%s-%02d-%d".format(zone, aisle, level)

    companion object {
        fun parse(code: String): Location {
            val (zone, aisle, level) = code.split("-").also {
                require(it.size == 3) { "bad location: $code" }
            }
            return Location(zone, aisle.toInt(), level.toInt())
        }
    }
}

/** On hand and reserved quantity of one product at one location. */
class StockLevel(val sku: String, val location: Location, onHand: Int) {
    var onHand: Int = onHand
        private set
    var reserved: Int = 0
        private set

    val available: Int get() = onHand - reserved

    fun tryReserve(quantity: Int): Boolean {
        if (available < quantity) return false
        reserved += quantity
        return true
    }

    fun release(quantity: Int) {
        reserved = (reserved - quantity).coerceAtLeast(0)
    }

    fun consume(quantity: Int) {
        release(quantity)
        onHand -= quantity
    }

    fun receive(quantity: Int) {
        onHand += quantity
    }
}

/** A promise of [quantity] units of a SKU at a location to an order. */
data class Reservation(
    val id: String,
    val orderId: OrderId,
    val sku: String,
    val location: Location,
    val quantity: Int,
    val expiresAt: Instant,
) {
    fun isExpired(now: Instant): Boolean = !expiresAt.isAfter(now)
}

/** No location can serve a line, even split. */
class OutOfStockException(val sku: String, val requested: Int, val available: Int) :
    RuntimeException("$sku: requested $requested, available $available") {
    val shortfall: Int get() = requested - available
}

/** Receives inventory events; the outbox writer implements it. */
interface InventoryListener {
    fun stockReserved(reservation: Reservation)

    fun stockReleased(reservation: Reservation, reason: String)

    fun reorderPointReached(sku: String, available: Int) {}

    companion object {
        val NOOP: InventoryListener = object : InventoryListener {
            override fun stockReserved(reservation: Reservation) {}
            override fun stockReleased(reservation: Reservation, reason: String) {}
        }
    }
}

/** A counted quantity at a location, compared with what the system expects. */
data class CycleCount(val sku: String, val location: Location, val expected: Int, val counted: Int, val reason: String? = null) {
    val difference: Int get() = counted - expected

    val needsReason: Boolean get() = difference != 0 && reason == null

    fun describe(): String = when {
        difference == 0 -> "$sku at $location: ok"
        difference > 0 -> "$sku at $location: $difference over"
        else -> "$sku at $location: ${-difference} short (${reason ?: "no reason"})"
    }
}

/** Moves stock between two locations of the same SKU. */
data class StockTransfer(val sku: String, val from: Location, val to: Location, val quantity: Int) {
    init {
        require(from != to) { "transfer to the same location" }
        require(quantity > 0) { "quantity must be positive" }
    }

    val crossesZones: Boolean get() = from.zone != to.zone
}

/** Sorts locations into a walking route: zone, then aisle, snaking levels. */
fun List<Location>.pickingRoute(): List<Location> =
    sortedWith(compareBy<Location> { it.zone }.thenBy { it.aisle }.thenBy { if (it.aisle % 2 == 0) -it.level else it.level })

/** Splits counts into the ones that need a supervisor and the rest. */
fun List<CycleCount>.partitionForReview(tolerance: Int = 2): Pair<List<CycleCount>, List<CycleCount>> =
    partition { kotlin.math.abs(it.difference) > tolerance || it.needsReason }

class Inventory(
    private val clock: Clock,
    private val reservationTtl: Duration = Duration.ofMinutes(30),
    private val listener: InventoryListener = InventoryListener.NOOP,
) {
    private val levels = mutableMapOf<String, MutableList<StockLevel>>()
    private val reservations = mutableMapOf<String, Reservation>()
    private val reorderPoints = mutableMapOf<String, Int>()
    private val mutex = Mutex()

    /** Changes are logged; the delegate keeps the last value it saw. */
    var frozen: Boolean by Delegates.observable(false) { _, old, new ->
        if (old != new) println("reservations ${if (new) "frozen" else "unfrozen"}")
    }

    val skuCount: Int by lazy { levels.size }

    // -----------------------------------------------------------------------
    // Stock
    // -----------------------------------------------------------------------

    suspend fun receive(sku: String, location: Location, quantity: Int) = mutex.withLock {
        levelAt(sku, location).receive(quantity)
    }

    fun setReorderPoint(sku: String, point: Int) {
        reorderPoints[sku] = point
    }

    fun available(sku: String): Int = levelsOf(sku).sumOf { it.available }

    fun availableByLocation(sku: String): Map<Location, Int> =
        levelsOf(sku).filter { it.available > 0 }.associate { it.location to it.available }

    private fun levelsOf(sku: String): List<StockLevel> = levels[sku].orEmpty()

    private fun levelAt(sku: String, location: Location): StockLevel {
        val list = levels.getOrPut(sku) { mutableListOf() }
        return list.firstOrNull { it.location == location }
            ?: StockLevel(sku, location, 0).also { list += it }
    }

    // -----------------------------------------------------------------------
    // Reservations
    // -----------------------------------------------------------------------

    /** Reserves every line of [order], all or nothing; returns reservation ids. */
    suspend fun reserve(order: Order): List<String> = mutex.withLock {
        check(!frozen) { "reservations are frozen" }
        val made = mutableListOf<Reservation>()
        try {
            for (line in order.lines) {
                made += reserveLine(order.id, line)
            }
        } catch (e: OutOfStockException) {
            made.forEach { undo(it) }
            throw e
        }
        made.forEach(listener::stockReserved)
        made.map { it.sku }.distinct().forEach(::checkReorderPoint)
        made.map(Reservation::id)
    }

    private fun reserveLine(orderId: OrderId, line: Line): List<Reservation> {
        val expiresAt = clock.instant() + reservationTtl
        val single = levelsOf(line.sku)
            .filter { it.available >= line.quantity }
            .maxByOrNull { it.available }
        if (single != null && single.tryReserve(line.quantity)) {
            return listOf(newReservation(orderId, line.sku, single.location, line.quantity, expiresAt))
        }
        return splitLine(orderId, line, expiresAt)
    }

    private fun splitLine(orderId: OrderId, line: Line, expiresAt: Instant): List<Reservation> {
        var remaining = line.quantity
        val parts = mutableListOf<Reservation>()
        for (level in levelsOf(line.sku).sortedByDescending { it.available }) {
            if (remaining == 0) break
            val take = minOf(remaining, level.available)
            if (take > 0 && level.tryReserve(take)) {
                parts += newReservation(orderId, line.sku, level.location, take, expiresAt)
                remaining -= take
            }
        }
        if (remaining > 0) {
            parts.forEach { undo(it) }
            throw OutOfStockException(line.sku, line.quantity, line.quantity - remaining)
        }
        return parts
    }

    private fun newReservation(orderId: OrderId, sku: String, location: Location, quantity: Int, expiresAt: Instant) =
        Reservation(UUID.randomUUID().toString(), orderId, sku, location, quantity, expiresAt)
            .also { reservations[it.id] = it }

    suspend fun release(orderId: OrderId, reason: String) = mutex.withLock {
        reservationsOf(orderId).forEach {
            undo(it)
            listener.stockReleased(it, reason)
        }
    }

    suspend fun consume(orderId: OrderId) = mutex.withLock {
        for (r in reservationsOf(orderId)) {
            levelAt(r.sku, r.location).consume(r.quantity)
            reservations.remove(r.id)
        }
    }

    /** Releases expired reservations; returns how many. */
    suspend fun expire(): Int = mutex.withLock {
        val now = clock.instant()
        val due = reservations.values.filter { it.isExpired(now) }
        due.forEach {
            undo(it)
            listener.stockReleased(it, "expired")
        }
        due.size
    }

    /** Runs [expire] every [period] until the scope is cancelled. */
    fun startExpiryJob(scope: CoroutineScope, period: Duration = Duration.ofMinutes(1)) = scope.launch {
        while (isActive) {
            val released = expire()
            if (released > 0) println("released $released expired reservation(s)")
            delay(period.toMillis())
        }
    }

    private fun reservationsOf(orderId: OrderId): List<Reservation> =
        reservations.values.filter { it.orderId == orderId }

    private fun undo(reservation: Reservation) {
        levelAt(reservation.sku, reservation.location).release(reservation.quantity)
        reservations.remove(reservation.id)
    }

    private fun checkReorderPoint(sku: String) {
        val point = reorderPoints[sku] ?: return
        val available = available(sku)
        if (available < point) listener.reorderPointReached(sku, available)
    }

    /** Applies a cycle count: adjusts on-hand stock to the counted quantity. */
    suspend fun applyCount(count: CycleCount) = mutex.withLock {
        check(!count.needsReason) { "a difference needs a reason: ${count.describe()}" }
        val level = levelAt(count.sku, count.location)
        if (count.difference > 0) level.receive(count.difference) else level.consume(-count.difference)
    }

    /** Moves stock; refuses to move reserved units. */
    suspend fun transfer(t: StockTransfer) = mutex.withLock {
        val source = levelAt(t.sku, t.from)
        if (source.available < t.quantity) throw OutOfStockException(t.sku, t.quantity, source.available)
        source.consume(t.quantity)
        levelAt(t.sku, t.to).receive(t.quantity)
    }

    /** Weekly recomputation of a reorder point from daily demand. */
    fun recomputeReorderPoint(sku: String, dailyDemand: List<Int>, leadTimeDays: Int, z: Double = 1.88): Int {
        fun mean(xs: List<Int>) = xs.average()
        fun stdDev(xs: List<Int>): Double {
            val m = mean(xs)
            return kotlin.math.sqrt(xs.sumOf { (it - m) * (it - m) } / xs.size)
        }
        val safety = z * stdDev(dailyDemand) * kotlin.math.sqrt(leadTimeDays.toDouble())
        val point = (mean(dailyDemand) * leadTimeDays + safety).toInt()
        setReorderPoint(sku, point)
        return point
    }
}
