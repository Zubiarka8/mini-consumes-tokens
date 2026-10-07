package com.example.warehouse.app

import com.example.warehouse.inventory.Inventory
import com.example.warehouse.inventory.InventoryListener
import com.example.warehouse.inventory.Location
import com.example.warehouse.inventory.OutOfStockException
import com.example.warehouse.inventory.Reservation
import com.example.warehouse.money.Currency
import com.example.warehouse.money.ExchangeRates
import com.example.warehouse.money.Money
import com.example.warehouse.money.eur
import com.example.warehouse.order.EventSink
import com.example.warehouse.order.Line
import com.example.warehouse.order.Order
import com.example.warehouse.order.OrderEvent
import com.example.warehouse.order.OrderId
import com.example.warehouse.order.countByState
import com.example.warehouse.order.describe
import com.example.warehouse.pricing.PricingEngine
import com.example.warehouse.pricing.defaultEngine
import kotlinx.coroutines.runBlocking
import java.time.Clock
import java.time.LocalDate
import java.util.concurrent.atomic.AtomicLong
import com.example.warehouse.order.packingSlip as renderSlip

/**
 * Application service and entry point: places, approves, pays and cancels
 * orders, coordinating [Inventory], [PricingEngine] and the repository.
 */

/** A request from the shop, before prices are known. */
data class PlaceOrder(val customerId: String, val currency: Currency, val quantities: Map<String, Int>, val tier: String? = null) {
    init {
        require(quantities.isNotEmpty()) { "empty basket" }
    }
}

/** What the shop gets back. */
sealed interface Outcome {
    data class Accepted(val id: OrderId, val number: String, val total: Money) : Outcome
    data class PendingApproval(val id: OrderId, val number: String, val total: Money) : Outcome
    data class Rejected(val reason: String) : Outcome
}

/** Storage of orders; an in-memory map in this demo. */
interface OrderRepository {
    fun save(order: Order)
    fun find(id: OrderId): Order?
    fun findByNumber(number: String): Order?
    fun all(): Collection<Order>
}

class InMemoryOrders : OrderRepository {
    private val byId = linkedMapOf<OrderId, Order>()

    override fun save(order: Order) {
        byId[order.id] = order
    }

    override fun find(id: OrderId): Order? = byId[id]

    override fun findByNumber(number: String): Order? = byId.values.firstOrNull { it.number == number }

    override fun all(): Collection<Order> = byId.values
}

/** Generates gapless order numbers per year. */
object OrderNumbers {
    private val sequence = AtomicLong()

    fun next(clock: Clock): String = "PED-%d-%06d".format(LocalDate.now(clock).year, sequence.incrementAndGet())

    fun reset() = sequence.set(0)
}

/** Collects published events; stands in for the outbox. */
class Outbox : EventSink, InventoryListener {
    val events = mutableListOf<OrderEvent>()
    val stock = mutableListOf<String>()

    override fun accept(event: OrderEvent) {
        events += event
    }

    override fun stockReserved(reservation: Reservation) {
        stock += "reserved ${reservation.quantity} × ${reservation.sku} at ${reservation.location}"
    }

    override fun stockReleased(reservation: Reservation, reason: String) {
        stock += "released ${reservation.sku}: $reason"
    }

    override fun reorderPointReached(sku: String, available: Int) {
        stock += "reorder $sku ($available left)"
    }
}

