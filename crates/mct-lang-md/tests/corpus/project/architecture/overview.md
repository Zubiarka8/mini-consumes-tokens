---
title: Architecture overview
aliases:
  - Architecture
  - Arquitectura
tags: [architecture, team/warehouse]
---

# Architecture overview

How the warehouse service is put together. Decisions behind each choice are
in [[./decisions]]; operational concerns in [[../operations/runbook]]; the
public contract in [[../api/reference]]. Words with a precise meaning are
defined in the [[../glossary]]. #architecture

## Context

The warehouse sits between three neighbours:

- The **shop** (e-commerce team) places orders and reads stock through the
  HTTP API ([[../api/reference#Orders]], [[../api/reference#Stock]]).
- The **carriers** book shipments, print labels and report tracking events
  back through webhooks ([[../api/reference#Carrier webhooks]]).
- The **ERP** owns suppliers, purchase prices and accounting; it receives our
  invoices nightly and sends advance shipping notices.

![[context-diagram.svg]]

## Bounded contexts

The code is one deployable split into contexts that talk through domain
events, never through each other's tables. #ddd

### Catalog

Products, categories, attributes and price lists. Imported nightly from the
ERP's XML export; edited in the back office for anything the ERP does not
know (photos, descriptions, cross-sell). Owns the *product* and *category*
terms of the [[../glossary#Product]] entry.

### Inventory

Stock levels per product and location, reservations and movements. Publishes
`StockReserved`, `StockReleased` and `ReorderPointReached`. The reservation
rules are in [[#Reservations]] below.

### Orders

The order aggregate and its state machine ([[#Order lifecycle]]). Talks to
Inventory to reserve, to Pricing to quote and to Billing to invoice.

### Pricing

Turns a basket into a quote: base prices, price lists, discounts and VAT. Pure
functions over the catalog plus exchange rates. See [[../glossary#VAT]].

### Shipping

Picking, packing, carrier booking and tracking. One adapter per carrier
([[./decisions#ADR-006 One adapter per carrier]]).

### Billing

Invoices, credit notes and the daily batch to the ERP. The only context
allowed to assign invoice numbers ([[../glossary#Invoice number]]).

## Order lifecycle

```text
NEW ──reserve──▶ RESERVED ──pay──▶ PAID ──pick──▶ PICKED ──ship──▶ SHIPPED ──invoice──▶ INVOICED
 │                  │                │               │
 └──────────────────┴────cancel──────┴───────────────┴──▶ CANCELLED
```

Transitions are commands on the `Order` aggregate; each emits an event of the
same name. The only path back is cancellation, and only before `SHIPPED`.

### Approval of large orders

Orders above the threshold of [[../glossary#Large order]] stop in `NEW` until a
supervisor approves them. Approval is a user task of the BPMN process; a
rejection cancels the order and releases nothing (nothing was reserved yet).

### Reservations

A reservation is created per order line, atomically per location:

1. Pick the location with the most available stock that can serve the whole
   line; split the line only if no single location can.
2. Increment `reserved` with a conditional update (`on_hand - reserved >= qty`).
3. Emit `StockReserved` with the reservation id.

A reservation expires 30 minutes after creation unless the order is paid
([[./decisions#ADR-003 Reserve before payment]]). Expiry is a scheduled job,
not a database trigger.

#### Concurrency

Two orders racing for the last unit both run the conditional update; exactly
one matches. The loser gets `OutOfStock` and the order goes to manual review.
We never lock rows for longer than one statement. #performance

### Payment

Payment is confirmed by the provider's webhook, not by the browser redirect.
The webhook is idempotent on the provider's event id; see
[[../api/reference#Payment webhook]].

### Picking and packing

Picking is split by zone (ambient, chilled, bulky) and runs in parallel; the
order is packed when every zone is done. Chilled items are picked last.

### Shipping and invoicing

Booking a carrier returns a tracking number and a label. The invoice is
issued when the carrier confirms the pickup, never before: an order that never
leaves the building is not invoiced.

## Replenishment

When available stock drops below a product's reorder point, Inventory emits
`ReorderPointReached`. The replenishment process drafts a purchase order for
the reorder quantity, a supervisor approves it, and the goods are put away
when the ERP's advance shipping notice arrives and the delivery is scanned.

Reorder points are recomputed weekly from demand:

```text
reorder_point = average_daily_demand × lead_time_days + safety_stock
safety_stock  = z(service_level) × σ(daily_demand) × √lead_time_days
```

With a 97 % service level, `z ≈ 1.88`. The defaults per category are in the
configuration ([[#Configuration]]).

## Data

### Storage

One PostgreSQL database with a schema per context. Contexts never join across
schemas; a read model that needs data from two contexts is built from events.

| Schema      | Main tables                                   |
|-------------|-----------------------------------------------|
| `catalog`   | `products`, `categories`, `price_lists`       |
| `inventory` | `stock_levels`, `reservations`, `movements`   |
| `orders`    | `orders`, `order_lines`, `approvals`          |
| `shipping`  | `shipments`, `parcels`, `tracking_events`     |
| `billing`   | `invoices`, `invoice_lines`, `credit_notes`   |

A read replica serves the catalog and stock queries of the shop; writes always
go to the primary. Invoices older than seven years move to the archive
database.

### Events

Events are stored in an outbox table in the same transaction as the state
change and relayed to the message broker by a poller. Consumers are
idempotent on the event id. The rationale is in
[[./decisions#ADR-002 Transactional outbox]].

```json
{
  "id": "01J9Z4K8Q2W7",
  "type": "StockReserved",
  "occurredAt": "2026-10-01T09:12:44Z",
  "payload": { "orderId": "PED-2026-004812", "sku": "sku-1001", "qty": 2 }
}
```

### Migrations

Schema changes follow expand-and-contract so that two versions of the service
can run against the same schema during a rollout
([[./decisions#ADR-004 Expand and contract migrations]]).

## Configuration

Configuration comes from, in order of precedence: environment variables,
`/etc/warehouse/override.properties`, `warehouse.properties` on the classpath.

| Key                          | Default  | Meaning                             |
|------------------------------|----------|-------------------------------------|
| `db.pool.max`                | `20`     | Primary pool size                   |
| `warehouse.worker-id`        | `1`      | Id generator worker (1–31)          |
| `reservation.ttl`            | `PT30M`  | Reservation expiry                  |
| `orders.large-threshold`     | `5000`   | EUR above which approval is needed  |
| `replenishment.service-level`| `0.97`   | Target service level                |

Secrets (database passwords, carrier API keys) come from the secret store and
are never written to these files. #security

## Cross-cutting concerns

### Security

- Customers authenticate in the shop; the shop calls us with a service token
  scoped per operation ([[../api/reference#Authentication]]).
- Back-office users log in with SSO; roles map to the lanes of the order
  process (picker, shipper, supervisor, accountant, admin).
- Personal data is limited to what shipping and invoicing need, and is
  redacted from logs ([[../operations/runbook#Logs and traces]]). #security

### Observability

Every request carries a request id and a tenant; both go into the logs and the
traces. The service level objectives and their alerts are owned by the
[[../operations/runbook#Service level objectives|runbook]].

### Performance budgets

| Operation             | p95 budget |
|-----------------------|------------|
| Stock query           | 50 ms      |
| Reserve order         | 200 ms     |
| Quote                 | 100 ms     |
| Book carrier          | 2 s        |

Anything over budget for a week becomes a ticket with the `performance` label.
#performance

## Deployment

One container image, three process types started with different arguments:

- `web` — the HTTP API, scaled on CPU;
- `worker` — event consumers and scheduled jobs, scaled on queue depth;
- `relay` — the outbox poller, exactly one replica.

Rollouts are canaried ([[../handbook#Releases]]). Rollbacks are described in
[[../operations/runbook#Rollback]].

## Failure modes

What happens when each dependency fails, and what the service does about it.

| Dependency        | Failure                  | Behaviour                                   |
|-------------------|--------------------------|---------------------------------------------|
| Primary database  | Unreachable              | Writes fail fast; `ready` goes red          |
| Read replica      | Lagging                  | Stock may be stale; reservations still safe |
| Broker            | Unreachable              | Outbox grows; nothing is lost               |
| Payment provider  | Down                     | Orders stay `RESERVED`, then expire         |
| A carrier         | Down                     | Fallback to the next carrier                |
| ERP               | Export missing           | Yesterday's catalog stays in place          |

The runbook has a section per symptom ([[../operations/runbook#Common symptoms]]).

## Testing strategy

- **Unit tests** for the domain: aggregates, pricing rules, reorder points.
  Fast, no I/O, run on every save.
- **Integration tests** per context against a real PostgreSQL in a container,
  including the migrations.
- **Contract tests** for the HTTP API, generated from the examples of
  [[../api/reference]]; the shop runs the same contracts on its side.
- **Carrier adapter tests** against recorded responses, re-recorded monthly.
- **End-to-end smoke tests** on staging and on the canary: place, pay, ship
  and invoice one order per carrier.

A test that needs the network outside the container is a bug in the test.
#testing

## Known limitations

- Split reservations across locations are rare but make picking slower; we do
  not optimise the split.
- The ERP export is nightly, so a price change in the ERP reaches the shop the
  next morning.
- Carrier webhooks are not ordered; tracking events are re-sorted by their own
  timestamp, not by arrival.

## Ownership

Every context has an owner who reviews structural changes to it and keeps its
section of this note current. Owners are listed in
[[../handbook#Team members]].

| Context    | Owner     | Main risks                                  |
|------------|-----------|---------------------------------------------|
| Catalog    | Ana Belén | ERP export format changes                   |
| Inventory  | Ana Belén | Reservation correctness under concurrency   |
| Orders     | Jon       | State machine regressions                   |
| Pricing    | Jon       | Rounding, VAT rules                         |
| Shipping   | Mikel     | Carrier API changes, label printing         |
| Billing    | Itziar    | Gapless numbering, ERP batch                |

An owner is not a gatekeeper: anyone can change any context, but the owner is
asked to review. When an owner leaves, the tech lead reassigns the context in
the same week.

Ownership also covers the context's alerts: the owner reviews the alert's
runbook section ([[../operations/runbook#Alerts]]) after every incident in
which it fired. #ownership

## Further reading

- [[./decisions]] — why things are the way they are.
- [[../operations/runbook]] — what to do when they break.
- [[../api/reference]] — what the outside world sees.
- [[../glossary]] — what the words mean.
- [[../../outside-the-vault]] — a link that escapes the vault on purpose.
