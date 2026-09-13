namespace Billing {
    public class Program {
        public static void Main(string[] args) {
            var inv = new Invoice();
            inv.AddItem(10.0);
            inv.AddItem(10.0, 0.5);
        }
    }
}
