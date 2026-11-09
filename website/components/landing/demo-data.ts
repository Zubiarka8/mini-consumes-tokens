// Sample project behind the two interactive demos. Everything is made up.
export type DemoFile = {
  path: string;
  lang: string;
  imports: string[];
  symbols: [name: string, lines: string][];
};
export const demoFiles: DemoFile[] = [
  {
    path: "src/cli.js",
    lang: "javascript",
    imports: ["src/invoice.js"],
    symbols: [
      ["parseArgs", "L4-12"],
      ["parseFlags", "L14-22"],
    ],
  },
  {
    path: "src/invoice.js",
    lang: "javascript",
    imports: ["src/format.js"],
    symbols: [
      ["calculateTotal", "L5-9"],
      ["formatPrice", "L11-18"],
    ],
  },
  {
    path: "src/format.js",
    lang: "javascript",
    imports: ["src/currency.js"],
    symbols: [
      ["formatMoney", "L3-12"],
      ["roundTo", "L14-17"],
    ],
  },
  {
    path: "src/currency.js",
    lang: "javascript",
    imports: [],
    symbols: [["toCents", "L1-4"]],
  },
];
export const baseName = (path: string) => path.split("/").pop() ?? path;
export const importedBy = (path: string) =>
  demoFiles.filter((f) => f.imports.includes(path)).map((f) => f.path);
export const demoDuplicates = [
  ["formatPrice", "formatMoney"],
  ["parseArgs", "parseFlags"],
  ["roundTo", "toCents"],
] as const;
export const demoDeadCode = [
  ["parseFlags", "src/cli.js"],
  ["roundTo", "src/format.js"],
] as const;
export const fileOf = (symbol: string) =>
  demoFiles.find((f) => f.symbols.some(([n]) => n === symbol))?.path ??
  demoFiles[0].path;
