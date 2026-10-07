package com.example.warehouse.money

import java.math.BigDecimal
import java.math.MathContext
import java.math.RoundingMode
import java.text.NumberFormat
import java.time.LocalDate
import java.util.Locale
import java.util.Currency as JavaCurrency

/**
 * Money for the warehouse: a decimal amount with an explicit currency, rounded
 * once with banker's rounding (ADR-005). Arithmetic across currencies throws.
 */

typealias Rate = BigDecimal

/** ISO 4217 currencies we trade in, with their minor units. */
enum class Currency(val scale: Int, val symbol: String) {
    EUR(2, "€"),
    USD(2, "$"),
    GBP(2, "£"),
    JPY(0, "¥") {
        override fun format(amount: BigDecimal): String = "$symbol${amount.toPlainString()}"
    };

    open fun format(amount: BigDecimal): String = "${amount.toPlainString()} $symbol"

    fun javaCurrency(): JavaCurrency = JavaCurrency.getInstance(name)

    companion object {
        fun parse(code: String): Currency =
            entries.firstOrNull { it.name.equals(code.trim(), ignoreCase = true) }
                ?: throw IllegalArgumentException("unsupported currency: $code")
    }
}

/** Thrown when two amounts of different currencies meet. */
class CurrencyMismatchException(val expected: Currency, val actual: Currency) :
    IllegalArgumentException("currency mismatch: expected $expected, got $actual")

/** Thrown by [Money.parse] for unreadable input. */
class MoneyFormatException(input: String, reason: String?) :
    IllegalArgumentException("cannot parse \"$input\" as money: $reason")

/**
 * An immutable amount. Equality ignores trailing zeros: `1.0 EUR == 1.00 EUR`.
 */
class Money private constructor(amount: BigDecimal, val currency: Currency) : Comparable<Money> {

    val amount: BigDecimal = amount.setScale(currency.scale, RoundingMode.HALF_EVEN)

    val minorUnits: Long
        get() = amount.movePointRight(currency.scale).longValueExact()

    val isZero: Boolean get() = amount.signum() == 0
    val isNegative: Boolean get() = amount.signum() < 0

    operator fun plus(other: Money): Money {
        requireSameCurrency(other)
        return Money(amount + other.amount, currency)
    }

    operator fun minus(other: Money): Money {
        requireSameCurrency(other)
        return Money(amount - other.amount, currency)
    }

    operator fun times(factor: Int): Money = Money(amount * factor.toBigDecimal(), currency)

    operator fun times(factor: BigDecimal): Money = Money(amount * factor, currency)

    operator fun unaryMinus(): Money = Money(amount.negate(), currency)

    infix fun percentOf(rate: BigDecimal): Money = times(rate.movePointLeft(2))

    fun split(parts: Int): List<Money> {
        require(parts > 0) { "parts must be positive: $parts" }
        val share = minorUnits / parts
        val remainder = minorUnits % parts
        return List(parts) { i -> ofMinor(share + if (i < remainder) 1 else 0, currency) }
    }

    fun allocate(vararg weights: Int): List<Money> {
        val total = weights.sum()
        require(total > 0) { "weights add up to zero" }
        require(weights.none { it < 0 }) { "negative weight" }
        val shares = weights.map { w -> minorUnits * w / total }.toMutableList()
        var leftover = minorUnits - shares.sum()
        var i = 0
        while (leftover > 0) {
            shares[i % shares.size] = shares[i % shares.size] + 1
            leftover--
            i++
        }
        return shares.map { ofMinor(it, currency) }
    }

    fun convert(to: Currency, rates: ExchangeRates, on: LocalDate): Money =
        if (to == currency) this else Money(amount.multiply(rates.rate(currency, to, on), MathContext.DECIMAL64), to)

    fun format(locale: Locale = Locale.forLanguageTag("es-ES")): String {
        val formatter = NumberFormat.getCurrencyInstance(locale).apply {
            currency = this@Money.currency.javaCurrency()
        }
        return formatter.format(amount)
    }

    private fun requireSameCurrency(other: Money) {
        if (other.currency != currency) throw CurrencyMismatchException(currency, other.currency)
    }

    override fun compareTo(other: Money): Int {
        requireSameCurrency(other)
        return amount.compareTo(other.amount)
    }

    override fun equals(other: Any?): Boolean =
        other is Money && other.currency == currency && other.amount.compareTo(amount) == 0

    override fun hashCode(): Int = 31 * amount.stripTrailingZeros().hashCode() + currency.hashCode()

    override fun toString(): String = "${amount.toPlainString()} ${currency.name}"

    companion object Factory {
        fun of(amount: BigDecimal, currency: Currency): Money = Money(amount, currency)

        fun of(amount: String, currency: Currency): Money = of(BigDecimal(amount), currency)

        fun ofMinor(units: Long, currency: Currency): Money = Money(BigDecimal.valueOf(units, currency.scale), currency)

        fun zero(currency: Currency): Money = of(BigDecimal.ZERO, currency)

        /** Parses `"24.90 EUR"` or `"EUR 24.90"`. */
        fun parse(text: String): Money {
            val parts = text.trim().split(Regex("\\s+"))
            if (parts.size != 2) throw MoneyFormatException(text, "expected an amount and a currency")
            return try {
                val (first, second) = parts
                if (first.first().isLetter()) of(second, Currency.parse(first)) else of(first, Currency.parse(second))
            } catch (e: NumberFormatException) {
                throw MoneyFormatException(text, e.message)
            }
        }
    }
}

/** A closed range of amounts, used by catalog price filters. */
data class MoneyRange(val min: Money, val max: Money) {
    init {
        require(min <= max) { "$min > $max" }
    }

