import { formatTotal } from "./format";

export async function createInvoice(price: number): Promise<number> {
    const response = await fetch("/api/invoices", {
        method: "POST",
        body: JSON.stringify({ price }),
    });
    const data = await response.json();
    return formatTotal(data.total);
}
