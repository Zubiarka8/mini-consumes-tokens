---
title: Warehouse engineering handbook
aliases: [Handbook, Manual de ingeniería]
tags:
  - home
  - team/warehouse
  - "#onboarding"
---

# Warehouse engineering handbook

This is the entry point of the warehouse team's notes. Everything else hangs
off it: the [[architecture/overview|architecture overview]], the decision log
in [[architecture/decisions]], the [[operations/runbook]] for incidents, the
[[api/reference|HTTP API reference]] and the [[glossary]]. #home

> [!note] How to read these notes
> Links are wiki links. `[[note#Heading]]` jumps to a section of another note,
> `[[#Heading]]` to a section of this one, and `![[note]]` embeds a whole note.
> Inline code such as `[[not-a-link]]` and `#not-a-tag` is ignored.

## Who we are

The team owns the order fulfilment path end to end: catalog import, stock,
orders, picking, shipping and invoicing. We sit between the shop
(front end owned by the e-commerce team) and the carriers. The architecture is
described in [[architecture/overview#Bounded contexts]].

### Team members

| Name            | Role               | Focus                         |
|-----------------|--------------------|-------------------------------|
| Itziar          | Tech lead          | [[architecture/overview]]     |
| Jon             | Backend            | Orders, payments              |
| Ana Belén       | Backend            | Inventory, replenishment      |
| Mikel           | SRE                | [[operations/runbook]]        |
| Nerea           | Product            | Roadmap, carriers             |

Links inside table cells are documentation for humans: the indexer does not
read tables, so the cells above create no relation.

### Rituals

- **Daily** at 09:30, 15 minutes, in the `#warehouse` channel huddle.
- **Planning** every other Monday; see [[#Planning]] below.
- **Incident review** within five working days of any SEV-1 or SEV-2, using
  the template in [[operations/runbook#Post-incident review]].
- **Architecture forum** on the first Thursday of the month; proposals become
  entries of [[architecture/decisions]].

### Working agreements

1. Every change goes through a pull request with one approval.
2. Anything touching money (pricing, invoices, refunds) needs two approvals,
   one from someone who has read [[glossary#Money]].
3. A failing `main` build is everybody's problem; whoever broke it reverts
   first and investigates later.
4. On-call hands over on Monday at 10:00 with the checklist in
   [[operations/runbook#Handover]].

## Getting started

Welcome! The first week is about getting a working environment, shipping a
small change and shadowing on-call for a day. #onboarding

### Day one

- [ ] Get access to the repository, the issue tracker and the chat.
- [ ] Read this handbook and the [[glossary]].
- [ ] Skim [[architecture/overview]] — the diagrams are enough for now.
- [ ] Set up the development environment (below).
- [ ] Pick a `good first issue` and say hello in `#warehouse`.

### Development environment

You need a JDK, Docker and a recent PostgreSQL client. Everything else runs
in containers.

```bash
git clone git@example.com:warehouse/warehouse.git
cd warehouse
./gradlew build            # compiles and runs the unit tests
docker compose up -d db    # PostgreSQL 16 with the schema from db/migration
./gradlew bootRun --args='--spring.profiles.active=dev'
```

The `dev` profile loads the seed catalog. A fenced block like the one above is
never scanned, so a `[[link]]` or `#tag` written inside it stays text:

```text
[[this-is-not-a-link]] #this-is-not-a-tag
```

#### Configuration

Local overrides go in `~/.warehouse/override.properties`. The keys are
described in [[architecture/overview#Configuration]], and the ones you will
most likely touch are:

- `db.url`, `db.user`, `db.password`
- `warehouse.worker-id` (any number between 1 and 31 locally)
- `mail.host` (point it at the MailHog container: `localhost`)

#### Test data

`./gradlew seed` loads three customers, the demo catalog and a few orders in
every state. The order numbers follow the format explained in
[[glossary#Order number]].

### Day two to five

- Pair with someone on a real ticket.
- Read the last three incident reviews linked from
  [[operations/runbook#Past incidents]].
- Shadow the on-call engineer for one shift.
- Ship your first change. Celebrate. 🎉

## How we work

### Planning

We plan in two-week cycles. Each cycle has a goal written as one sentence and
at most five tickets per person. The backlog is ordered by Nerea; anything
operational that came out of an incident review jumps the queue.

#### Estimation

We estimate in t-shirt sizes:

| Size | Meaning                                  |
|------|------------------------------------------|
| S    | Less than a day, no design discussion    |
| M    | A few days, maybe a short design note    |
| L    | A cycle; needs a decision in the log     |
| XL   | Split it                                 |

An **L** ticket needs a short design note. When the design changes something
structural, it becomes an entry of [[architecture/decisions#Decision log]].

### Code review

Reviews are about correctness first, then clarity. A useful checklist:

- Does the change keep the invariants of [[glossary#Reservation]] and
  [[glossary#Stock level]]?
- Are database migrations backwards compatible (see
  [[architecture/decisions#ADR-004 Expand and contract migrations]])?
- Is there a test that fails without the change?
- Will on-call understand the new log lines at 3 a.m.?

Comment with intent: prefix nits with `nit:` and blocking remarks with
`blocking:`. #code-review

### Releases

We deploy `main` continuously. A merged pull request reaches production in
about 20 minutes:

1. CI builds the image and runs the full test suite.
2. The image goes to staging; smoke tests run against it.
3. A canary takes 5 % of traffic for ten minutes.
4. If the error rate and latency stay within the budgets of
   [[operations/runbook#Service level objectives]], the rollout completes.

Rolling back is always safe and always the first move; the procedure is in
[[operations/runbook#Rollback]].

#### Feature flags

Risky changes ship dark behind a flag. Flags live in the configuration service
and are read through `FeatureFlags.isEnabled("name")`. Remove a flag within two
cycles of turning it on everywhere; a stale flag is a bug. #feature-flags

### Security basics

- Never paste customer data into tickets or chat; link the order number.
- Secrets live in the secret store. A secret in a commit is an incident, even
  if the commit was never pushed — rotate it.
- Dependencies are scanned on every build; a critical finding blocks the
  release until it is fixed or explicitly accepted by the tech lead.
- The threat model is reviewed yearly at the architecture forum. The last one
  is summarised in [[architecture/overview#Security]]. #security

## Domain primer

The short version of the domain, with the precise definitions in the
[[glossary]].

### Orders

An order goes through `NEW → RESERVED → PAID → PICKED → SHIPPED → INVOICED`,
or ends in `CANCELLED` from any state before `SHIPPED`. The state machine is
drawn in [[architecture/overview#Order lifecycle]]. Large orders (above the
threshold in [[glossary#Large order]]) wait for a supervisor's approval before
reservation.

### Stock

Stock is tracked per product and location. *On hand* is what is physically on
the shelf; *reserved* is promised to orders; *available* is the difference.
Reorder points trigger the replenishment process described in
[[architecture/overview#Replenishment]].

### Money

All amounts are decimals with an explicit currency; never floats. VAT rates
depend on the product category. See [[glossary#Money]] and
[[glossary#VAT]] before touching pricing code.

### Carriers

We integrate four carriers. Their quirks — label formats, cut-off times,
tracking webhooks — are collected in [[api/reference#Carrier webhooks]].

## Notes about these notes

These notes are plain Markdown in the repository, under `docs/`. Edit them in
any editor; Obsidian works well if you open `docs/` as a vault.

- Headings become sections that other notes can link to, so rename them with
  care: search for `[[note#Old heading]]` first.
- Use relative links for notes in the same folder, e.g.
  `[[./decisions]]` from inside `architecture/`.
- A link to a note that does not exist yet is fine; it shows up as a wanted
  page, like [[roadmap-2027]].
- Tags are for cross-cutting topics: #security, #performance, #money,
  #on-call. Digits alone are issue numbers, not tags: #1234.
- A URL fragment such as https://example.com/docs#install is not a tag either.

### Templates

New notes start from one of these:

- **Decision**: see [[architecture/decisions#Template]].
- **Incident review**: see [[operations/runbook#Post-incident review]].
- **API endpoint**: copy any section of [[api/reference]].

### Embedding

Short notes can be embedded rather than linked. The current on-call rota is
embedded here so it is visible from the home page:

![[operations/runbook#On-call rota]]

And the context diagram:

![[architecture/context-diagram.svg]]

## Frequently asked questions

### Where do I find the logs?

In the log platform, under the `warehouse` index. The fields are described in
[[operations/runbook#Logs and traces]].

### Who approves database changes?

Anyone on the team, as long as the migration follows
[[architecture/decisions#ADR-004 Expand and contract migrations]]. Destructive
migrations (dropping a column or table) need the tech lead.

### What do I do if I break production?

Roll back first ([[operations/runbook#Rollback]]), then tell `#warehouse`, then
investigate. Nobody gets blamed for a rollback. #on-call

### How do I add a carrier?

Implement `CarrierClient`, register it in the shipping context and add its
webhook to [[api/reference#Carrier webhooks]]. The checklist is in
[[architecture/decisions#ADR-006 One adapter per carrier]].

### Why is the reservation timeout 30 minutes?

Because payment providers give up after 25. The reasoning is recorded in
[[architecture/decisions#ADR-003 Reserve before payment]].

### Can I use floats for prices just this once?

No. #money

Glossary of acronyms
--------------------

The full list is in the [[glossary]]; these are the ones you will hear on day
one.

- **ASN** — advance shipping notice, sent by suppliers before a delivery.
- **SKU** — stock keeping unit; our product identifier (`sku-1001`).
- **SLO** — service level objective; see
  [[operations/runbook#Service level objectives]].
- **WMS** — warehouse management system; this one.

Changelog of this handbook
--------------------------

- 2026-10-05 — Security basics.
- 2026-10-01 — Added the domain primer and the FAQ.
- 2026-09-15 — Moved the runbook into its own note.
- 2026-09-01 — First version, migrated from the old wiki.
