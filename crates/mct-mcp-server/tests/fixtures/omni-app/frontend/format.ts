export function formatAmount(cents: number): string {
  return (cents / 100).toFixed(2);
}

export interface LedgerLine {
  account: string;
  cents: number;
}

export class LedgerView {
  render(line: LedgerLine): string {
    return line.account + " " + formatAmount(line.cents);
  }
}
