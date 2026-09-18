package jvm

/** In-memory ledger store used by the Kotlin layer. */
class Repository {
    private val rows = mutableListOf<Int>()

    fun store(cents: Int): Int {
        rows.add(cents)
        return rows.size
    }
}

fun buildRepository(): Repository {
    return Repository()
}
