#include "Invoice.h"
#include "Logger.h"

double Invoice::addItem(double price) {
    total += price;
    Logger::log("item added");
    return total;
}

double Invoice::addItem(double price, double taxRate) {
    double taxed = price * (1 + taxRate);
    total += taxed;
    Logger::log("item added with tax");
    return total;
}