class OrderService(
    private val orders: OrderRepository,
    private val inventory: Inventory,
    private val pricing: PricingEngine,
    private val prices: Map<String, Money>,
    private val rates: ExchangeRates,
    private val outbox: Outbox,
    private val clock: Clock,
    private val largeOrderThreshold: Money = 5_000.eur(),
) {
    suspend fun place(request: PlaceOrder): Outcome {
        val lines = priceLines(request)
        val now = clock.instant()
        val order = Order.place(OrderNumbers.next(clock), request.customerId, request.currency, lines, now)
        val quote = pricing.quote(order.lines, request.currency, request.tier)
        val totalInEuro = quote.gross.convert(Currency.EUR, rates, LocalDate.now(clock))
        if (totalInEuro > largeOrderThreshold) {
            save(order)
            return Outcome.PendingApproval(order.id, order.number, quote.gross)
        }
        return reserveOrReject(order) ?: Outcome.Accepted(order.id, order.number, quote.gross)
    }

    suspend fun approve(id: OrderId, approved: Boolean, supervisor: String): Outcome {
        val order = orders.find(id) ?: return Outcome.Rejected("unknown order $id")
        if (!approved) {
            order.cancel("rejected by $supervisor", clock.instant())
            save(order)
            return Outcome.Rejected("rejected by $supervisor")
        }
        return reserveOrReject(order) ?: Outcome.Accepted(order.id, order.number, order.total)
    }

    fun confirmPayment(number: String, paymentId: String, amount: Money) {
        val order = requireNotNull(orders.findByNumber(number)) { "unknown order $number" }
        order.pay(paymentId, amount, clock.instant())
        save(order)
    }

    suspend fun cancel(id: OrderId, reason: String) {
        val order = orders.find(id) ?: return
        order.cancel(reason, clock.instant())
        inventory.release(id, reason)
        save(order)
    }

    private suspend fun reserveOrReject(order: Order): Outcome? {
        val now = clock.instant()
        return try {
            val ids = inventory.reserve(order)
            order.reserve(ids, now)
            save(order)
            null
        } catch (e: OutOfStockException) {
            order.cancel("out of stock: ${e.sku}", now)
            save(order)
            Outcome.Rejected(e.message ?: "out of stock")
        }
    }

    private fun priceLines(request: PlaceOrder): List<Line> =
        request.quantities.entries.mapIndexed { index, (sku, quantity) ->
            val price = prices[sku] ?: error("no price for $sku")
            Line(index + 1, sku, quantity, price)
        }

    private fun save(order: Order) {
        order.drainEvents(outbox)
        orders.save(order)
    }

    fun dashboard(): String = orders.all().countByState().entries.joinToString("\n") { (state, n) -> "$state: $n" }

    fun packingSlip(number: String): String? = orders.findByNumber(number)?.let(::renderSlip)
}

// ---------------------------------------------------------------------------
// HTTP routes
// ---------------------------------------------------------------------------

/** A minimal request/response pair, enough for the demo router. */
data class Request(val method: String, val path: String, val body: Map<String, String> = emptyMap())

data class Response(val status: Int, val body: String) {
    companion object {
        fun ok(body: String) = Response(200, body)
        fun created(body: String) = Response(201, body)
        fun notFound(what: String) = Response(404, "not found: $what")
        fun badRequest(why: String) = Response(400, why)
    }
}

typealias Handler = suspend (Request) -> Response

/** Maps `METHOD /path` to handlers; path segments starting with `:` are variables. */
class Router {
    private val routes = mutableListOf<Triple<String, List<String>, Handler>>()

    fun get(path: String, handler: Handler) = add("GET", path, handler)

    fun post(path: String, handler: Handler) = add("POST", path, handler)

    private fun add(method: String, path: String, handler: Handler) {
        routes += Triple(method, path.trim('/').split('/'), handler)
    }

    suspend fun handle(request: Request): Response {
        val segments = request.path.trim('/').split('/')
        val match = routes.firstOrNull { (method, pattern, _) ->
            method == request.method && pattern.size == segments.size &&
                pattern.zip(segments).all { (p, s) -> p.startsWith(":") || p == s }
        } ?: return Response.notFound(request.path)
        return match.third(request)
    }
}

/** Scopes a token may carry; see the API reference. */
enum class Scope(val claim: String) {
    ORDERS_READ("orders:read"),
    ORDERS_WRITE("orders:write"),
    STOCK_READ("stock:read");

