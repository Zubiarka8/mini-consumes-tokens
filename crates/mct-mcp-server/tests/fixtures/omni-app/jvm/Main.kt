package jvm

fun runOnce(cents: Int): Int {
    val repository = buildRepository()
    return repository.store(cents)
}
