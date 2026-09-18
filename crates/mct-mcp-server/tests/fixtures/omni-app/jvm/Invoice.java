package jvm;

/** A single billable invoice. */
public class Invoice {
    private int total;

    public int addItem(int cents) {
        this.total = this.total + cents;
        return this.total;
    }

    public int getTotal() {
        return this.total;
    }
}
