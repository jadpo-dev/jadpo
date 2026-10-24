# Jadpo by example

Small, complete Jadpo projects, commented throughout, that together use
every feature the compiler has built so far. They add to the
[language pages](../docs/language/README.md), which stay the
specification: the pages say what each feature means, and these show it
working in a real project layout, with tests that pass.

Read them in order. Each one assumes the ones before it, and each has a
README saying what it teaches, how to run it and what to try next.

1. [01-first-project/](01-first-project/README.md): the project root, a
   domain, named types, `use` lines, a function, actions and tests.
2. [02-named-types/](02-named-types/README.md): every built-in
   representation, the rules a type can add, refinements, literals and
   conversions.
3. [03-objects-and-enums/](03-objects-and-enums/README.md): objects,
   inputs and outputs, field types, absent values, enums and tagged
   unions, and types built from other types.
4. [04-lists-and-loops/](04-lists-and-loops/README.md): lists and sets,
   `for`, `var mut`, and the `collection` operations with inline
   functions.
5. [05-logic/](05-logic/README.md): operators, `if` chains, every form
   of `match`, division with rounding, and recursion.
6. [06-failures/](06-failures/README.md): declaring failures, `fails`,
   `reject`, `attempt`, checks that can fail, a condition that proves one
   can't, and handling failures with `match`.
7. [07-testing/](07-testing/README.md): test kinds, fixtures, and
   asserting failures.
8. [08-domains-and-sharing/](08-domains-and-sharing/README.md): several
   domains, `public`, `shared/`, libraries, `use <domain>`, a public
   fixture, and a scenario in `tests/`.
9. [09-bakery-orders/](09-bakery-orders/README.md): a fuller project
   that puts all of it together.
10. [10-library-loans/](10-library-loans/README.md): stored records:
    entities, references, unique rules and indexes, the migrations that
    keep the database in step with them, the queries that read them, and
    the writes that change them: updates, patches, deletes, conflicts and
    a counter.
11. [11-cake-shop/](11-cake-shop/README.md): HTTP and sign-in: route
    groups, inputs from the query string, the body and a header,
    statuses, APIs and their records, deadlines and idempotent writes,
    browser sessions, API tokens and service keys, the caller passed to
    actions, simple grants, and route tests that sign in.
12. [12-recipe-book/](12-recipe-book/README.md): policy for many
    customers in one application: tenancy, membership roles, actions
    listed with who may run them, field restrictions, a role across the
    whole application and the permissions report.
13. [13-lettings-office/](13-lettings-office/README.md): settings:
    `config.jadpo`, a setting's types and defaults, the connections'
    addresses, constants, test values, and the keys a `crypto` call takes
    without code reading them: sealing, passwords, a shared secret, a
    private key and another service's tokens.
14. [14-studio-timetable/](14-studio-timetable/README.md): time: zones,
    times a rule repeats on, opening hours, ranges of time, a day's,
    week's, month's and year's bounds, calendar arithmetic and periods,
    the night the clocks change, and times shown to people.
15. [15-parcel-tracker/](15-parcel-tracker/README.md): translations:
    messages, a language file per locale, plural forms, enums with
    wording, a message two domains share, the reader's language and
    direction, Jadpo's own text and a language still in progress.
16. [16-market-hall/](16-market-hall/README.md): money: amounts in minor
    units, currencies fixed by a type, rounding, prices and deals,
    percentages, sharing to the penny, rates, cards and amounts shown to
    people.
17. [17-ebike-hire/](17-ebike-hire/README.md): physical quantities:
    types with bounds in units, kinds that combine, units only at the
    edges, every kind's units by name, exact decimal maths, and
    quantities a person typed.
18. [18-village-newsletter/](18-village-newsletter/README.md): text and
    the web: text operations, Markdown and rich text made safe, text in
    each language, links, colours, images, networks, tokens, digests and
    encodings.
