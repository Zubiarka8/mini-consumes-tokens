import { Invoice } from "./invoice";

function useInvoiceTotal(): number {
    const invoice = new Invoice();
    invoice.addItem(10, 0.1);
    return invoice.total;
}

export function App() {
    const total = useInvoiceTotal();
    return (
        <div>
            <span>{total}</span>
            <button onClick={() => useInvoiceTotal()}>Recompute</button>
        </div>
    );
}
