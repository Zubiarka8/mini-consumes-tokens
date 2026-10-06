package com.example.warehouse.pricing

import com.example.warehouse.money.Currency
import com.example.warehouse.money.ExchangeRates
import com.example.warehouse.money.Money
import com.example.warehouse.money.distributeDiscount
import com.example.warehouse.money.gross
import com.example.warehouse.money.sum
import com.example.warehouse.money.vat
import com.example.warehouse.order.Line
import java.math.BigDecimal
import java.time.LocalDate

/**
 * Pricing turns a basket into a quote: base prices, price lists, discount
 * rules and VAT. Rules are composed with a small DSL:
 *
 * ```
 * val engine = pricing {
 *     volume { 10 to 2; 50 to 5 }
 *     loyalty("gold", 6)
 * }
 * ```
 */

/** A priced basket ready to be shown to the customer. */
data class Quote(
    val lines: List<QuotedLine>,
    val currency: Currency,
) {
    val net: Money get() = lines.map { it.net }.sum(currency)
    val vat: Money get() = lines.map { it.vat }.sum(currency)
    val gross: Money get() = net + vat
    val discount: Money get() = lines.map { it.discount }.sum(currency)
}

data class QuotedLine(val line: Line, val net: Money, val discount: Money, val vat: Money)

/** What a rule sees: the basket and who is buying. */
data class PricingContext(val lines: List<Line>, val customerTier: String?, val date: LocalDate)

/** A discount rule: returns the discount for the whole basket, or zero. */
fun interface DiscountRule {
    fun discount(context: PricingContext, subtotal: Money): Money

    infix fun then(next: DiscountRule): DiscountRule = DiscountRule { ctx, subtotal ->
        val first = discount(ctx, subtotal)
        first + next.discount(ctx, subtotal - first)
    }
}

/** VAT rates by product category. */
enum class VatCategory(val percent: Int) {
    GENERAL(21),
    FOOD(10),
    BOOKS(4),
    EXEMPT(0);

    companion object {
        fun forSku(sku: String, catalog: Map<String, VatCategory>): VatCategory = catalog[sku] ?: GENERAL
    }
}

// ---------------------------------------------------------------------------
// Rules
// ---------------------------------------------------------------------------

/** Percentage off when the basket has at least N units. */
class VolumeDiscount(private val thresholds: Map<Int, Int>) : DiscountRule {
    override fun discount(context: PricingContext, subtotal: Money): Money {
        val units = context.lines.sumOf { it.quantity }
        val percent = thresholds.filterKeys { units >= it }.values.maxOrNull() ?: return Money.zero(subtotal.currency)
        return subtotal percentOfInt percent
    }
}

/** Percentage off by loyalty tier. */
class LoyaltyDiscount(private val tiers: Map<String, Int>) : DiscountRule {
    override fun discount(context: PricingContext, subtotal: Money): Money {
        val percent = context.customerTier?.let(tiers::get) ?: 0
        return subtotal percentOfInt percent
    }
}

/** A fixed amount off above a minimum, e.g. "10 € off above 100 €". */
class CouponDiscount(private val code: String, private val off: Money, private val minimum: Money) : DiscountRule {
    override fun discount(context: PricingContext, subtotal: Money): Money =
        if (subtotal >= minimum) off.min(subtotal) else Money.zero(subtotal.currency)

    override fun toString(): String = "coupon $code"
}

private infix fun Money.percentOfInt(percent: Int): Money = this percentOf percent.toBigDecimal()

private fun Money.min(other: Money): Money = if (this <= other) this else other

// ---------------------------------------------------------------------------
// DSL
// ---------------------------------------------------------------------------

@DslMarker
annotation class PricingDsl

@PricingDsl
class PricingBuilder {
    private val rules = mutableListOf<DiscountRule>()
    var vatCatalog: Map<String, VatCategory> = emptyMap()

    fun volume(block: VolumeBuilder.() -> Unit) {
        rules += VolumeDiscount(VolumeBuilder().apply(block).thresholds)
    }

