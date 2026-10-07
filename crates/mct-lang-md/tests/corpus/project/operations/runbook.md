---
title: Operations runbook
aliases: [Runbook, On-call guide]
tags: [operations, on-call]
---

# Operations runbook

What to do when the warehouse service misbehaves. Start with
[[#Triage]], roll back early ([[#Rollback]]) and write a review afterwards
([[#Post-incident review]]). The system itself is described in
[[../architecture/overview]]. #on-call

## On-call rota

| Week of    | Primary   | Secondary |
|------------|-----------|-----------|
| 2026-09-28 | Mikel     | Jon       |
| 2026-10-05 | Ana Belén | Mikel     |
| 2026-10-12 | Jon       | Itziar    |
| 2026-10-19 | Itziar    | Ana Belén |

The rota is embedded in the [[../handbook]]. Swaps are fine; update the table
and tell `#warehouse`.

### Handover

Every Monday at 10:00 the outgoing primary hands over to the incoming one:

1. Open incidents and their current state.
2. Alerts that fired more than twice, and whether they were actionable.
3. Deploys in flight or frozen.
4. Anything weird that did not become an incident.

Write the handover in the on-call channel so the secondary sees it too.

### Escalation

1. Primary on-call (paged by the alert).
2. Secondary, after 10 minutes without acknowledgement.
3. Tech lead, after 20 minutes or for any SEV-1.
4. For carrier outages, the carrier's support line from
   [[../api/reference#Carrier contacts]].

## Severity levels

| Level | Definition                                           | Response     |
|-------|------------------------------------------------------|--------------|
| SEV-1 | Orders cannot be placed or paid                      | Immediately  |
| SEV-2 | Orders placed but not shipped; stock wrong           | 30 minutes   |
| SEV-3 | Degraded but working (slow, one carrier down)        | Working hours|
| SEV-4 | Cosmetic, back office only                           | Next cycle   |

## Triage

When an alert fires:

1. **Acknowledge** it so the secondary is not paged.
2. **Check recent deploys.** If something shipped in the last hour, roll it
   back ([[#Rollback]]) before anything else.
3. **Check the dashboards** ([[#Dashboards]]): error rate, latency, queue
   depth, database connections.
4. **Check dependencies**: payment provider status page, carriers, ERP.
5. **Declare** an incident in `#warehouse-incidents` if users are affected.

> [!warning]
> Do not debug in production with a deploy pending. Roll back, then debug.

### Common symptoms

#### Orders stuck in RESERVED

Payments are not being confirmed. Check the payment webhook endpoint
([[../api/reference#Payment webhook]]) for errors and the provider's status
page. If the provider is down, nothing to do but wait; reservations expire on
their own after 30 minutes
([[../architecture/decisions#ADR-003 Reserve before payment]]).

#### Stock going negative

Should be impossible ([[../architecture/overview#Concurrency]]). If it
happens, freeze reservations with the `reservations.frozen` flag and page the
tech lead: it is a SEV-2 and a data repair. #data-repair

#### Outbox lag growing

The relay is down or slow. Check that exactly one `relay` replica is running
([[../architecture/overview#Deployment]]). Two relays publish duplicates
(harmless, consumers are idempotent); zero relays publish nothing.

```bash
kubectl -n warehouse get pods -l app=warehouse,process=relay
kubectl -n warehouse logs deploy/warehouse-relay --since=15m | tail -50
```

#### One carrier failing

Orders fall back to the next carrier automatically
([[../architecture/decisions#ADR-006 One adapter per carrier]]). Disable the
failing carrier with its flag if the fallback is slow, and call its support
line.

#### Database connections exhausted

Usually a slow query holding connections. Find it:

```sql
SELECT pid, now() - query_start AS running, state, left(query, 120)
  FROM pg_stat_activity
 WHERE datname = 'warehouse' AND state <> 'idle'
 ORDER BY running DESC
 LIMIT 20;
```

Cancel it with `pg_cancel_backend(pid)` if it is a report; investigate if it
is a reservation.

## Rollback

Rolling back is always safe because migrations are expand-and-contract
([[../architecture/decisions#ADR-004 Expand and contract migrations]]).

```bash
# list the last releases
./ops/release list --last 5
# roll back to the previous one
./ops/release rollback --to previous --reason "SEV-2 checkout errors"
```

The rollback takes about four minutes and goes through the same canary as a
release, with the thresholds relaxed. If the previous release is also bad, roll
back again; never roll forward under pressure.

### Rolling back a migration

Don't. Ship a new expand step instead. The only exception is a migration that
has not been contracted yet, which the old code can run against anyway.

## Service level objectives

| SLO                         | Target   | Window  | Alert at           |
|-----------------------------|----------|---------|--------------------|
| Order placement success     | 99.9 %   | 28 days | 2 % budget in 1 h  |
| Stock query latency (p95)   | 50 ms    | 28 days | 10 % budget in 6 h |
| Reservation latency (p95)   | 200 ms   | 28 days | 10 % budget in 6 h |
| Order shipped within 24 h   | 98 %     | 7 days  | 5 % budget in 24 h |

Canary thresholds ([[../architecture/decisions#ADR-008 Canary releases]]) are
derived from these: error rate at most 1.5× stable, p95 at most 1.2× stable.

## Alerts

Every alert has a runbook section; if it does not, the alert is a bug.

### HighOrderErrorRate

More than 1 % of order placements failing for five minutes. Follow
[[#Triage]]. Almost always a recent deploy or the payment provider.

### ReservationExpiryLag

The expiry job is more than five minutes behind. Stock is being held by
abandoned checkouts. Restart the `worker` deployment; if it keeps lagging,
scale it.

### OutboxRelayLag

Oldest unsent outbox row older than two minutes. See
[[#Outbox lag growing]].

### CarrierBookingFailures

More than 20 % of bookings with one carrier failing for ten minutes. See
[[#One carrier failing]].

### ReplicaLag

The read replica more than 30 seconds behind the primary for five minutes.
The shop shows stale stock; reservations are unaffected
([[../architecture/decisions#ADR-009 Read replica for shop queries]]). Check
for long-running queries on the replica and for a vacuum in progress. If lag
passes ten minutes, route shop reads to the primary with the
`stock.read-from-primary` flag.

### DiskSpaceLow

The database volume above 85 %. Usually the archive job stopped; check that
`invoiceArchiver` ran last night.

## Dashboards

- **Overview** — request rate, error rate, p95 per endpoint, saturation.
- **Orders** — orders per state, approvals waiting, reservations expiring.
- **Inventory** — stock queries, reservation conflicts, reorder events.
- **Shipping** — bookings per carrier, label print errors, webhook delays.
- **Database** — connections, slow queries, replication lag.

## Logs and traces

Logs are JSON with `requestId`, `tenant` and `orderNumber` in every line.
Personal data (names, addresses, emails) is redacted by the logging
configuration; if you see any in the logs, that is a SEV-3 security bug.
#security

Useful queries:

```text
service:warehouse AND level:ERROR AND orderNumber:"PED-2026-004812"
service:warehouse AND logger:*SlowQueryMonitor
service:warehouse AND message:"OutOfStock" | stats count by sku
```

Traces follow a request from the shop through the service and into the
database and carriers. The trace id equals the request id.

## Data repair

Repairs are scripts reviewed like code, run once, and kept in
`ops/repairs/` with the incident they fix. Never edit production data by hand
in a SQL console. #data-repair

### Recomputing reserved stock

```sql
UPDATE inventory.stock_levels s
   SET reserved = coalesce(r.total, 0)
  FROM (SELECT product_id, location_id, sum(quantity) AS total
          FROM inventory.reservations
         WHERE released_at IS NULL
         GROUP BY product_id, location_id) r
 WHERE r.product_id = s.product_id AND r.location_id = s.location_id;
```

### Re-sending an invoice to the ERP

Mark it for the next batch: `./ops/billing resend --invoice FAC-2026-000123`.
The batch runs at 23:55 ([[../api/reference#Invoices]]).

## Planned maintenance

- **Database minor upgrades** — Tuesdays 06:00–07:00, failover to the standby
  first; expect one minute of write errors.
- **Carrier certificate rotations** — announced by the carriers; update the
  secret and restart the `worker` deployment.
- **Annual stock count** — the first Sunday of January. Reservations are
  frozen with `reservations.frozen` for the duration; the shop shows
  "available Monday".

Announce maintenance in `#warehouse` and to the e-commerce team at least two
working days ahead.

## Peak season

From Black Friday to the 6th of January:

1. Deploy freeze except for fixes, from the Wednesday before Black Friday.
2. Double the `web` and `worker` minimum replicas.
3. A second person shadows on-call during evening peaks (19:00–23:00).
4. Carrier cut-off times move one hour earlier; update the configuration.
5. Daily check of the [[#Service level objectives]] with the product owner.

Last year's peak numbers are kept in the Orders dashboard
([[#Dashboards]]) for comparison. #peak

## Post-incident review

Within five working days of a SEV-1 or SEV-2. Blameless: we look at the
system, not at people. The template:

1. **Summary** — two sentences.
2. **Impact** — orders affected, revenue, duration.
3. **Timeline** — UTC, from first signal to resolution.
4. **Root causes** — usually more than one.
5. **What went well / what went badly.**
6. **Actions** — tickets with owners; they jump the queue
   ([[../handbook#Planning]]).

## Past incidents

### 2026-05-14 Checkout errors after a pricing change

A new discount rule threw on baskets with a single bulky item. 42 minutes,
about 300 failed checkouts. Rolled back. Led to
[[../architecture/decisions#ADR-008 Canary releases]].

### 2026-05-27 Duplicate shipments

Two relay replicas after a misconfigured scale-out published every event twice;
one carrier adapter was not idempotent. 18 duplicate bookings, cancelled by
hand. Fixed the adapter and pinned the relay to one replica.

### 2026-08-03 Stock showing zero in the shop

The read replica fell 40 minutes behind during a vacuum. The shop showed
products as sold out. Added replication lag to the [[#Dashboards]] and an alert.

### 2026-09-09 Invoices with one-cent differences

A rounding change applied VAT per unit instead of per line. Fixed and
documented in [[../glossary#VAT]]. #money