19. [19-book-club/](19-book-club/README.md): lists, sets and maps,
    numbers, randomness, codes in other code systems, versions, a rule
    across a type's fields and declining a standard type.
20. [20-fell-race/](20-fell-race/README.md): geography in code:
    points, lines and polygons, distances, bearings, lengths and areas
    on the ellipsoid, inside, across and near, boxes and circles,
    polygons valid when built, WKT and GeoJSON layers.
21. [21-veg-box-rounds/](21-veg-box-rounds/README.md): stored places:
    PostGIS columns, queries nearest first, in a box, a circle or an
    area, the indexes they need, a map layer from a query, counts and
    sums by month, and a backfill worked out from each row.
22. [22-plant-helpline/](22-plant-helpline/README.md): stored vectors,
    the nearest query and its tuned index, distance in code, and
    prompts: checked prompt files, rendering, a shared prompt and a
    conversation.
23. [23-repair-cafe/](23-repair-cafe/README.md): events: declaring and
    emitting them, the outbox, subscribers and their retries, events
    that cause events, the event record, an action that only emits, and
    routes that answer 202 or wait for the follow-on work with
    `settle:`.
24. [24-seed-library/](24-seed-library/README.md): batches: work over
    many items a page at a time, an action per item, problems, the
    summary at the end, `key:`, a run's status and cancelling it, and a
    standard operation in a query's condition.
25. [25-allotment-rents/](25-allotment-rents/README.md): schedules, and
    background work in an application with tenants: a schedule reaching
    every tenant, a subscriber and a batch reaching one.
26. [26-delivery-desk/](26-delivery-desk/README.md): packages: an
    installed library package locked by its digest and used through its
    name, and the app's own integration with a provider's API in the
    package format, called through a connection; webhooks, a package's
    subscribers, events and poll, a recipe, recordings, a mail package
    and a queue filling capabilities, two versions side by side with a
    package's words, an OpenAPI import mapped to standard types, and
    several changing calls kept as steps.
27. [27-guest-house/](27-guest-house/README.md): email and signing in
    by email: an email as a type, its HTML and text templates checked
    against its fields, its subject worded like a message, the shared
    layout and a shared partial, sending by emitting after commit, the
    SMTP connection, tests that read what they sent, magic-link sign-in
    with routes and a sign-in email of the app's own, a link's token and
    a code, Jadpo's own words in another language, sign-in events, and
    passkeys, recovered through a magic link.
28. [28-lost-property/](28-lost-property/README.md): AI: a model
    package and its connection, budgets, embeddings one at a time and
    many at once, a search by meaning, typed answers from a model,
    personal data sent to it, a stream sent to a route, the fake every
    test answers through, and recorded answers.

`golden-todo/` holds the actual application, authored tests and versioned
44-case contract used by the shared native/workerd runner. It is an
application conformance fixture rather than a numbered teaching example.

## How the comments work

- `///` above a declaration describes it, as it would in any real
  project: editors and generated documentation show it.
- `//` comments are the lesson: what a part is for and why it is written
  that way, said once, where a reader first meets it. Later examples
  don't repeat what earlier ones taught.
- Each project's README says what the project is about, and its "What
  it teaches" list names the files in the order to read them, each
  beside the idea it shows; a file's tests sit beside it. `app.jadpo` comments only on `application`
  itself, where there is something to say, and each file opens with what
  it shows.

## Running them

Build `jadpo` once from the repository root:

```sh
cargo build --release -p jadpo-cli
```

Then point it at any example, for instance:

```sh
target/release/jadpo check examples/01-first-project
target/release/jadpo test examples/01-first-project
```

The gate (`python3 tools/verify.py`) checks, formats and tests every
example, and fails when a built feature appears in none of them. It also
makes each "Try next" edit and checks it gives the error or failed test
the README names: under each such bullet, a comment the rendered page
hides (`<!-- try {"file": …, "replace": …, "with": …, "check": […]} -->`)
spells the edit out exactly.
