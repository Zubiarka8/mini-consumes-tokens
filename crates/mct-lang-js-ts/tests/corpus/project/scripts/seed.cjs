#!/usr/bin/env node
"use strict";

/**
 * Seeds a development warehouse through the HTTP API: customers, stock and a
 * handful of orders in every state. Plain CommonJS so it runs with any Node
 * without a build step; the fixtures below mirror the demo catalog of the
 * TypeScript code and the API paths documented in docs/api/reference.md.
 *
 *   node scripts/seed.cjs --base-url http://localhost:8080/api/v1 --orders 20
 */

const fs = require("node:fs");
const path = require("node:path");
const { setTimeout: delay } = require("node:timers/promises");
const { randomUUID } = require("node:crypto");
const { version } = require("../package.json");

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const fixtures = {
  stock: [
    { sku: "sku-1001", location: "A-03-2", quantity: 40 },
    { sku: "sku-1002", location: "A-03-4", quantity: 12 },
    { sku: "sku-1003", location: "A-03-5", quantity: 200 },
    { sku: "sku-1004", location: "A-03-5", quantity: 200 },
    { sku: "sku-1101", location: "A-07-1", quantity: 9 },
    { sku: "sku-1201", location: "B-01-1", quantity: 30 },
    { sku: "sku-2001", location: "C-02-1", quantity: 960 },
    { sku: "sku-2101", location: "F-01-3", quantity: 180 },
    { sku: "sku-2103", location: "F-02-1", quantity: 600 },
    { sku: "sku-3001", location: "Z-BULK-1", quantity: 4 },
    { sku: "sku-3002", location: "Z-BULK-2", quantity: 10 },
    { sku: "sku-3101", location: "D-04-1", quantity: 7 },
  ],
  customers: [
    {
      id: "cus_8812",
      name: "Itziar Etxeberria",
      email: "itziar@example.com",
      address: { street: "Calle Iparraguirre 12", postalCode: "48009", city: "Bilbao", country: "ES" },
      tier: "gold",
    },
    {
      id: "cus_8813",
      name: "Jon Agirre",
      email: "jon@example.com",
      address: { street: "Avenida Libertad 3", postalCode: "20004", city: "Donostia", country: "ES" },
      tier: "silver",
    },
    {
      id: "cus_8814",
      name: "Ana Belén Ruiz",
      email: "anabelen@example.com",
      address: { street: "Rua Augusta 100", postalCode: "1100-053", city: "Lisboa", country: "PT" },
      tier: null,
    },
    {
      id: "cus_8815",
      name: "Müller GmbH",
      email: "einkauf@mueller.example",
      address: { street: "Hauptstraße 5", postalCode: "10115", city: "Berlin", country: "DE" },
      tier: "b2b",
    },
  ],
};

const DEFAULTS = {
  baseUrl: "http://localhost:8080/api/v1",
  orders: 12,
  token: process.env.WAREHOUSE_TOKEN || "dev-token",
  dryRun: false,
  verbose: false,
};

// ---------------------------------------------------------------------------
// Arguments
// ---------------------------------------------------------------------------

function parseArgs(argv) {
  const options = { ...DEFAULTS };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    switch (arg) {
      case "--base-url":
        options.baseUrl = argv[++i];
        break;
      case "--orders":
        options.orders = Number.parseInt(argv[++i], 10);
        break;
      case "--token":
        options.token = argv[++i];
        break;
      case "--dry-run":
        options.dryRun = true;
        break;
      case "-v":
      case "--verbose":
        options.verbose = true;
        break;
      case "-h":
      case "--help":
        printHelp();
        process.exit(0);
        break;
      default:
        throw new Error(`unknown argument: ${arg}`);
    }
  }
  if (!Number.isInteger(options.orders) || options.orders < 0) {
    throw new Error("--orders must be a non-negative integer");
  }
  return options;
}

function printHelp() {
  console.log(
    [
      "usage: node scripts/seed.cjs [options]",
      "  --base-url URL   API base URL (default: " + DEFAULTS.baseUrl + ")",
      "  --orders N       number of orders to create (default: " + DEFAULTS.orders + ")",
      "  --token T        bearer token (default: $WAREHOUSE_TOKEN)",
      "  --dry-run        print the requests instead of sending them",
      "  -v, --verbose    log every request",
    ].join("\n"),
  );
}

// ---------------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------------

