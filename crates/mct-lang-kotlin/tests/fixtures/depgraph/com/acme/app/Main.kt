package com.acme.app

import com.acme.model.Money
import com.acme.util.Strings as S
import com.acme.util.trim
import kotlin.collections.List

fun main() {
    val all: List<Money> = listOf(Money(trim(S.name)))
}
