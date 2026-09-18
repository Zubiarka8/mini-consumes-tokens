import { formatAmount, LedgerLine } from "./format";

export function postAmount(account: string, cents: number): string {
  const line: LedgerLine = { account, cents };
  return renderLine(line);
}

export function renderLine(line: LedgerLine): string {
  return line.account + ": " + formatAmount(line.cents);
}
