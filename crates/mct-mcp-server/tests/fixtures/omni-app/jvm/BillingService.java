package jvm;

import jvm.Invoice;

/** Orchestrates invoice creation. */
public class BillingService {
    public int charge(int cents) {
        Invoice invoice = new Invoice();
        invoice.addItem(cents);
        return invoice.getTotal();
    }
}