function createApi(options) {
  let sent = 0;

  async function request(method, urlPath, body) {
    sent++;
    if (options.verbose || options.dryRun) {
      console.log(`${method} ${urlPath}${body ? " " + JSON.stringify(body) : ""}`);
    }
    if (options.dryRun) {
      return { dryRun: true };
    }
    const response = await fetch(options.baseUrl + urlPath, {
      method,
      headers: {
        Authorization: `Bearer ${options.token}`,
        "Content-Type": "application/json",
        "Idempotency-Key": randomUUID(),
      },
      body: body ? JSON.stringify(body) : undefined,
    });
    if (response.status === 429) {
      const retryAfter = Number(response.headers.get("Retry-After") || "1");
      await delay(retryAfter * 1000);
      return request(method, urlPath, body);
    }
    if (!response.ok) {
      const text = await response.text();
      throw new Error(`${method} ${urlPath} → ${response.status}: ${text}`);
    }
    return response.status === 204 ? null : response.json();
  }

  return {
    get: (urlPath) => request("GET", urlPath),
    post: (urlPath, body) => request("POST", urlPath, body),
    get sent() {
      return sent;
    },
  };
}

// ---------------------------------------------------------------------------
// Seeding steps
// ---------------------------------------------------------------------------

async function seedStock(api) {
  for (const item of fixtures.stock) {
    await api.post(`/stock/${encodeURIComponent(item.sku)}/receipts`, {
      location: item.location,
      quantity: item.quantity,
    });
  }
  return fixtures.stock.length;
}

async function seedCustomers(api) {
  const created = [];
  for (const customer of fixtures.customers) {
    const result = await api.post("/customers", customer);
    created.push(result && result.id ? result.id : customer.id);
  }
  return created;
}

function randomBasket(rng) {
  const skus = fixtures.stock.map((s) => s.sku);
  const lines = [];
  const count = 1 + Math.floor(rng() * 3);
  while (lines.length < count) {
    const sku = skus[Math.floor(rng() * skus.length)];
    if (!lines.some((l) => l.sku === sku)) {
      lines.push({ sku, quantity: 1 + Math.floor(rng() * 4) });
    }
  }
  return lines;
}

/** A small deterministic PRNG so every run seeds the same data. */
function mulberry32(seed) {
  let a = seed >>> 0;
  return function next() {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const TARGET_STATES = ["RESERVED", "PAID", "SHIPPED", "CANCELLED"];

async function advance(api, order, target) {
  const number = encodeURIComponent(order.number);
  switch (target) {
    case "CANCELLED":
      await api.post(`/orders/${number}/cancel`, { reason: "seed" });
      return;
    case "SHIPPED":
      await api.post(`/dev/orders/${number}/pay`, {});
      await api.post(`/dev/orders/${number}/ship`, { carrier: "seur" });
      return;
    case "PAID":
      await api.post(`/dev/orders/${number}/pay`, {});
      return;
    default:
      return;
  }
}

async function seedOrders(api, customers, count) {
  const rng = mulberry32(74);
  const byState = Object.fromEntries(TARGET_STATES.map((s) => [s, 0]));
  for (let i = 0; i < count; i++) {
    const customerId = customers[i % customers.length];
    const order = await api.post("/orders", { customerId, lines: randomBasket(rng) });
    const target = TARGET_STATES[i % TARGET_STATES.length];
    if (order && !order.dryRun) {
      await advance(api, order, target);
    }
    byState[target]++;
  }
  return byState;
}

function writeReport(file, report) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, JSON.stringify(report, null, 2) + "\n");
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main(argv = process.argv.slice(2)) {
  const options = parseArgs(argv);
  const api = createApi(options);
  const started = Date.now();
  const stock = await seedStock(api);
  const customers = await seedCustomers(api);
  const orders = await seedOrders(api, customers, options.orders);
  const report = {
    version,
    baseUrl: options.baseUrl,
    stock,
    customers: customers.length,
    orders,
    requests: api.sent,
    seconds: (Date.now() - started) / 1000,
  };
  writeReport(path.join(__dirname, "..", "target", "seed-report.json"), report);
  console.log(`seeded ${stock} stock rows, ${customers.length} customers, ${options.orders} orders`);
  return report;
}

module.exports = { parseArgs, createApi, mulberry32, randomBasket, seedOrders, main };
module.exports.DEFAULTS = DEFAULTS;
exports.TARGET_STATES = TARGET_STATES;

if (require.main === module) {
  process.on("unhandledRejection", (reason) => {
    console.error("unhandled rejection:", reason);
    process.exitCode = 2;
  });
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