    fun loyalty(tier: String, percent: Int) {
        val existing = rules.filterIsInstance<LoyaltyDiscount>()
        rules.removeAll(existing)
        rules += LoyaltyDiscount(mapOf(tier to percent))
    }

    fun coupon(code: String, off: Money, minimum: Money) {
        rules += CouponDiscount(code, off, minimum)
    }

    fun rule(rule: DiscountRule) {
        rules += rule
    }

    fun build(rates: ExchangeRates): PricingEngine =
        PricingEngine(rules.reduceOrNull { a, b -> a then b } ?: NO_DISCOUNT, vatCatalog, rates)
}

@PricingDsl
class VolumeBuilder {
    internal val thresholds = mutableMapOf<Int, Int>()

    infix fun Int.to(percent: Int) {
        thresholds[this] = percent
    }
}

val NO_DISCOUNT = DiscountRule { _, subtotal -> Money.zero(subtotal.currency) }

fun pricing(rates: ExchangeRates = ExchangeRates.identity(), block: PricingBuilder.() -> Unit): PricingEngine =
    PricingBuilder().apply(block).build(rates)

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

class PricingEngine(
    private val rule: DiscountRule,
    private val vatCatalog: Map<String, VatCategory>,
    private val rates: ExchangeRates,
) {
    /** Prices [lines] in [currency]; line prices in another currency are converted. */
    fun quote(lines: List<Line>, currency: Currency, customerTier: String? = null, date: LocalDate = LocalDate.now()): Quote {
        val converted = lines.map { it.copy(unitPrice = it.unitPrice.convert(currency, rates, date)) }
        val subtotals = converted.map { it.subtotal }
        val subtotal = subtotals.sum(currency)
        val discount = rule.discount(PricingContext(converted, customerTier, date), subtotal)
        val perLine = distributeDiscount(subtotals, discount)
        val quoted = converted.zip(perLine) { line, lineDiscount ->
            val net = line.subtotal - lineDiscount
            val category = VatCategory.forSku(line.sku, vatCatalog)
            QuotedLine(line, net, lineDiscount, vat(net, category.percent))
        }
        return Quote(quoted, currency)
    }

    /** The gross price of one unit, for the shop's product page. */
    fun shelfPrice(sku: String, unitPrice: Money): Money =
        gross(unitPrice, VatCategory.forSku(sku, vatCatalog).percent)

    /** Explains a quote line by line, for support. */
    fun explain(quote: Quote): List<String> = quote.lines.map { q ->
        buildString {
            append(q.line.sku).append(": ").append(q.line.quantity).append(" × ").append(q.line.unitPrice)
            if (!q.discount.isZero) append(" − ").append(q.discount)
            append(" + VAT ").append(q.vat)
        }
    } + "total ${quote.gross}"
}

// ---------------------------------------------------------------------------
// Promotions
// ---------------------------------------------------------------------------

/** A promotion shown in the shop; each kind turns into a [DiscountRule]. */
sealed class Promotion(val title: String) {
    abstract fun toRule(): DiscountRule

    class PercentOff(title: String, private val percent: Int) : Promotion(title) {
        override fun toRule() = DiscountRule { _, subtotal -> subtotal percentOfInt percent }
    }

    class BuyXGetY(title: String, private val sku: String, private val buy: Int, private val free: Int) : Promotion(title) {
        override fun toRule() = DiscountRule { ctx, subtotal ->
            val line = ctx.lines.firstOrNull { it.sku == sku } ?: return@DiscountRule Money.zero(subtotal.currency)
            val freeUnits = line.quantity / (buy + free) * free
            line.unitPrice * freeUnits
        }
    }

    object FreeShipping : Promotion("Envío gratis") {
        override fun toRule() = NO_DISCOUNT
    }
}

/** Price changes of one SKU over time, newest last. */
class PriceHistory(private val sku: String) {
    private val changes = mutableListOf<Pair<LocalDate, Money>>()

    fun record(from: LocalDate, price: Money) {
        require(changes.isEmpty() || changes.last().first < from) { "changes must be in date order" }
        changes += from to price
    }

