#include "arithmetic.hpp"

int accumulate_cents(int base, int delta) {
    return base + delta;
}

int settle_totals(int base) {
    return accumulate_cents(base, 100);
}
