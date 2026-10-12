---
title: Project layout and modules
area: 1
status: decided
checked-with: jadpo 3ce094d0 (2026-10-11)
rules:
  - data.project_role_invalid
  - toolchain.empty_project
  - module.*
  - semantic.duplicate_declaration
  - semantic.name_case
  - semantic.unknown_name
  - semantic.unknown_callee
  - semantic.not_callable
  - semantic.wrong_name_kind
  - semantic.standard_*
  - syntax.unexpected_character
  - syntax.unterminated_string
  - syntax.invalid_escape
  - syntax.expected_declaration
  - syntax.expected_name
  - syntax.unexpected_token
  - syntax.empty_import
  - syntax.import_requires_module
  - syntax.route_export_invalid
---

# Project layout and modules

How a Jadpo project is laid out on disk, how one file sees declarations from
another, and the naming rules every name follows.

## Project layout

> **Changing.** Decided on 2026-10-12 to replace this with a fixed root and domain folders; see [Decided layout](#decided-layout). This section describes the compiler today.

A project is every `.jadpo` file under the folder you give the compiler. A few
folder names have a recognised role. The golden todo app is laid out like this:

```text
application.jadpo        application and authentication
config.jadpo             configuration
entities/todo.jadpo      one entity, with its own queries, actions and failures
entities/user.jadpo
values/domain.jadpo      types without identity
values/contracts.jadpo   request and response shapes
routes/todos.jadpo       HTTP routes
```

Here is the smallest project that uses the role folders:

```jadpo project=layout file=entities/customer.jadpo
entity Customer {
    id: Uuid
    identity: id
}
```

```jadpo project=layout file=values/labels.jadpo
type CustomerName = Text {
    min_length: 1
    max_length: 120
}
```

What the compiler checks today:

| Folder | Meant for | Checked today |
|---|---|---|
| `entities/` | One entity per file, with its own queries, actions and failures | Exactly one entity per file, and no actions unrelated to it. The file name does not have to match the entity. Plain types are also accepted here. |
| `values/` | Types without identity: domain values, request and response shapes | Entities are rejected. Failures and actions are accepted. |
| `queries/` | Reads across several entities | Entities are rejected. |
| `workflows/` | Actions that change several entities | Entities are rejected. |
| `routes/` | HTTP routes | Entities are rejected. Types and actions are accepted. |
| Root, `jobs/` and any other folder | Not defined | Nothing is checked. An entity at the root or in a made-up folder is accepted. |

Folders are organisation only: moving a file never changes what a declaration
means or its database identity.

## Source files

> **Changing.** All code will live in domain folders and names stay unique project-wide; see [Decided layout](#decided-layout).

File order and declaration order never matter: a declaration can refer to one
defined later or in another file. Without module headers the whole project
shares one namespace, and every declaration is visible everywhere. The golden
todo app works this way.

Each name can be declared once per project. A second `Count` in another file is
an error (see `semantic.duplicate_declaration` below).

## Modules

> **Being removed.** Decided on 2026-10-12: `module`, `import` and module-level `public` are replaced by domain folders, owner-qualified calls and public declarations per domain; see [Decided layout](#decided-layout).

A file opts into modules by starting with a `module` header. Declarations are
private to their module unless marked `public`, and another module uses them
only through a selective `import` that names each declaration.

```jadpo project=modules file=pricing.jadpo
module shop.pricing

// Prices are whole pence; decimals arrive with the money type.
public type Pence = Int {
    min: 0
}

public function add_delivery(subtotal: Pence) -> Pence {
    return subtotal
}
```

```jadpo project=modules file=checkout.jadpo
module shop.checkout
import shop.pricing { Pence, add_delivery }

function order_total(subtotal: Pence) -> Pence {
    return add_delivery(subtotal)
}
```

The rules:

- One module per file, declared on the first line. Module names are explicit;
  folder names never become module names.
- Once any file declares a module, every file must.
- Imports name each declaration. There are no wildcards, aliases, re-exports
  or relative imports.
- An imported function is called by its bare name. `shop.pricing.add_delivery(...)`
  is not a second way to call it.
- Modules cannot import each other in a cycle.
- Routes cannot be marked `public`; who can reach a route is decided by the
  route and its policy.
- Names stay unique across the whole project, even in different modules.

## Naming

> **Changing.** Module names go away, consecutive capitals will be rejected (`VatRate`, not `VATRate`) and calls will name their owner; see [Decided layout](#decided-layout).

Every name has one spelling, decided by what kind of thing it names.

| Kind of name | Form | Examples |
|---|---|---|
| Types and declared contracts (entities, failures, services, config, events) | `UpperCamelCase` | `Todo`, `TodoNotFound`, `ReminderMail` |
| Values, fields, parameters, functions, actions, queries, jobs, variants | `lower_snake_case` | `due_at`, `order_total`, `overdue_reminders` |
| Module names | dotted `lower_snake_case` | `shop.order_pricing` |
| Standard library | lowercase namespace, always qualified | `temporal.in_zone(...)` |
| Keywords | lowercase | `function`, `return`, `match` |

Identifiers are ASCII letters, digits and underscores, and cannot start with a
digit. The compiler checks the case and suggests the right spelling.

## Comments

> **Changing.** `///` documentation comments were decided on 2026-10-12; see [Decided layout](#decided-layout).

Line comments start with `//`. They carry no meaning and are kept for tooling.
There are no block comments and no documentation comments yet.

## Every error has the same shape

Each compiler error gives the location, a reason, a suggested next step and who
should act on it, plus a stable rule ID with a help link. Tools read the same
fields as JSON with `jadpo check <project> --diagnostic-format=json`.

## Errors

> **Changing.** The `module.*` and import errors below go away with modules, and new layout and naming errors arrive with the rework; see [Decided layout](#decided-layout).

Every error this area can produce, with an example that triggers it and the
compiler's actual output.

### `data.project_role_invalid`

```jadpo project=e-role file=entities/customer.jadpo expect=data.project_role_invalid
entity Customer {
    id: Uuid
    identity: id
}

action unrelated() -> Bool {
    return true
}
```

### `toolchain.empty_project`

```jadpo project=e-empty file=notes.jadpo expect=toolchain.empty_project
// Nothing declared yet.
```

### `semantic.duplicate_declaration`

```jadpo project=e-duplicate file=a.jadpo hidden
type Count = Int {
    min: 0
}
```

```jadpo project=e-duplicate file=b.jadpo expect=semantic.duplicate_declaration
type Count = Int {
    min: 1
}
```

### `semantic.name_case`

```jadpo project=e-case file=checkout.jadpo expect=semantic.name_case
type Pence = Int {
    min: 0
}

function orderTotal(subtotal: Pence) -> Pence {
    return subtotal
}
```

### `semantic.unknown_name`

```jadpo project=e-unknown-name expect=semantic.unknown_name
type Pair = Object {
    first: Missing
}
```

### `semantic.unknown_callee`

```jadpo project=e-unknown-callee expect=semantic.unknown_callee
type Count = Int {
    min: 0
}

function f(x: Count) -> Count {
    return lookup(x)
}
```

### `semantic.not_callable`

```jadpo project=e-not-callable expect=semantic.not_callable
failure KnownFailure {
    kind: InvalidValue
    code: "known_failure"
}

action invalid_call() -> Unit {
    var second = KnownFailure()
}
```

### `semantic.wrong_name_kind`

```jadpo project=e-wrong-kind expect=semantic.wrong_name_kind
type Count = Int {
    min: 0
}

function g(x: Count) -> Count {
    return x
}

type Pair = Object {
    first: g
}
```

### `semantic.standard_namespace_reserved`

```jadpo project=e-std-namespace expect=semantic.standard_namespace_reserved
type Count = Int {
    min: 0
}

function temporal(x: Count) -> Count {
    return x
}
```

### `semantic.standard_operation_reserved`

```jadpo project=e-std-operation expect=semantic.standard_operation_reserved
type Count = Int {
    min: 0
}

function in_zone(x: Count) -> Count {
    return x
}
```

### `semantic.standard_operation_qualification`

```jadpo project=e-std-qualify expect=semantic.standard_operation_qualification
type Flag = Bool {}

action invalid_standard_call() -> Flag {
    var elapsed = between()
    return Flag(true)
}
```

### `module.public_requires_module`

```jadpo project=e-public expect=module.public_requires_module
public type Thing = Int {
    min: 0
}
```

### `module.module_required`

```jadpo project=e-module-required file=one.jadpo hidden
module a.one

public type One = Int {
    min: 0
}
```

```jadpo project=e-module-required file=loose.jadpo expect=module.module_required
type Loose = Int {
    min: 0
}
```

### `module.duplicate_module`

```jadpo project=e-duplicate-module file=one.jadpo hidden
module a.one

public type One = Int {
    min: 0
}
```

```jadpo project=e-duplicate-module file=also_one.jadpo expect=module.duplicate_module
module a.one

public type Two = Int {
    min: 0
}
```

### `module.unknown_module`

```jadpo project=e-unknown-module file=two.jadpo expect=module.unknown_module
module a.two
import a.nowhere { Thing }

public type Two = Int {
    min: 0
}
```

### `module.unknown_export`

```jadpo project=e-unknown-export file=one.jadpo hidden
module a.one

public type One = Int {
    min: 0
}
```

```jadpo project=e-unknown-export file=two.jadpo expect=module.unknown_export
module a.two
import a.one { Missing }

public type Two = Int {
    min: 0
}
```

### `module.private_import`

```jadpo project=e-private file=pricing.jadpo hidden
module shop.pricing

public type Pence = Int {
    min: 0
}

function add_delivery(subtotal: Pence) -> Pence {
    return subtotal
}
```

```jadpo project=e-private file=checkout.jadpo expect=module.private_import
module shop.checkout
import shop.pricing { Pence, add_delivery }

function order_total(subtotal: Pence) -> Pence {
    return add_delivery(subtotal)
}
```

### `module.import_required`

```jadpo project=e-import-required file=one.jadpo hidden
module a.one

public type Thing = Int {
    min: 0
}
```

```jadpo project=e-import-required file=two.jadpo expect=module.import_required
module a.two

type Pair = Object {
    first: Thing
}
```

### `module.duplicate_import`

```jadpo project=e-duplicate-import file=one.jadpo hidden
module a.one

public type Thing = Int {
    min: 0
}
```

```jadpo project=e-duplicate-import file=two.jadpo expect=module.duplicate_import
module a.two
import a.one { Thing, Thing }
```

### `module.import_conflict`

```jadpo project=e-import-conflict file=one.jadpo hidden
module a.one

public type Thing = Int {
    min: 0
}
```

```jadpo project=e-import-conflict file=two.jadpo expect=module.import_conflict
module a.two
import a.one { Thing }

type Thing = Int {
    min: 0
}
```

### `module.self_import`

```jadpo project=e-self-import expect=module.self_import
module a.one
import a.one { Thing }

public type Thing = Int {
    min: 0
}
```

### `module.import_cycle`

```jadpo project=e-cycle file=one.jadpo hidden
module a.one
import a.two { Two }

public type One = Int {
    min: 0
}
```

```jadpo project=e-cycle file=two.jadpo expect=module.import_cycle
module a.two
import a.one { One }

public type Two = Int {
    min: 0
}
```

### `module.standard_namespace_import`

```jadpo project=e-std-import expect=module.standard_namespace_import
module a.one
import temporal { in_zone }

public type One = Int {
    min: 0
}
```

### `module.standard_namespace_reserved`

```jadpo project=e-std-module expect=module.standard_namespace_reserved
module temporal

public type One = Int {
    min: 0
}
```

### `syntax.import_requires_module`

```jadpo project=e-import-without-module expect=syntax.import_requires_module
import a.two { Thing }
```

### `syntax.empty_import`

```jadpo project=e-empty-import file=two.jadpo hidden
module a.two

public type Two = Int {
    min: 0
}
```

```jadpo project=e-empty-import file=one.jadpo expect=syntax.empty_import
module a.one
import a.two { }
```

### `syntax.route_export_invalid`

```jadpo project=e-route-public expect=syntax.route_export_invalid
module a.one

public route GET /health {
    auth: none
}
```

### `syntax.expected_declaration`

```jadpo project=e-expected-declaration expect=syntax.expected_declaration
Count = Int
```

### `syntax.expected_name`

```jadpo project=e-expected-name expect=syntax.expected_name
type Pair = Object {
    : Int
}
```

### `syntax.unexpected_token`

```jadpo project=e-unexpected-token expect=syntax.unexpected_token
type Count = Int {
    min: 0
}

function f(x: Count -> Count {
    return x
}
```

### `syntax.unexpected_character`

```jadpo project=e-unexpected-character expect=syntax.unexpected_character
type Count = Int {
    min: 0 $
}
```

### `syntax.unterminated_string`

```jadpo project=e-unterminated expect=syntax.unterminated_string
type Label = Text {
    pattern: "abc
}
```

### `syntax.invalid_escape`

```jadpo project=e-escape expect=syntax.invalid_escape
type Label = Text {
    pattern: "a\qb"
}
```

## Review

### Status

| Feature | Status |
|---|---|
| Role folders | Partly checked. Only "entities live in `entities/`, one per file" is enforced, and an entity at the root or in a made-up folder is still accepted. The documented rule that failures cannot go in `values/` is not enforced. |
| Files, single namespace, order independence | Works in general. |
| Modules, `public`, selective imports, cycle check | Works in general, within the limits listed above. The golden app does not use modules. |
| Naming case rules | Works in general. |
| Reserved words | Inconsistent: `var return = x` is accepted, but `return` cannot then be read back. A fix policy was chosen on 2026-10-01 and not built. |
| Comments | `//` only. Documentation comments are undesigned. |
| Golden app layout | A todo route (`GET /todos/{todo_id}`) sits in the health route file and is accepted. The reminder job, mail service and readiness check were dropped in the migration, so the layout has never held them. |

### Consistency with the rest of the language

- **Two ways to organise a project.** Folders carry roles (`entities/`,
  `routes/`) while modules carry visibility, and the two are unrelated: a file's
  folder says nothing about its module and vice versa.
- **Import lists are the only comma-separated brace block.** Every other `{ }`
  block in the language holds one `name: value` item per line; `import m { A, b }`
  holds a comma list on one line.
- **Three call spellings, by rule.** Your own functions are called bare
  (`add_delivery(x)`), the standard library is always qualified
  (`temporal.in_zone(x)`), and entity operations go through the entity
  (`Todo.get(id)`). This is deliberate (the qualifier shows who owns the
  operation) and consistently enforced.
- **Declaration headers differ.** `type Name = Base { }` uses `=`, while
  `entity Name { }`, `failure Name { }` and `service Name { }` do not. This
  spans several areas and is flagged for the end-of-review syntax pass.
- **Separation risks duplication, not breakage.** With no imports every
  reference resolves wherever it lives, but an agent working in one domain
  can write a helper that already exists in another. Placement cannot prevent
  two copies. Mitigations: agents look things up in the compiler's project map
  (`jadpo inspect`, the language server) rather than browsing folders, and a
  possible compiler warning for near-identical functions or types.
  The compiler also generates a catalogue of every library function with its
  signature and documentation comment; the Jadpo agent skill tells agents to
  consult it before writing a helper. Nothing is maintained by hand.
- **The formatter keeps your blank lines and import order.** Two people can
  format the same file differently and both pass `jadpo fmt --check`.

### Decided layout

Shaped in discussion with Richard on 2026-10-11 and 12, on top of three
pieces of research in `docs/language/research/` (what developers like and
dislike in other frameworks' layouts, six realistic applications from this
todo app up to a 72-entity platform, and candidate layouts tested against
them) and an independent review of this proposal against that research and
fresh evidence. Decided on 2026-10-12; nothing here is built yet.

**The shape.** Every project has the same shape at every size. The root holds
a fixed set of files and folders. All application code lives in business
areas (domains) under `domains/`, one level deep, and every domain contains
the same fixed folders with no subfolders. A file's path follows from two
facts: its domain and what kind of declaration it is.

```text
app.jadpo                  application, principal, health, readiness, language version
config.jadpo               configuration: names, types, bindings, defaults
authentication.jadpo       authentication methods and their failures
policy.jadpo               application-wide policy, such as tenancy
domains/
  planning/
    entities/
      todo.jadpo           Todo with its actions, queries, policy and the types only it uses
      todo.test.jadpo      its tests, beside it
      user.jadpo
      user.test.jadpo
      service.jadpo
      service_credential.jadpo
    types/
      todo_view.jadpo      shared within the domain: used by Todo and User
    actions/
      send_due_reminders.jadpo
    routes/
      todos.jadpo          route group Todos, prefix /todos: every /todos route
      todos.test.jadpo
      users.jadpo
    schedules/
      overdue_reminders.jadpo
    integrations/
      reminder_mail.jadpo
      reminder_mail.contract.json
database/
  identities.json          stable identity of every table and column
  migrations/
    20261012T0930_initial/
build/                     generated output, never committed
```

The same rules for a large application:

```text
app.jadpo  config.jadpo  authentication.jadpo  policy.jadpo
shared/
  types/          money.jadpo  country_code.jadpo  address.jadpo
  libraries/      rounding.jadpo
domains/
  ordering/
    entities/     cart.jadpo  order.jadpo  order.test.jadpo
    actions/      checkout.jadpo
    queries/      order_history.jadpo
    routes/       orders.jadpo  admin_orders.jadpo
    events/       order_event.jadpo
    subscribers/  cancel_unpaid_orders.jadpo
    schedules/    expire_quotes.jadpo
    fixtures/     sample_catalogue.jadpo
  inventory/
    actions/      reserve_stock.jadpo        public: called by checkout in one transaction
  payments/
    integrations/ stripe_v2024.jadpo  stripe_v2024.contract.json
    routes/       stripe_webhooks.jadpo
  notifications/
    integrations/ postmark.jadpo
    subscribers/  send_order_confirmation.jadpo
  catalogue/  fulfilment/  identity/
database/
  identities.json
  migrations/     20260901T1000_initial/  20261012T0930_add_order_channel/
  seeds/          local_demo.jadpo
translations/
tests/            checkout_to_delivery.test.jadpo
build/
```

#### The root

- Only these entries: `app.jadpo`, `config.jadpo`, `authentication.jadpo`,
  `policy.jadpo`, `domains/`, `shared/`, `database/`, `translations/`,
  `tests/` and `build/`.
- `app.jadpo` also records the project's Jadpo language version, replacing
  today's hidden `.app/project.json`.
- A `.jadpo` file anywhere else is an error. Folders with no `.jadpo` files
  (`docs/`, `.github/`) are ignored.
- There is no `env.jadpo`. `config.jadpo` declares every setting; values come
  from the platform environment in production and from one git-ignored
  `.env.local` file locally, with secrets entered through `jadpo config set`.
- A single file handed to the compiler on its own (a test case, a snippet in
  these docs) is checked as one implicit domain with the layout rules off.

#### Domains

- Every project has at least one domain, and all code outside the root files
  lives in one. The folder name is the domain's name; nothing in the code
  declares it, so the two cannot disagree. Domains are one level deep and
  never nest.
- Name a domain after a business area (`ordering`, `planning`), not a plural
  entity (`orders`, `todos`): domain names appear in calls, so they must not
  read like variables. Generic names (`misc`, `common`, `utils`, `helpers`,
  `core`) are rejected for domains and libraries alike.
- Split a domain only along a seam the compiler shows has no references
  across it ("these 12 entities never reference these 9"). A large domain
  whose parts depend on each other stays large; size alone is not a reason to
  split.
- `jadpo domain rename`, `split` and `merge` commands do the moves and update
  every call.
- A domain contains only these folders, each named after its declaration
  keyword, and only when they have files. No subfolders; revisit if any
  folder in a real application passes about 60 files.

| Folder | Holds | Notes |
|---|---|---|
| `entities/` | One entity per file, with its own actions, queries, failures and policy | The entity's dossier |
| `types/` | Types and failures used by more than one file in the domain | Single-use ones live beside their user |
| `actions/` | Actions that change several entities in one transaction | Called by triggers or, if `public`, by other domains |
| `queries/` | Reads across several entities | |
| `libraries/` | Named groups of helper functions, e.g. `library rounding` | Called like the standard library: `rounding.half_even(x)` |
| `routes/` | Named route groups with a prefix the compiler checks | HTTP trigger: the URL is written in the source |
| `schedules/` | Time triggers: when to run which action | Replaces "jobs" |
| `events/` | Event definitions | |
| `subscribers/` | Event triggers, including long-running coordination | Replaces durable "workflows" |
| `integrations/` | Third-party APIs, one declaration per provider version, contract beside it | Replaces "services"; versioning designed in area 11 |
| `fixtures/` | Test data shared by several test files in the domain | |

#### Files and names

- One primary declaration per file, named after it: `ServiceCredential` lives
  in `service_credential.jadpo`. Route groups and libraries are named
  declarations, so the same rule covers them. Supporting types, failures and
  helpers used only by that file live in it too.
- A name must convert to its file name and back unchanged, so consecutive
  capitals are not allowed: `VatRate`, not `VATRate` (Rust's guideline too).
- Names are unique across the whole project and across kinds, so every file
  name is unique and a search for a name finds exactly one file. Domain,
  library and standard-library names are unique among themselves, and local
  variables cannot reuse them. Standard-library names are fixed per language
  version, so a new standard namespace never breaks an existing library.
- Tests sit beside their subject as `<name>.test.jadpo`. Scenarios that span
  several domains go in the root `tests/`. The kind of test (pure,
  integration, route, schedule) is known from what it calls, so it is not a
  folder; `jadpo test --kind route` runs one kind.
- Non-Jadpo files (provider contracts, email templates, search mappings) sit
  beside the file that uses them and must be referenced from it.

#### Sharing and promotion

- A type, failure or helper used by one file lives in that file. When a
  second file in the domain uses it, it must move to the domain's `types/` or
  `libraries/`; when a second domain does, to `shared/` (pure types and
  libraries only). Tests never count as users, a route only counts when
  nothing else uses the type, and a failure counts where it is raised.
- Promotion is required: the compiler reports it with a one-step fix, applied
  through the editor or `jadpo fmt --layout`, never silently on save.
  Demotion is never forced: a promoted declaration stays where it is until
  nothing uses it, when it becomes a dead-code error. This avoids code moving
  back and forth, broken `git blame` and merge conflicts on moves.
- Promotion is a move, not a rewrite: the library keeps its name, so calls
  such as `rounding.half_even(x)` and the function text stay identical and
  behaviour cannot change.
- A helper that depends on one domain's types (`order_math.total(order)`)
  stays in that domain and becomes public when another domain calls it.
  Depending on two domains' types is an error that needs a design decision.
  Making a helper generic, for example extracting `money.sum(amounts)` for
  `ordering` and `payments` to share, is a deliberate refactor.
- Duplicates are caught at the point of writing: when a new helper looks like
  an existing one, `jadpo check` says so and names it ("a similar library
  exists: `rounding.half_even` in `shared/libraries/rounding.jadpo`"). The
  compiler also generates a catalogue of every library function with its
  signature, documentation comment and callers.
- A function's description is written once, as a `///` comment above it.
  The catalogue, the caller list and the domains that use it are computed
  and shown in the editor, never written into source files.
- Before shared or public code changes, its reach is visible in three ways:
  find-references in the editor (available today), a `jadpo impact <name>`
  command listing calling domains, entry points and covering tests, and
  `jadpo check` itself reporting which domains a change affects and running
  their tests.

#### Ownership and calls replace modules

- A domain owns everything in it and is the visibility boundary.
  Declarations are private to their domain unless marked `public`.
- Entity identities, types, failures, events, queries, libraries and actions
  can be public. A public action is the domain's write interface: checkout in
  `ordering` can call `inventory.reserve_stock(...)` and both changes commit
  or roll back together in one database transaction, using the transaction
  machinery that exists today. Another domain never touches the entities
  directly. If a domain ever moves to its own database, the compiler rejects
  the shared transaction rather than losing the all-or-nothing guarantee.
- Events are for reactions that do not need to be all-or-nothing, such as
  sending a confirmation email: they run after the commit, so a failed email
  never rolls back an order.
- The dot names the owner of what you call: `temporal.in_zone(x)`,
  `rounding.half_even(x)`, `Todo.get(id)`, `inventory.reserve_stock(...)`.
  Your own domain's actions and queries are called bare. Searching for
  `inventory.` finds calls into that domain, but not entity operations such as
  `Order.find`, which are named by their entity.
- There are no `module` headers and no `import` lists.
- This reverses parts of two accepted contracts, which must be updated if it
  is adopted: the naming contract (NAME-D03 module names, NAME-D04 bare calls
  within a module, NAME-D09 imports) and the event design's rule that
  ownership follows module names.

#### Database

- `database/identities.json` is the registry that gives every table, column
  and index a stable identity, so renaming a field renames its column instead
  of dropping it. Compiler-maintained, committed and reviewed (today it is
  `schema.identities.json` at the root). One sorted entry per line, so two
  branches that change the schema produce a readable merge.
- Each schema change is a folder under `database/migrations/`, named by date,
  time and purpose, holding your decisions on destructive changes, the
  generated SQL for each database and any data backfill written in Jadpo. The
  compiler keeps a record of the latest migration so history stays linear.
- `database/seeds/` holds local demonstration and development data.

#### What it looks like in code

Illustrative; the exact syntax of each piece is settled in the area named.

A library, its documentation comment and a call from another domain (areas 1
and 3):

```jadpo fragment
// shared/libraries/rounding.jadpo
/// Rounds to the nearest whole penny, ties to the even penny.
library rounding {
    function half_even(amount: Price) -> Price { ... }
}

// domains/ordering/actions/checkout.jadpo
var total: Price = rounding.half_even(subtotal)
```

A public action called inside another domain's transaction (areas 1 and 6):

```jadpo fragment
// domains/inventory/actions/reserve_for_cart.jadpo
public action reserve_for_cart(cart: Cart.Ref)
    fails OutOfStock -> Reservation { ... }

// domains/ordering/actions/checkout.jadpo
action checkout(cart: Cart.Ref)
    consistency: atomic
    fails OutOfStock, CartEmpty -> Order
{
    var reservation: Reservation = attempt inventory.reserve_for_cart(cart)
    var order: Order = attempt Order.place(cart, reservation)
    return order
}
```

A named route group with a checked prefix (area 7):

```jadpo fragment
// domains/planning/routes/todos.jadpo
routes Todos {
    prefix: /todos

    GET / { ... }
    GET /{todo_id} { ... }
    PATCH /{todo_id} { ... }
}
```

A schedule, a subscriber in another domain, and an integration pinned to a
provider version (areas 11 and 12):

```jadpo fragment
// domains/planning/schedules/overdue_reminders.jadpo
schedule overdue_reminders {
    every: 15m
    run: send_due_reminders()
}

// domains/notifications/subscribers/send_order_confirmation.jadpo
subscriber send_order_confirmation {
    on: OrderEvent.placed(order) -> attempt postmark.send(confirmation_for(order))
}

// domains/payments/integrations/stripe_v2024.jadpo
integration StripeV2024 {
    base_url: "https://api.stripe.com"
    api_version: "2024-06-20"
    ...
}
```

#### What the compiler would check

New errors would include `layout.unknown_location`, `layout.wrong_folder`,
`layout.file_name_mismatch`, `layout.promotion_required`,
`layout.unused_declaration`, `layout.test_without_subject`,
`layout.asset_unreferenced`, `layout.route_prefix_mismatch`,
`naming.round_trip`, `naming.folded_collision`, `naming.generic_name`,
`visibility.private_cross_domain`, `layout.call_direction` and a
`library.similar_exists` warning.

#### Cost of building it

The `jadpo new` scaffold, about 200 single-file compile tests (covered by
the single-file rule above), the golden todo migration and the existing
examples all move to the new layout.

#### Decided in later areas

Route group syntax (area 7), the `schedule` keyword (area 12), integrations
and provider API versions (area 11), translation file format (area 10) and the
migration workflow (area 5). The layout only reserves their folders.

### Open questions

1. **`domains/` folder, or domains directly at the root?**
   *Recommendation:* keep `domains/`. Every root entry is then fixed by the
   language and projects differ only inside it, so nobody has to tell a domain
   from a reserved folder.
2. **Should the dot always name the owner of what you call?** This reverses
   the current rule that your own functions are called without a prefix.
   *Recommendation:* yes. Libraries read exactly like the standard library,
   imports disappear (missing imports are among the most common reasons LLM
   code fails to compile), and every call says where its function lives.
   Exact syntax is confirmed in area 3.
3. **Should creating a domain need human approval?** An agent that hits a
   cross-domain error might add a domain or reshuffle code rather than ask.
   *Recommendation:* yes: a new or renamed domain appears in the approval
   review like a policy change, because it changes who owns what.
4. **How is policy protected from agent edits?** Entity policy lives in the
   entity file. *Recommendation:* protect it through the approval check,
   which already fingerprints policy, so any policy change needs human
   approval wherever it lives.
5. **Multiple deployables (API, worker, back-office) as declarations in one
   project, not separate projects?** *Recommendation:* declarations. Each
   deployable selects its routes, schedules and subscribers and is compiled
   from what it reaches.
6. **Large entity files.** An order entity in a real shop could reach 800
   lines. *Recommendation:* no companion files for now; the formatter keeps a
   fixed section order. Revisit with measured sizes from the order/payment app.
7. **Reserved words.** *Recommendation:* build the chosen policy:
   control-flow and declaration words are fully reserved, with a short explicit
   list of words such as `input` and `value` that can be used as names.
8. **Documentation comments.** *Recommendation:* add `///` comments that
   attach to the following declaration and flow into generated documentation,
   the library catalogue and OpenAPI descriptions.
9. **Formatter owns layout within a file.** *Recommendation:* the formatter
   decides blank lines (one between declarations) and section order within a
   file, so equivalent code always looks identical.

### Decisions

Decided by Richard on 2026-10-12. Nothing here is built yet; the rework is listed in the roadmap.

| Topic | Decision | Date |
|---|---|---|
| Shape | Same at every size: fixed root, all code in one level of domains, the same fixed folders in every domain, no subfolders | 2026-10-12 |
| Folder names | Each folder is its declaration keyword in plural; `values/` becomes `types/`, `workflows/` becomes `actions/` | 2026-10-12 |
| Triggers | Routes, schedules (replacing "jobs") and subscribers each call actions; durable workflows are subscribers | 2026-10-12 |
| Root files | `app.jadpo`, `config.jadpo`, `authentication.jadpo`, `policy.jadpo`; no `env.jadpo` | 2026-10-12 |
| Compiler data | Visible `database/` folder for the identity registry, migrations and seeds, not a hidden folder | 2026-10-12 |
| Tests | Beside their subject; cross-domain scenarios in root `tests/`; no folder per test kind | 2026-10-12 |
| Domain naming | Folder name is the domain name; business-area names; junk-drawer names rejected | 2026-10-12 |
| Cross-domain writes | Public actions, sharing the caller's transaction; events for reactions that need not be all-or-nothing | 2026-10-12 |
| Promotion | Required with a one-step fix; demotion never forced | 2026-10-12 |
| Helpers | Descriptions written once as `///`; catalogue, callers and impact computed; duplicates flagged by `jadpo check` | 2026-10-12 |
| Domains folder | Domains live in `domains/`, so every root entry is fixed by the language | 2026-10-12 |
| Call naming | The dot names the owner of what you call; own-domain actions and queries are bare; no imports | 2026-10-12 |
| New domains | Creating or renaming a domain appears in the approval review, like a policy change | 2026-10-12 |
| Policy protection | Through the approval check wherever policy lives, not by file path | 2026-10-12 |
| Deployables | Declarations in one project, each compiled from what it reaches | 2026-10-12 |
| Large entities | No companion files for now; revisit with measured sizes from the order/payment app | 2026-10-12 |
| Reserved words | Control-flow and declaration words fully reserved, with a short explicit list of usable names | 2026-10-12 |
| Documentation comments | `///` comments attach to the next declaration and feed generated docs, the library catalogue and OpenAPI | 2026-10-12 |
| Formatting | The formatter owns blank lines and section order within a file | 2026-10-12 |
| Keywords match folders | `schedule` (folder `schedules/`), `integration` (`integrations/`) and `library` (`libraries/`) replace `job` and `service` | 2026-10-12 |
| Unnamed singletons | Declarations that exist once per project carry no name: `application { }`, `config { }`, `principal { }`, `locales { }` | 2026-10-12 |