    fun priceOn(date: LocalDate): Money? = changes.lastOrNull { (from, _) -> from <= date }?.second

    /** The lowest price of the last 30 days, required by EU law next to a discount. */
    fun lowestBefore(date: LocalDate, days: Long = 30): Money? =
        changes.filter { (from, _) -> from > date.minusDays(days) && from <= date }
            .map { it.second }
            .minByOrNull { it.amount }

    override fun toString() = "PriceHistory($sku, ${changes.size} changes)"
}

/** Convenience for tests: a price list from pairs. */
fun priceList(currency: Currency, vararg prices: Pair<String, String>): Map<String, Money> =
    prices.associate { (sku, amount) -> sku to Money.of(BigDecimal(amount), currency) }

/** A row of the ERP's price export that could not be read. */
data class ImportError(val lineNumber: Int, val text: String, val reason: String)

/**
 * Reads the ERP's `sku;amount;currency` export. Bad rows are collected, not
 * fatal: a price list with one broken row still updates every other SKU.
 */
fun importPriceList(csv: String): Pair<Map<String, Money>, List<ImportError>> {
    val prices = mutableMapOf<String, Money>()
    val errors = mutableListOf<ImportError>()
    csv.lineSequence().withIndex()
        .filter { (_, text) -> text.isNotBlank() && !text.startsWith("#") }
        .forEach { (index, text) ->
            val fields = text.split(';').map(String::trim)
            try {
                require(fields.size == 3) { "expected 3 fields, got ${fields.size}" }
                val (sku, amount, code) = fields
                prices[sku] = Money.of(BigDecimal(amount), Currency.parse(code))
            } catch (e: IllegalArgumentException) {
                errors += ImportError(index + 1, text, e.message ?: "invalid row")
            }
        }
    return prices to errors
}

/** Validates that every SKU of a basket has a price. */
fun missingPrices(lines: List<String>, prices: Map<String, Money>): List<String> =
    lines.filterNot(prices::containsKey)

/** A price list in another currency, converted at the reference rate of [on]. */
fun Map<String, Money>.convertedTo(currency: Currency, rates: ExchangeRates, on: LocalDate): Map<String, Money> =
    mapValues { (_, price) -> price.convert(currency, rates, on) }

/** Rounds a quote's gross total to the nearest 5 cents (cash payments). */
fun Quote.cashTotal(): Money {
    val cents = gross.minorUnits
    val rounded = (cents + 2) / 5 * 5
    return Money.ofMinor(rounded, currency)
}

/** Describes the rule chain, for the back office. */
fun describeRules(vararg rules: DiscountRule): String = rules.joinToString(" → ") { rule ->
    when (rule) {
        is VolumeDiscount -> "volume"
        is LoyaltyDiscount -> "loyalty"
        is CouponDiscount -> rule.toString()
        else -> rule::class.simpleName ?: "lambda"
    }
}

/** A seasonal rule built from a lambda, for the Black Friday campaign. */
fun seasonal(from: LocalDate, to: LocalDate, percent: Int): DiscountRule = DiscountRule { ctx, subtotal ->
    if (ctx.date in from..to) subtotal percentOfInt percent else Money.zero(subtotal.currency)
}

/** The default engine of the shop. */
fun defaultEngine(rates: ExchangeRates): PricingEngine = pricing(rates) {
    volume {
        10 to 2
        50 to 5
        200 to 9
    }
    loyalty("gold", 6)
    rule(seasonal(LocalDate.of(2026, 11, 27), LocalDate.of(2026, 11, 30), 15))
    vatCatalog = mapOf("sku-2001" to VatCategory.FOOD, "sku-2101" to VatCategory.FOOD)
}

/** A quick check used by the readiness probe. */
fun selfCheck(engine: PricingEngine): Boolean {
    val one = Line(1, "sku-1001", 1, Money.of(BigDecimal("10.00"), Currency.EUR))
    val quote = engine.quote(listOf(one), Currency.EUR)
    return quote.gross > quote.net
}
