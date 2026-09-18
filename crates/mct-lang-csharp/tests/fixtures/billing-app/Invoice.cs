namespace Billing {
    public class Invoice {
        private double total;

        public double AddItem(double price) {
            total += price;
            Logger.Log("item added");
            return total;
        }

        public double AddItem(double price, double taxRate) {
            double taxed = price * (1 + taxRate);
            total += taxed;
            Logger.Log("item added with tax");
            return total;
        }
    }
}
