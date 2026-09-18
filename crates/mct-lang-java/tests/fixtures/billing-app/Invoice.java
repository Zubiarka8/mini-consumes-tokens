package billing;

public class Invoice {
    private double total;

    public double addItem(double price) {
        total += price;
        Logger.log("item added");
        return total;
    }

    public double addItem(double price, double taxRate) {
        double taxed = price * (1 + taxRate);
        total += taxed;
        Logger.log("item added with tax");
        return total;
    }
}
