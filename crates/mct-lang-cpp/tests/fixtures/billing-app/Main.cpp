#include "Invoice.h"

int main() {
    Invoice inv;
    inv.addItem(10.0);
    inv.addItem(10.0, 0.5);
    return 0;
}
