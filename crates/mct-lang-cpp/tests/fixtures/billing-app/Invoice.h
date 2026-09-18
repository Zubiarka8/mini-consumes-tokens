#pragma once

class Invoice {
public:
    double addItem(double price);
    double addItem(double price, double taxRate);

private:
    double total = 0.0;
};