    operator fun contains(value: Money): Boolean = value >= min && value <= max

    val width: Money get() = max - min
}

// ---------------------------------------------------------------------------
// Exchange rates
// ---------------------------------------------------------------------------

/** Converts between currencies at the reference rate of a date. */
fun interface ExchangeRates {
    fun rate(from: Currency, to: Currency, on: LocalDate): Rate

    companion object {
        fun identity(): ExchangeRates = ExchangeRates { from, to, on ->
            if (from != to) throw IllegalStateException("no rate $from→$to on $on")
            BigDecimal.ONE
        }

        fun perEuro(rates: Map<Currency, Rate>): ExchangeRates = FixedRates(rates + (Currency.EUR to BigDecimal.ONE))
    }
}

/** Rates fixed at construction, quoted per euro. */
private class FixedRates(private val perEuro: Map<Currency, Rate>) : ExchangeRates {
    override fun rate(from: Currency, to: Currency, on: LocalDate): Rate {
        val fromRate = perEuro[from] ?: missing(from, to, on)
        val toRate = perEuro[to] ?: missing(to, from, on)
        return toRate.divide(fromRate, MathContext.DECIMAL64)
    }

    private fun missing(a: Currency, b: Currency, on: LocalDate): Nothing =
        throw IllegalStateException("no rate $a→$b on $on")
}

/** Caches another source's rates per date; not thread-safe on purpose. */
class CachingRates(private val source: ExchangeRates) : ExchangeRates by source {
    private val cache = mutableMapOf<Triple<Currency, Currency, LocalDate>, Rate>()
    var hits = 0
        private set

    override fun rate(from: Currency, to: Currency, on: LocalDate): Rate {
        val key = Triple(from, to, on)
        cache[key]?.let {
            hits++
            return it
        }
        return source.rate(from, to, on).also { cache[key] = it }
    }

    fun clear() = cache.clear()
}

// ---------------------------------------------------------------------------
// Extensions and top-level helpers
// ---------------------------------------------------------------------------

/** Sums amounts of one currency; an empty collection gives zero. */
fun Iterable<Money>.sum(currency: Currency): Money = fold(Money.zero(currency)) { acc, m -> acc + m }

fun Iterable<Money>.maxOrNull(): Money? = maxWithOrNull(compareBy { it.amount })

val Money.isPositive: Boolean
    get() = amount.signum() > 0

fun String.toMoney(): Money = Money.parse(this)

fun Int.eur(): Money = Money.of(toBigDecimal(), Currency.EUR)

fun BigDecimal.eur(): Money = Money.of(this, Currency.EUR)

/** Rounds a rate for display, e.g. `1.0870 USD per EUR`. */
fun describeRate(from: Currency, to: Currency, rate: Rate): String =
    "${rate.setScale(4, RoundingMode.HALF_UP).toPlainString()} $to per $from"

/** Value-added tax for an amount at a rate, rounded per line. */
fun vat(net: Money, ratePercent: Int): Money = net percentOf ratePercent.toBigDecimal()

/** The gross amount of a net amount plus VAT. */
fun gross(net: Money, ratePercent: Int): Money = net + vat(net, ratePercent)

/** Distributes a discount over lines proportionally to their subtotals. */
fun distributeDiscount(subtotals: List<Money>, discount: Money): List<Money> {
    if (subtotals.isEmpty()) return emptyList()
    val weights = subtotals.map { it.minorUnits.toInt() }.toIntArray()
    return discount.allocate(*weights)
}

// ---------------------------------------------------------------------------
// Budgets
// ---------------------------------------------------------------------------

/**
 * A spending budget per cost centre, in one currency. Indexed access reads
 * and writes a centre's limit; [spend] records an expense and refuses one
 * that would overrun the limit.
 */
class Budget(val currency: Currency) {
    private val limits = mutableMapOf<String, Money>()
    private val spent = mutableMapOf<String, Money>()

    operator fun get(centre: String): Money = limits[centre] ?: Money.zero(currency)

    operator fun set(centre: String, limit: Money) {
        require(limit.currency == currency) { "budget is in $currency" }
        limits[centre] = limit
    }

    operator fun contains(centre: String): Boolean = centre in limits

    fun spent(centre: String): Money = spent[centre] ?: Money.zero(currency)

    fun remaining(centre: String): Money = this[centre] - spent(centre)

    fun spend(centre: String, amount: Money): Boolean {
        if (amount > remaining(centre)) return false
        spent[centre] = spent(centre) + amount
        return true
    }

    fun overview(): List<String> = limits.keys.sorted().map { centre ->
        "$centre: ${spent(centre)} of ${this[centre]} (${remaining(centre)} left)"
    }
}

/** Running statistics over a stream of amounts, e.g. order totals. */
class MoneyStats(private val currency: Currency) {
    private var count = 0
    private var total = Money.zero(currency)
    private var largest: Money? = null

    fun add(amount: Money) {
        count++
        total += amount
        largest = largest?.let { if (amount > it) amount else it } ?: amount
    }

    val average: Money
        get() = if (count == 0) Money.zero(currency) else Money.of(total.amount.divide(count.toBigDecimal(), MathContext.DECIMAL64), currency)

    fun report(): String = "n=$count total=$total avg=$average max=${largest ?: "-"}"
}

/** Groups amounts by currency and sums each group. */
fun Iterable<Money>.totalsByCurrency(): Map<Currency, Money> =
    groupBy { it.currency }.mapValues { (currency, amounts) -> amounts.sum(currency) }

/** A short audit string for logs. */
fun auditLine(label: String, vararg amounts: Money): String =
    amounts.joinToString(prefix = "$label: ", separator = ", ") { it.format(Locale.ROOT) }
