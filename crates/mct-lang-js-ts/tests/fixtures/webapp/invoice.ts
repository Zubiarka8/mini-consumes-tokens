import { log } from "./logger";

const { add } = require("./mathUtils");

interface Priced {
    total: number;
}

const computeTotal = (price: number, taxRate: number): number => {
    return add(price, price * taxRate);
};

export class Invoice implements Priced {
    total = 0;

    addItem(price: number, taxRate: number): number {
        this.total = computeTotal(price, taxRate);
        log("item added");
        return this.total;
    }
}
