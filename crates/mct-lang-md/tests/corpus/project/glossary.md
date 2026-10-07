---
title: Glossary
aliases:
  - Glosario
  - Terms
tags: [glossary, "#domain", 2026]
---

# Glossary

The words of the warehouse domain, with the meaning they have in the code.
When a term here disagrees with the code, the code is wrong or this note is
stale — open a ticket either way. Linked from the [[handbook]] and from most
sections of the [[architecture/overview]]. #domain

## A

### Advance shipping notice

**ASN.** A supplier's announcement of a delivery: which purchase order, which
products, how many, when. Received from the ERP; matched on arrival during
put-away ([[architecture/overview#Replenishment]]).

### Approval

A supervisor's decision on a [[#Large order]]. Recorded with its author and
comment in the order history. A rejected order is cancelled.

### Available stock

`on_hand − reserved` for a product at a location. The number the shop shows.
Never negative ([[operations/runbook#Stock going negative]]).

## B

### Batch (billing)

The daily set of invoices sent to the ERP at 23:55. An invoice belongs to the
batch of the day it was issued, in Europe/Madrid time.

### Bundle

A product sold as a fixed set of other products (`bundle-patio`). Reserving a
bundle reserves its components; the bundle itself has no stock.

## C

### Carrier

A shipping company we book parcels with. Each has its own adapter
([[architecture/decisions#ADR-006 One adapter per carrier]]) and webhook
([[api/reference#Carrier webhooks]]).

### Category

A node of the catalog tree (`cat-kitchen` → `cat-cookware`). A product belongs
to exactly one leaf category.

### Credit note

The document that refunds all or part of an [[#Invoice number|invoice]].
Never edit or delete an invoice; issue a credit note.

### Cut-off time

The latest time an order can be paid and still ship the same day. Per carrier;
14:00 for most.

## D

### Delivery note

The carrier's proof of delivery: who signed, when, where. Attached to the
shipment when the `delivered` [[#Tracking event]] arrives.

### Domain event

A fact that happened in one context and that others may react to
(`StockReserved`, `OrderPaid`). Past tense, immutable, written to the
[[#Outbox]].

## E

### Exception (shipping)

A [[#Tracking event]] that needs a human: wrong address, damaged parcel,
refused delivery. Opens a back-office task assigned to the shipper lane.

### Exchange rate

The rate used to convert [[#Money]] between currencies on a given date. Taken
from the central bank's daily reference rates, cached for one hour.

## F

### FEFO

First expired, first out: the [[#Lot]] with the nearest expiry is picked
first. Applies to food and anything with a shelf life.

### Fulfilment

Everything between a paid order and a delivered parcel: picking, packing,
booking and handing over to the carrier.

## G

### Goods receipt

The scan of a supplier delivery against its [[#Advance shipping notice]].
Differences (short, over, damaged) are recorded before [[#Put-away]].

## I

### Invoice number

`FAC-{yyyy}-{seq:000000}`, assigned by Billing only, gapless within a year as
the tax authority requires. A number is consumed even if the invoice is later
credited.

## L

### Large order

An order whose total exceeds `orders.large-threshold` (5 000 € by default).
Needs an [[#Approval]] before stock is reserved
([[architecture/overview#Approval of large orders]]).

### Location

A shelf address in the warehouse, `A-03-2`: zone, aisle, level. Bulky goods
use `Z-BULK-n`.

### Lot

A batch of a product with the same expiry date (`lot-2001-2609`). Picking is
first-expired-first-out.

## M

### Money

An amount with an explicit currency, as a decimal with scale 2 (0 for JPY).
Never a float. Arithmetic across currencies is not allowed; conversion goes
through exchange rates with an explicit date
([[architecture/decisions#ADR-005 Decimal money with explicit currency]]).
#money

### Movement

A change of stock at a location: receipt, put-away, pick, adjustment, transfer.
Every movement has a document (order, receipt, count) and an author.

## N

### Net weight

The product's weight without packaging, in grams. Shipping uses the gross
weight (with packaging), stored separately.

## O

### On hand

The physical quantity on the shelf, as last counted or moved.

### Order number

`PED-{yyyy}-{seq:000000}`, assigned when the order is placed. Distinct from
the order id (a ULID) used internally and in events.

### Outbox

The table events are written to in the same transaction as the state change,
then relayed to the broker
([[architecture/decisions#ADR-002 Transactional outbox]]).

## P

### Picking

Collecting an order's items from their locations. Split by zone and done in
parallel; see [[architecture/overview#Picking and packing]].

### Price list

A set of prices overriding the base price for some customers or countries
(`pl-wholesale`, `pl-uk`).

### Product

A thing we sell, identified by its [[#SKU]]. Owned by the catalog context.

### Put-away

Placing received goods on their locations. The suggested location is the one
with the most free space in the right zone.

## R

### Reorder point

The available-stock level that triggers replenishment for a product. Computed
weekly ([[architecture/overview#Replenishment]]).

### Reservation

Stock promised to an order line at a location. Created when the order is
placed, released on cancellation or expiry, consumed by picking.

#### Reservation expiry

30 minutes after creation if the order is not paid
([[architecture/decisions#ADR-003 Reserve before payment]]).

#### Split reservation

A line served from more than one location because none had enough. Rare.

## S

### Safety stock

Extra stock kept to absorb demand variability; part of the reorder point
formula.

### Service level

The probability of not running out of stock during a replenishment lead time.
Target 97 %.

### SKU

Stock keeping unit: `sku-` followed by four digits. Variants have their own
SKU (`sku-1201-wht`).

### Stock level

The row of `on_hand` and `reserved` for one product at one location.

## T

### Tenant

A shop brand served by this warehouse. Every request and log line carries it.

### Tracking event

A status update from a carrier: picked up, in transit, out for delivery,
delivered, exception ([[api/reference#Carrier webhooks]]).

## U

### Unit of measure

How a product is counted: unit, kilogram, litre or metre. Quantities in orders
and stock are always in the product's unit of measure.

## V

### Variant

A product that differs from its siblings in one attribute (colour, size) and
has its own [[#SKU]]. Variants share photos and description with their parent.

### VAT

Value added tax. Rates by category: 21 % general, 10 % food, 4 % books.
Computed **per line**, rounded once per line, then summed; computing per unit
caused one-cent differences ([[operations/runbook#2026-09-09 Invoices with one-cent differences]]).
#money

| Category       | Rate  |
|----------------|-------|
| General        | 21 %  |
| Food           | 10 %  |
| Books          | 4 %   |
| Intra-EU B2B   | 0 %   |

## W

### Wave

A group of orders picked together to shorten walking routes. Waves are formed
every 15 minutes per zone.

### Wishlist

Products a customer saved for later in the shop. Price drops on wishlisted
products send a notification.

## Z

### Zone

A part of the warehouse with its own storage conditions: ambient, chilled,
frozen, bulky. Picking is split by zone.

## Spanish ↔ English

The back office is in Spanish; the code is in English.

| Español              | English              |
|----------------------|----------------------|
| Pedido               | Order                |
| Albarán              | Packing slip         |
| Factura              | Invoice              |
| Abono                | Credit note          |
| Existencias          | Stock                |
| Ubicación            | Location             |
| Lote                 | Lot                  |
| Recuento             | Count                |
| Reposición           | Replenishment        |
| Transportista        | Carrier              |

Tags used across the notes: #domain, #money, #almacén, #on-call.
