package billing;

public class Main {
    public static void main(String[] args) {
        Invoice inv = new Invoice();
        inv.addItem(10.0);
        inv.addItem(10.0, 0.5);
    }
}
