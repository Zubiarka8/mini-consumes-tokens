---
title: HTTP API reference
aliases: [API, API reference]
tags: [api, "team/warehouse"]
---

# HTTP API reference

The warehouse service's public contract. Consumers are the shop (e-commerce
team), the back office and the carriers' webhooks. Concepts are defined in the
[[../glossary]]; how the service works inside is in
[[../architecture/overview]]. #api

## Conventions

- Base URL: `https://warehouse.internal.example.com/api/v1`.
- JSON bodies, `snake_case` fields, UTF-8.
- Amounts are objects `{ "amount": "24.90", "currency": "EUR" }`, never
  numbers ([[../architecture/decisions#ADR-005 Decimal money with explicit currency]]).
- Timestamps are RFC 3339 in UTC.
- Every response carries `X-Request-Id`; quote it when reporting a problem
  ([[../operations/runbook#Logs and traces]]).

### Authentication

Service-to-service calls use OAuth 2.0 client credentials. Tokens carry
scopes per operation:

| Scope            | Grants                                |
|------------------|---------------------------------------|
| `catalog:read`   | Product and category queries          |
| `catalog:write`  | Product edits from the back office    |
| `orders:read`    | Order queries                         |
| `orders:write`   | Placing, cancelling and editing orders|
| `stock:read`     | Stock queries                         |

```http
GET /api/v1/products/sku-1001 HTTP/1.1
Host: warehouse.internal.example.com
Authorization: Bearer eyJhbGciOiJSUzI1NiIs...
```

### Errors

Errors use RFC 9457 problem details:

```json
{
  "type": "https://warehouse.example.com/problems/out-of-stock",
  "title": "Out of stock",
  "status": 409,
  "detail": "sku-1001 has 1 available, 2 requested",
  "instance": "/api/v1/orders/PED-2026-004812"
}
```

| Status | When                                                       |
|--------|------------------------------------------------------------|
| 400    | Malformed request                                          |
| 401    | Missing or invalid token                                   |
| 403    | Token lacks the scope                                      |
| 404    | Unknown resource                                           |
| 409    | Business rule violation (out of stock, invalid transition) |
| 422    | Validation failed; `errors` lists the fields               |
| 429    | Rate limited; honour `Retry-After`                         |

### Pagination

List endpoints take `limit` (default 50, max 200) and `cursor`; the response
carries `next_cursor` until the last page.

### Idempotency

`POST` endpoints accept an `Idempotency-Key` header. Repeating a request with
the same key within 24 hours returns the first response.

## Products

### List products

`GET /products?category=cat-cookware&in_stock=true`

Returns product summaries. Filters: `category`, `q` (full text), `in_stock`,
`on_sale`, `updated_since`.

### Get a product

`GET /products/{sku}`

```json
{
  "sku": "sku-1001",
  "name": "Sartén antiadherente 28 cm",
  "category": "cat-cookware",
  "price": { "amount": "24.90", "currency": "EUR" },
  "was_price": { "amount": "29.90", "currency": "EUR" },
  "attributes": { "weight": 1150, "colour": "black", "storage": "ambient" }
}
```

### Update a product

`PATCH /products/{sku}` — back office only (`catalog:write`). Fields owned by
the ERP (price, supplier) are read-only here; see
[[../architecture/overview#Catalog]].

## Stock

### Query stock

`GET /stock?sku=sku-1001&sku=sku-2001`

Returns `on_hand`, `reserved` and `available` per SKU, summed over locations.
Served from the read replica; may lag a few seconds behind writes.

### Stock by location

`GET /stock/{sku}/locations` — back office only.

## Orders

### Place an order

`POST /orders`

```json
{
  "customer_id": "cus_8812",
  "lines": [
    { "sku": "sku-1001", "quantity": 2 },
    { "sku": "sku-2001", "quantity": 6 }
  ],
  "shipping": { "method": "standard", "address": { "postal_code": "48009", "country": "ES" } }
}
```

Reserves stock immediately ([[../architecture/overview#Reservations]]) and
returns `201` with the order in `RESERVED`, or `409` if any line is out of
stock. Orders above the large-order threshold return `202` in `NEW` pending
approval ([[../glossary#Large order]]).

### Get an order

`GET /orders/{number}` — the order with its lines, totals, state and history.

### Cancel an order

`POST /orders/{number}/cancel` — allowed before `SHIPPED`; releases the
reservations. Returns `409` for a shipped order.

### Approve a large order

`POST /orders/{number}/approve` — supervisors only; body
`{ "approved": true, "comment": "…" }`.

### Order events

`GET /orders/{number}/events` — the order's history, oldest first. Each entry
has `type`, `at`, `actor` and a type-specific `data`.

## Shipments

### Book a shipment

Done internally after picking; not part of the public API. Listed here because
the carriers call us back ([[#Carrier webhooks]]).

### Get tracking

`GET /orders/{number}/tracking` — carrier, tracking number and events sorted
by the carrier's timestamp.

## Invoices

### Get an invoice

`GET /invoices/{number}` (JSON) or `GET /invoices/{number}.pdf`.

Invoice numbers follow [[../glossary#Invoice number]]. Invoices are sent to the
ERP in a daily batch at 23:55; re-sending is described in
[[../operations/runbook#Re-sending an invoice to the ERP]].

### Credit notes

`POST /invoices/{number}/credit-notes` — accountants only. A credit note
references the invoice and the lines it refunds.

## Webhooks

### Payment webhook

`POST /webhooks/payments` — called by the payment provider. Signed with
HMAC-SHA256 in `X-Signature`; reject anything that does not verify.
Idempotent on the provider's `event_id`.

| Event               | Effect                                  |
|---------------------|-----------------------------------------|
| `payment.succeeded` | Order moves to `PAID`                   |
| `payment.failed`    | Order stays `RESERVED` until expiry     |
| `refund.succeeded`  | Credit note issued                      |

### Carrier webhooks

Each carrier posts tracking events to its own path; the adapter maps them to
our event types ([[../architecture/decisions#ADR-006 One adapter per carrier]]).

| Carrier | Path                       | Auth                | Label format |
|---------|----------------------------|---------------------|--------------|
| Correos | `/webhooks/carriers/correos` | Basic auth        | PDF          |
| SEUR    | `/webhooks/carriers/seur`    | HMAC header       | ZPL          |
| DHL     | `/webhooks/carriers/dhl`     | Shared secret     | PDF          |
| UPS     | `/webhooks/carriers/ups`     | OAuth bearer      | ZPL          |

Our tracking event types:

- `picked_up` — the carrier has the parcel; triggers invoicing.
- `in_transit`, `out_for_delivery`.
- `delivered` — final.
- `exception` — address problem, damage, refused; opens a back-office task.

Events arrive out of order; they are sorted by the carrier's timestamp
([[../architecture/overview#Known limitations]]).

### Carrier contacts

| Carrier | Support                 | Hours           |
|---------|-------------------------|-----------------|
| Correos | +34 900 000 001         | 08:00–20:00     |
| SEUR    | +34 900 000 002         | 24/7            |
| DHL     | +34 900 000 003         | 24/7            |
| UPS     | +34 900 000 004         | 08:00–22:00     |

Used during carrier incidents ([[../operations/runbook#Escalation]]).

## Health

Unauthenticated, for load balancers and the platform:

- `GET /health/live` — the process is up. Never checks dependencies.
- `GET /health/ready` — the process can serve traffic: database reachable,
  migrations applied, outbox relay heartbeat recent.
- `GET /health/info` — version, commit and start time.

`ready` failing for more than a minute pages on-call through the
`HighOrderErrorRate` path ([[../operations/runbook#Alerts]]).

## Rate limits

Per client id: 600 requests per minute for reads, 120 for writes. Partners may
have higher limits by agreement. A `429` carries `Retry-After` in seconds.
#api

## Versioning

The version is in the path (`/api/v1`). Additive changes (new fields, new
endpoints) ship without a new version; consumers must ignore unknown fields.
Breaking changes need a new version and six months of overlap.

### Deprecations

Deprecated endpoints answer with `Deprecation` and `Sunset` headers. Current
deprecations:

- `GET /stock/{sku}` (singular) — use `GET /stock?sku=…`; sunset 2027-03-31.

## Changelog

- 2026-10-05 — Health endpoints documented.
- 2026-10-01 — `GET /orders/{number}/events`.
- 2026-08-12 — Problem details for every error.
- 2026-06-02 — `Idempotency-Key` on all `POST` endpoints.
- 2026-03-10 — UPS webhook.

## Examples

### Placing an order with curl

```bash
curl -sS https://warehouse.internal.example.com/api/v1/orders \
  -H "Authorization: Bearer $TOKEN" \
  -H "Idempotency-Key: $(uuidgen)" \
  -H 'Content-Type: application/json' \
  -d '{"customer_id":"cus_8812","lines":[{"sku":"sku-1001","quantity":2}]}'
```

### Polling an order until it ships

```python
import time, requests

def wait_until_shipped(number, token):
    while True:
        r = requests.get(f"{BASE}/orders/{number}", headers={"Authorization": f"Bearer {token}"})
        r.raise_for_status()
        if r.json()["state"] in ("SHIPPED", "INVOICED", "CANCELLED"):
            return r.json()
        time.sleep(30)
```

Prefer the tracking webhooks to polling where you can.
