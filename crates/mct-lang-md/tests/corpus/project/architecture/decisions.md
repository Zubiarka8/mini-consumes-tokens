---
title: Architecture decision log
aliases: ["ADR log", Decisiones]
tags: [architecture, adr]
---

Architecture decision log
=========================

Every structural decision of the warehouse service, newest last. Each entry
follows the [[#Template]]. Context for the whole system is in
[[./overview]]; the people who decide are listed in
[[../handbook#Team members]]. #adr

Decision log
------------

| ADR | Title                               | Status     |
|-----|-------------------------------------|------------|
| 001 | Modular monolith                    | Accepted   |
| 002 | Transactional outbox                | Accepted   |
| 003 | Reserve before payment              | Accepted   |
| 004 | Expand and contract migrations      | Accepted   |
| 005 | Decimal money with explicit currency| Accepted   |
| 006 | One adapter per carrier             | Accepted   |
| 007 | Event sourcing for orders           | Rejected   |
| 008 | Canary releases                     | Accepted   |

### ADR-001 Modular monolith

**Status:** accepted, 2025-11-04.

#### Context

The team is five people. The previous system was eleven microservices owned
by three teams, and most incidents were integration failures between them.

#### Decision

One deployable, split into bounded contexts with enforced boundaries
([[./overview#Bounded contexts]]): no cross-schema joins, no calls into
another context's internals, communication through domain events or explicit
application services.

#### Consequences

- One pipeline, one runbook ([[../operations/runbook]]).
- Boundaries are enforced by an architecture test that fails the build when a
  context imports another's `internal` package.
- Extracting a context later remains possible because the boundaries are real.

### ADR-002 Transactional outbox

**Status:** accepted, 2025-11-18.

#### Context

Publishing an event after committing a transaction loses the event if the
process dies in between; publishing before committing announces changes that
may roll back.

#### Decision

Write events to an `outbox` table in the same transaction as the state
change. A single relay process polls the table and publishes to the broker,
marking rows as sent. Consumers deduplicate on the event id.

#### Consequences

- At-least-once delivery; consumers must be idempotent.
- The relay is a single replica ([[./overview#Deployment]]); its lag is an
  alert in [[../operations/runbook#Alerts]].
- Ordering is per aggregate, not global.

### ADR-003 Reserve before payment

**Status:** accepted, 2025-12-02.

#### Context

When payment came first, a confirmed payment sometimes met an empty shelf and
had to be refunded — 1.8 % of orders in peak season. #money

#### Decision

Reserve stock when the order is placed, before payment. Reservations expire
after 30 minutes without payment. The timeout is shorter than the payment
providers' own (25 minutes for card, 10 for Bizum) plus a safety margin.

#### Consequences

- Stock can be held by abandoned checkouts for up to 30 minutes.
- An expiry job releases reservations; see
  [[./overview#Reservations]] and the alert on its lag in
  [[../operations/runbook#Alerts]].
- Refunds for out-of-stock dropped to zero in the first month.

### ADR-004 Expand and contract migrations

**Status:** accepted, 2026-01-13.

#### Context

Rolling deployments run the old and the new version against the same
database for up to twenty minutes.

#### Decision

Every schema change is split into backwards-compatible steps:

1. **Expand** — add the new column/table, nullable or with a default.
2. **Migrate** — write both, backfill, read the new one.
3. **Contract** — stop writing the old one, then drop it in a later release.

A destructive step is never in the same release as the code that stops using
the old structure.

#### Consequences

- Renames take three releases. That is the price.
- The review checklist asks for it ([[../handbook#Code review]]).

### ADR-005 Decimal money with explicit currency

**Status:** accepted, 2026-02-03.

#### Context

A float rounding error produced invoices one cent off the order total. #money

#### Decision

All amounts are `Money(amount: BigDecimal, currency: Currency)` with scale 2
(0 for JPY) and banker's rounding, applied once per line. Arithmetic between
different currencies is a compile-time error; conversion goes through the
exchange-rate service with an explicit date. See [[../glossary#Money]].

```java
Money total = lines.stream()
    .map(OrderLine::subtotal)
    .reduce(Money.zero(EUR), Money::plus);
```

#### Consequences

- No `double` anywhere near prices; a lint rule enforces it.
- Rounding differences between line and total VAT are documented in
  [[../glossary#VAT]].

### ADR-006 One adapter per carrier

**Status:** accepted, 2026-03-10.

#### Context

Carriers differ in everything: authentication, label formats (PDF, ZPL),
cut-off times and webhook payloads.

#### Decision

A `CarrierClient` interface with one adapter per carrier, each in its own
package with its own tests against recorded responses. No shared "generic
carrier" code beyond the interface. Adding a carrier:

1. Implement `CarrierClient` (book, label, cancel, track).
2. Map its webhook to our tracking events ([[../api/reference#Carrier webhooks]]).
3. Add its cut-off times to the configuration.
4. Add a dashboard panel and an alert ([[../operations/runbook#Alerts]]).

#### Consequences

- Some duplication between adapters; accepted for isolation.
- A carrier outage only degrades its own adapter; orders fall back to the
  next carrier by priority.

### ADR-007 Event sourcing for orders

**Status:** rejected, 2026-04-21.

#### Context

The order history view needed every state change with its author and time.

#### Decision

Rejected. An append-only `order_history` table written in the same
transaction gives the history without the cost of rebuilding aggregates from
events. Revisit if we ever need temporal queries beyond the history view.

#### Consequences

- The `Order` aggregate stays a plain table.
- The outbox ([[#ADR-002 Transactional outbox]]) remains the only event store.

### ADR-008 Canary releases

**Status:** accepted, 2026-06-02.

#### Context

Two incidents in May were caused by changes that passed staging but failed
under production traffic patterns ([[../operations/runbook#Past incidents]]).

#### Decision

Every rollout sends 5 % of traffic to the new version for ten minutes and
compares error rate and p95 latency with the stable version. A regression
beyond the thresholds aborts the rollout automatically.

#### Consequences

- Deploys take ten minutes longer.
- The thresholds live next to the SLOs in
  [[../operations/runbook#Service level objectives]].

### ADR-009 Read replica for shop queries

**Status:** accepted, 2026-07-07.

#### Context

Stock and catalog queries from the shop are 92 % of our requests and compete
with reservations for primary connections.

#### Decision

Serve `GET /products` and `GET /stock` from a streaming read replica. Writes,
and any read inside a write transaction, stay on the primary. A routing data
source picks the target from the transaction's read-only flag.

#### Consequences

- The shop can see stock a few seconds stale; reservation is the source of
  truth and rejects what is no longer there.
- Replication lag is now an alert ([[../operations/runbook#Past incidents]]).

### ADR-010 Back-office desktop client

**Status:** accepted, 2026-08-25.

#### Context

Pickers and supervisors work on fixed terminals with scanners and label
printers; the web back office could not drive the printers reliably.

#### Decision

A desktop client for the back office that talks to the same HTTP API as
everyone else ([[../api/reference]]), with no private endpoints.

#### Consequences

- Printing and scanning work offline and sync when the connection returns.
- One more artefact to release; it follows the same versioning rules.

Proposals under discussion
--------------------------

### Split the shipping context into its own service

Shipping has the most external dependencies and the burstiest load. Moving it
out would isolate carrier outages further. Open questions:

- How do we keep picking and packing transactional with stock movements?
- Is the operational cost worth it for a team of five?

Owner: Mikel. Discussed at the architecture forum ([[../handbook#Rituals]]).
#proposal

### Replace nightly ERP import with change data capture

The catalog import is nightly ([[./overview#Known limitations]]). CDC from the
ERP's database would make price changes visible within minutes. Blocked on the
ERP vendor's licence terms. #proposal

Template
--------

Copy this block for a new decision:

```markdown
### ADR-NNN Short title

**Status:** proposed, YYYY-MM-DD.

#### Context

What forces are at play? Link the relevant [[note#Heading]].

#### Decision

What we will do, in the active voice.

#### Consequences

What becomes easier, what becomes harder.
```

The fenced block above is an example, so the `[[note#Heading]]` inside it is
not a link.

Superseded decisions
--------------------

None yet. When a decision is replaced, keep its entry, set its status to
*superseded by ADR-NNN* and link the new entry, e.g.
[[#ADR-008 Canary releases|ADR-008]].