    companion object {
        fun parse(claims: String): Set<Scope> =
            claims.split(' ').mapNotNull { claim -> entries.firstOrNull { it.claim == claim } }.toSet()
    }
}

/** Wraps a handler so it only runs for tokens carrying [required]. */
fun requireScope(required: Scope, tokens: Map<String, String>, handler: Handler): Handler = { request ->
    val token = request.body["authorization"]?.removePrefix("Bearer ")
    val scopes = token?.let(tokens::get)?.let(Scope::parse).orEmpty()
    when {
        token == null -> Response(401, "missing token")
        required !in scopes -> Response(403, "missing scope ${required.claim}")
        else -> handler(request)
    }
}

/** The routes of the order API, bound to a service. */
fun orderRoutes(service: OrderService): Router = Router().apply {
    get("/orders/dashboard") { Response.ok(service.dashboard()) }
    get("/orders/:number/slip") { req ->
        val number = req.path.split('/')[2]
        service.packingSlip(number)?.let(Response::ok) ?: Response.notFound(number)
    }
    post("/orders") { req ->
        val customer = req.body["customer"] ?: return@post Response.badRequest("customer is required")
        val quantities = req.body.filterKeys { it.startsWith("sku-") }.mapValues { it.value.toInt() }
        when (val outcome = service.place(PlaceOrder(customer, Currency.EUR, quantities))) {
            is Outcome.Accepted -> Response.created(outcome.number)
            is Outcome.PendingApproval -> Response(202, outcome.number)
            is Outcome.Rejected -> Response(409, outcome.reason)
        }
    }
}

// ---------------------------------------------------------------------------
// Wiring
// ---------------------------------------------------------------------------

/** Command-line flags of the demo. */
private data class Flags(val expire: Boolean = false, val verbose: Boolean = false, val tier: String? = null)

private fun parseFlags(args: Array<String>): Flags = args.fold(Flags()) { flags, arg ->
    when {
        arg == "--expire" -> flags.copy(expire = true)
        arg == "-v" || arg == "--verbose" -> flags.copy(verbose = true)
        arg.startsWith("--tier=") -> flags.copy(tier = arg.substringAfter('='))
        else -> error("unknown flag $arg")
    }
}

fun demoService(clock: Clock = Clock.systemUTC()): Pair<OrderService, Outbox> {
    val outbox = Outbox()
    val rates = ExchangeRates.identity()
    val inventory = Inventory(clock, listener = outbox).apply {
        setReorderPoint("sku-1001", 10)
    }
    runBlocking {
        inventory.receive("sku-1001", Location.parse("A-03-2"), 40)
        inventory.receive("sku-2001", Location.parse("C-02-1"), 960)
    }
    val prices = mapOf("sku-1001" to Money.of("24.90", Currency.EUR), "sku-2001" to Money.of("9.80", Currency.EUR))
    val service = OrderService(InMemoryOrders(), inventory, defaultEngine(rates), prices, rates, outbox, clock)
    return service to outbox
}

fun main(args: Array<String>) = runBlocking {
    val flags = parseFlags(args)
    val (service, outbox) = demoService()
    val outcome = service.place(PlaceOrder("cus_8812", Currency.EUR, mapOf("sku-1001" to 2, "sku-2001" to 6), flags.tier))
    val message = when (outcome) {
        is Outcome.Accepted -> "accepted ${outcome.number} for ${outcome.total}"
        is Outcome.PendingApproval -> "waiting for approval: ${outcome.number}"
        is Outcome.Rejected -> "rejected: ${outcome.reason}"
    }
    println(message)
    if (flags.verbose) {
        outbox.events.map(::describe).forEach(::println)
        outbox.stock.forEach { println("  $it") }
        println(service.dashboard())
    }
    if (outcome is Outcome.Accepted) {
        service.packingSlip(outcome.number)?.let(::println)
    }
}
