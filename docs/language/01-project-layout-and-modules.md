---
title: Project layout and modules
area: 1
status: decided
checked-with: jadpo 3ce094d0 (2026-10-11)
rules:
  - toolchain.empty_project
  - layout.*
  - naming.*
  - visibility.*
  - semantic.duplicate_declaration
  - semantic.name_case
  - semantic.unknown_name
  - semantic.unknown_callee
  - semantic.not_callable
  - semantic.wrong_name_kind
  - semantic.own_domain_qualification
  - library.similar_exists
  - semantic.standard_namespace_reserved
  - semantic.standard_operation_qualification
  - syntax.unexpected_character
  - syntax.unterminated_string
  - syntax.invalid_escape
  - syntax.expected_declaration
  - syntax.expected_name
  - syntax.unexpected_token
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

::: today

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

:::

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

Control-flow and declaration words are reserved (question 7): none of them
names a declaration, field, parameter, variable or variant, and using one is
`syntax.expected_name`. They are `action`, `and`, `assert`, `attempt`,
`else`, `emit`, `entity`, `fails`, `false`, `fixture`, `for`, `function`,
`if`, `in`, `library`, `match`, `mut`, `none`, `not`, `or`, `package`,
`propagate`, `provide`, `public`, `query`, `reject`, `resource`, `return`,
`routes`, `schedule`, `seed`, `subscriber`, `test`, `true`, `type` and
`var`. A setting the language defines may still use one as its key
(`package: Stripe.v2024_06_20` in a connection, `test:` in configuration).

These words mean something only where the language expects them, and stay
usable as names everywhere else: `asc` and `desc` (a query's order), `end`
(templates), `matches` (the pattern operator), `input` and `value`, and the
words that open a declaration with no name, `acceptance`, `application`,
`authentication`, `config`, `flags`, `locales`, `migration`, `policy` and
`principal`. `include`, `pick`, `omit` and `optional` build a type's fields,
so they name anything but a field (area 2), and the standard-library and
generated namespaces (`collection`, `clock`, `config` ...) name no domain
or library.

## Comments

> **Changing.** `///` documentation comments were decided on 2026-10-12; see [Decided layout](#decided-layout).

Line comments start with `//`. They carry no meaning and are kept for tooling.
There are no block comments and no documentation comments yet.

## Every error has the same shape

Each compiler error gives the location, a reason, a suggested next step and who
should act on it, plus a stable rule ID with a help link. Tools read the same
fields as JSON with `jadpo check --json`.

## Errors

Every error this area can produce, with an example that triggers it and the
new compiler's real output from `jadpo check`. An example without `app.jadpo`
is checked as loose files, one implicit domain with the layout rules off; the
layout examples are small projects. One decided error is not built yet:
`layout.asset_unreferenced` waits for the reference forms of areas 11 and
16. The `module.*` errors, `syntax.import_requires_module`,
`syntax.empty_import`, `syntax.route_export_invalid` and
`data.project_role_invalid` went with modules and role folders.

### `toolchain.empty_project`

```jadpo check project=e-empty file=notes.jadpo expect=toolchain.empty_project
// Nothing declared yet.
```

```text output
error: This project declares nothing
  reason: Its one `.jadpo` file holds no declarations, only comments or nothing at all.
  next: Add a declaration, such as a `type`, `function` or `action` (agent)
  rule: toolchain.empty_project
  help: docs/language/01-project-layout-and-modules.html#toolchain-empty-project

check failed: 1 error, 1 file
```

### `semantic.duplicate_declaration`

```jadpo check project=e-duplicate file=a.jadpo hidden
type Count = Int {
    min: 0
}
```

```jadpo check project=e-duplicate file=b.jadpo expect=semantic.duplicate_declaration
type Count = Int {
    min: 1
}
```

```text output
error: `Count` is declared twice
 --> b.jadpo:1:6
  |
1 | type Count = Int {
  |      ^^^^^ declared again here
  reason: Names are unique across the whole project, whatever they name; `Count` is already a type in `a.jadpo`.
  next: Rename this type, or remove it if it's a copy (agent)
  rule: semantic.duplicate_declaration
  help: docs/language/01-project-layout-and-modules.html#semantic-duplicate-declaration

check failed: 1 error, 2 files
```

### `semantic.name_case`

```jadpo check project=e-case file=checkout.jadpo expect=semantic.name_case
type Pence = Int {
    min: 0
}

function orderTotal(subtotal: Pence) -> Pence {
    return subtotal
}
```

```text output
error: Function `orderTotal` isn't written in lower_snake_case
 --> checkout.jadpo:5:10
  |
5 | function orderTotal(subtotal: Pence) -> Pence {
  |          ^^^^^^^^^^ should be `order_total`
  reason: Types, entities and route groups are written in UpperCamelCase; functions, actions, queries, fields, variables and the rest in lower_snake_case.
  next: Rename `orderTotal` to `order_total`, here and where it's used (agent)
  rule: semantic.name_case
  help: docs/language/01-project-layout-and-modules.html#semantic-name-case

check failed: 1 error, 1 file
```

### `naming.round_trip`

```jadpo check project=e-round-trip expect=naming.round_trip
type HTTPStatus = Int {
    min: 100
    max: 599
}
```

```text output
error: `HTTPStatus` has consecutive capitals
 --> main.jadpo:1:6
  |
1 | type HTTPStatus = Int {
  |      ^^^^^^^^^^ reads back as `HttpStatus`
  reason: A name must convert to its file name and back unchanged: `HTTPStatus` becomes `http_status.jadpo`, which reads back as `HttpStatus`.
  next: Rename it `HttpStatus` (agent)
  rule: naming.round_trip
  help: docs/language/01-project-layout-and-modules.html#naming-round-trip

check failed: 1 error, 1 file
```

### `naming.folded_collision`

```jadpo check project=e-folded expect=naming.folded_collision
type DueDate = CalendarDate {}

function due_date(due: DueDate) -> DueDate {
    return due
}
```

```text output
error: `due_date` and `DueDate` share a file name
 --> main.jadpo:3:10
  |
3 | function due_date(due: DueDate) -> DueDate {
  |          ^^^^^^^^ also `due_date.jadpo`
  reason: Both names become `due_date.jadpo`, so a search for one finds the other; `DueDate` is a type in `main.jadpo`.
  next: Rename one of them so their file names differ (agent)
  rule: naming.folded_collision
  help: docs/language/01-project-layout-and-modules.html#naming-folded-collision

check failed: 1 error, 1 file
```

### `naming.generic_name`

```jadpo check project=e-generic expect=naming.generic_name
type Pence = Int {
    min: 0
}

library helpers {
    function add_delivery(subtotal: Pence) -> Pence {
        return subtotal
    }
}
```

```text output
error: `helpers` is too generic for a library
 --> main.jadpo:5:9
  |
5 | library helpers {
  |         ^^^^^^^ says nothing about what it holds
  reason: Domains and libraries are named after what they hold, because the name appears in every call; `common`, `core`, `helpers`, `misc` and `utils` say nothing.
  next: Rename the library after what its functions do (agent)
  rule: naming.generic_name
  help: docs/language/01-project-layout-and-modules.html#naming-generic-name

check failed: 1 error, 1 file
```

### `semantic.standard_namespace_reserved`

```jadpo check project=e-std-namespace expect=semantic.standard_namespace_reserved
type Count = Int {
    min: 0
}

function temporal(x: Count) -> Count {
    return x
}
```

```text output
error: `temporal` is a standard-library namespace
 --> main.jadpo:5:10
  |
5 | function temporal(x: Count) -> Count {
  |          ^^^^^^^^ reserved
  reason: Standard-library namespaces can't name a declaration, domain, library or variable, so `temporal.` always means the standard library.
  next: Choose another name than `temporal` (agent)
  rule: semantic.standard_namespace_reserved
  help: docs/language/01-project-layout-and-modules.html#semantic-standard-namespace-reserved

check failed: 1 error, 1 file
```

### `semantic.unknown_name`

```jadpo check project=e-unknown-name expect=semantic.unknown_name
type Pair = Object {
    first: Missing
}
```

```text output
error: `Missing` isn't declared
 --> main.jadpo:2:12
  |
2 |     first: Missing
  |            ^^^^^^^ not declared
  reason: Nothing in this project or the standard library is called `Missing`.
  next: Declare `Missing`, or use an existing name (agent)
  rule: semantic.unknown_name
  help: docs/language/01-project-layout-and-modules.html#semantic-unknown-name

check failed: 1 error, 1 file
```

### `semantic.unknown_callee`

```jadpo check project=e-unknown-callee expect=semantic.unknown_callee
type Count = Int {
    min: 0
}

function f(x: Count) -> Count {
    return lookup(x)
}
```

```text output
error: `lookup` isn't a function, action or query
 --> main.jadpo:6:12
  |
6 |     return lookup(x)
  |            ^^^^^^ not declared
  reason: Nothing in this project called `lookup` can be called.
  next: Declare `lookup`, or call an existing function (agent)
  rule: semantic.unknown_callee
  help: docs/language/01-project-layout-and-modules.html#semantic-unknown-callee

check failed: 1 error, 1 file
```

### `semantic.standard_operation_qualification`

```jadpo check project=e-std-qualify expect=semantic.standard_operation_qualification
type DueAt = Instant {}

type HomeZone = Zone {}

type DueTime = Time {}

function local_due(at: DueAt, zone: HomeZone) -> DueTime {
    var local: Time = in_zone(at, zone)
    return DueTime(local)
}
```

```text output
error: `in_zone` is a standard-library operation
 --> main.jadpo:8:23
  |
8 |     var local: Time = in_zone(at, zone)
  |                       ^^^^^^^ needs its namespace
  reason: The standard library is always called through its namespace, so the dot says who owns the operation.
  next: Call `temporal.in_zone(...)` (agent)
  rule: semantic.standard_operation_qualification
  help: docs/language/01-project-layout-and-modules.html#semantic-standard-operation-qualification

check failed: 1 error, 1 file
```

### `semantic.not_callable`

```jadpo check project=e-not-callable expect=semantic.not_callable
type OutOfStock = Failure {
    kind: FailureKind.conflict
}

action reserve_item() fails OutOfStock -> Unit {
    reject OutOfStock()
}
```

```text output
error: `OutOfStock` is a failure and can't be called
 --> main.jadpo:6:12
  |
6 |     reject OutOfStock()
  |            ^^^^^^^^^^ not callable
  reason: A failure is raised with `reject OutOfStock`, with its fields in a block when it has any.
  next: Write `OutOfStock` without `()` (agent)
  rule: semantic.not_callable
  help: docs/language/01-project-layout-and-modules.html#semantic-not-callable

check failed: 1 error, 1 file
```

### `semantic.wrong_name_kind`

```jadpo check project=e-wrong-kind expect=semantic.wrong_name_kind
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

```text output
error: `g` is a function, not a type
 --> main.jadpo:10:12
   |
10 |     first: g
   |            ^ expected a type
  reason: Here Jadpo expects a type, such as `Count` or `List<Count>`.
  next: Name a type here (agent)
  rule: semantic.wrong_name_kind
  help: docs/language/01-project-layout-and-modules.html#semantic-wrong-name-kind

check failed: 1 error, 1 file
```

### `visibility.private_cross_domain`

```jadpo check project=e-private file=app.jadpo hidden
application {}
```

```jadpo check project=e-private file=shared/types/quantity.jadpo hidden
type Quantity = Int {
    min: 1
}
```

```jadpo check project=e-private file=domains/inventory/actions/reserve_stock.jadpo
action reserve_stock(quantity: Quantity) -> Unit {}
```

```jadpo check project=e-private file=domains/ordering/actions/checkout.jadpo expect=visibility.private_cross_domain
action checkout(quantity: Quantity) -> Unit {
    inventory.reserve_stock(quantity)
}
```

```text output
error: `reserve_stock` is private to domain `inventory`
 --> domains/ordering/actions/checkout.jadpo:2:15
  |
2 |     inventory.reserve_stock(quantity)
  |               ^^^^^^^^^^^^^ private
  reason: Another domain may use only what a domain marks `public`; `reserve_stock` is an action in `domains/inventory/actions/reserve_stock.jadpo`.
  next: Mark `reserve_stock` `public` if `inventory` should offer it to other domains (human)
  rule: visibility.private_cross_domain
  help: docs/language/01-project-layout-and-modules.html#visibility-private-cross-domain

check failed: 1 error, 4 files
```

### `visibility.public_not_allowed`

A domain offers other domains its types, entities, queries, libraries and
actions; nothing else is `public`:

```jadpo check project=e-public file=app.jadpo hidden
application {}
```

```jadpo check project=e-public file=domains/planning/routes/todos.jadpo expect=visibility.public_not_allowed
public routes Todos {
    prefix: "/todos"
}
```

```text output
error: A route group can't be `public`
 --> domains/planning/routes/todos.jadpo:1:1
  |
1 | public routes Todos {
  | ^^^^^^ not part of a domain's interface
  reason: A route group is a trigger: nothing calls it, so there is nothing to offer another domain; who reaches a route is decided by the route and its policy.
  next: Remove `public` (agent)
  rule: visibility.public_not_allowed
  help: docs/language/01-project-layout-and-modules.html#visibility-public-not-allowed

check failed: 1 error, 2 files
```

### `semantic.own_domain_qualification`

The dot names the owner of what you call, so code calls its own domain's
actions and queries by their bare names:

```jadpo check project=e-own-domain file=app.jadpo hidden
application {}
```

```jadpo check project=e-own-domain file=domains/planning/actions/snooze_todo.jadpo
action snooze_todo() -> Unit {}
```

```jadpo check project=e-own-domain file=domains/planning/actions/tidy_up.jadpo expect=semantic.own_domain_qualification
action tidy_up() -> Unit {
    planning.snooze_todo()
}
```

```text output
error: `snooze_todo` is in this file's own domain
 --> domains/planning/actions/tidy_up.jadpo:2:5
  |
2 |     planning.snooze_todo()
  |     ^^^^^^^^ own domain
  reason: The dot names the owner of what you call, and code in `planning` owns `snooze_todo` itself, so it calls it by its bare name.
  next: Call `snooze_todo(...)` without `planning.` (agent)
  rule: semantic.own_domain_qualification
  help: docs/language/01-project-layout-and-modules.html#semantic-own-domain-qualification

check failed: 1 error, 3 files
```

### `layout.unknown_location`

```jadpo check project=e-location file=app.jadpo hidden
application {}
```

```jadpo check project=e-location file=domains/planning/helpers/due_dates.jadpo expect=layout.unknown_location
type Days = Int {
    min: 0
}

library due_dates {
    function days_left(days: Days) -> Days {
        return days
    }
}
```

```text output
error: `helpers/` isn't a folder a domain has
 --> domains/planning/helpers/due_dates.jadpo
  reason: A domain holds only `entities/`, `types/`, `failures/`, `actions/`, `queries/`, `libraries/`, `routes/`, `schedules/`, `events/`, `subscribers/`, `emails/` and `fixtures/`, each named after what its files declare.
  next: Move the file to `domains/planning/libraries/due_dates.jadpo` (agent)
  rule: layout.unknown_location
  help: docs/language/01-project-layout-and-modules.html#layout-unknown-location

check failed: 1 error, 2 files
```

### `layout.wrong_folder`

```jadpo check project=e-folder file=app.jadpo hidden
application {}
```

```jadpo check project=e-folder file=domains/planning/types/complete_todo.jadpo expect=layout.wrong_folder
action complete_todo() -> Unit {}
```

```text output
error: Action `complete_todo` is in `types/`
 --> domains/planning/types/complete_todo.jadpo:1:8
  |
1 | action complete_todo() -> Unit {}
  |        ^^^^^^^^^^^^^ belongs in `actions/`
  reason: `types/` holds types shared by several files in the domain; every action has its own file in `actions/`.
  next: Move `complete_todo` to `domains/planning/actions/complete_todo.jadpo` (agent)
  rule: layout.wrong_folder
  help: docs/language/01-project-layout-and-modules.html#layout-wrong-folder

check failed: 1 error, 2 files
```

### `layout.file_name_mismatch`

```jadpo check project=e-file-name file=app.jadpo hidden
application {}
```

```jadpo check project=e-file-name file=domains/planning/actions/snooze_todo.jadpo hidden
action snooze_todo(due: DueDate) -> Unit {}
```

```jadpo check project=e-file-name file=domains/planning/types/due.jadpo expect=layout.file_name_mismatch
type DueDate = CalendarDate {}
```

```text output
error: `DueDate` is in `due.jadpo`
 --> domains/planning/types/due.jadpo:1:6
  |
1 | type DueDate = CalendarDate {}
  |      ^^^^^^^ file name should be `due_date.jadpo`
  reason: A file is named after the declaration it holds, so a search for a name finds its file.
  next: Rename `due.jadpo` to `due_date.jadpo` (agent)
  rule: layout.file_name_mismatch
  help: docs/language/01-project-layout-and-modules.html#layout-file-name-mismatch

check failed: 1 error, 3 files
```

### `layout.promotion_required`

```jadpo check project=e-promotion file=app.jadpo hidden
application {}
```

```jadpo check project=e-promotion file=domains/planning/actions/postpone_all.jadpo
action postpone_all(days: SnoozeDays) -> Unit {}
```

```jadpo check project=e-promotion file=domains/planning/actions/snooze_todo.jadpo expect=layout.promotion_required
type SnoozeDays = Int {
    min: 1
    max: 30
}

action snooze_todo(days: SnoozeDays) -> Unit {}
```

```text output
error: `SnoozeDays` is used beyond this file
 --> domains/planning/actions/snooze_todo.jadpo:1:6
  |
1 | type SnoozeDays = Int {
  |      ^^^^^^^^^^ used by another file
  reason: A type used by one file lives in it; once another file uses it, it moves to `domains/planning/types/`. `domains/planning/actions/postpone_all.jadpo` uses it too.
  next: Move `SnoozeDays` to `domains/planning/types/snooze_days.jadpo` (agent)
  rule: layout.promotion_required
  help: docs/language/01-project-layout-and-modules.html#layout-promotion-required

check failed: 1 error, 3 files
```

### `layout.unused_declaration`

```jadpo check project=e-unused file=app.jadpo hidden
application {}
```

```jadpo check project=e-unused file=domains/planning/types/due_window.jadpo expect=layout.unused_declaration
type DueWindow = Int {
    min: 1
}
```

```text output
error: `DueWindow` isn't used
 --> domains/planning/types/due_window.jadpo:1:6
  |
1 | type DueWindow = Int {
  |      ^^^^^^^^^ nothing uses this
  reason: A shared type exists to be used by several files; one that nothing uses is dead code. Tests don't count as users.
  next: Remove `DueWindow` (agent)
  rule: layout.unused_declaration
  help: docs/language/01-project-layout-and-modules.html#layout-unused-declaration

check failed: 1 error, 2 files
```

### `layout.test_without_subject`

```jadpo check project=e-test-subject file=app.jadpo hidden
application {}
```

```jadpo check project=e-test-subject file=domains/planning/actions/archive_todo.test.jadpo expect=layout.test_without_subject
test "archiving a todo hides it" {}
```

```text output
error: `archive_todo.test.jadpo` has no `archive_todo.jadpo` beside it
 --> domains/planning/actions/archive_todo.test.jadpo
  reason: A test file sits beside the file it tests, named after it; scenarios across domains go in the root `tests/` folder. `actions/` has no `archive_todo.jadpo`.
  next: Rename it after the file it tests, or move it to `tests/` (agent)
  rule: layout.test_without_subject
  help: docs/language/01-project-layout-and-modules.html#layout-test-without-subject

check failed: 1 error, 2 files
```

### `layout.route_prefix_mismatch`

```jadpo check project=e-prefix file=app.jadpo hidden
application {}
```

```jadpo check project=e-prefix file=domains/planning/routes/todos.jadpo expect=layout.route_prefix_mismatch
routes Todos {
    prefix: /tasks
}
```

```text output
error: Prefix `/tasks` doesn't match `todos.jadpo`
 --> domains/planning/routes/todos.jadpo:2:13
  |
2 |     prefix: /tasks
  |             ^^^^^^ gives `tasks.jadpo`
  reason: A route group's file is named after its prefix's fixed segments joined with `_`, so a URL leads to one file: `/tasks` lives in `tasks.jadpo`.
  next: Change the prefix to match `todos.jadpo`, or rename the file `tasks.jadpo` and the group `Tasks` (agent)
  rule: layout.route_prefix_mismatch
  help: docs/language/01-project-layout-and-modules.html#layout-route-prefix-mismatch

check failed: 1 error, 2 files
```

### `layout.call_direction`

```jadpo check project=e-direction file=app.jadpo hidden
application {}
```

```jadpo check project=e-direction file=domains/ordering/types/order_total.jadpo hidden
type OrderTotal = Int {
    min: 0
}
```

```jadpo check project=e-direction file=domains/ordering/actions/apply_discount.jadpo hidden
action apply_discount(discount: Discount) -> Unit {}
```

```jadpo check project=e-direction file=shared/types/discount.jadpo expect=layout.call_direction
type Discount = Object {
    applies_to: OrderTotal
}
```

```text output
error: Shared code uses `OrderTotal` from domain `ordering`
 --> shared/types/discount.jadpo:2:17
  |
2 |     applies_to: OrderTotal
  |                 ^^^^^^^^^^ domain code
  reason: `shared/` holds code every domain may use, so it can't depend on any one domain; `OrderTotal` is a type in `domains/ordering/types/order_total.jadpo`.
  next: Move the shared declaration into `ordering`, or stop it depending on `OrderTotal` (human)
  rule: layout.call_direction
  help: docs/language/01-project-layout-and-modules.html#layout-call-direction

check failed: 1 error, 4 files
```

### `library.similar_exists`

A warning, so `check` still passes. A function is like a library function
when its parameter types, result, `fails` list and body are the same once
each parameter and variable is renamed by its position: a copy, however it
is formatted or named. Of two alike library functions, the later one in
file order is reported.

```jadpo check project=e-similar expect=library.similar_exists
type Pence = Int {
    min: 0
}

library rounding {
    /// The amount, capped at a thousand pence.
    function capped(amount: Pence) -> Pence {
        if (amount > Pence(1000)) {
            return Pence(1000)
        }
        return amount
    }
}

function cap_total(total: Pence) -> Pence {
    if (total > Pence(1000)) {
        return Pence(1000)
    }
    return total
}
```

```text output
warning: A similar library function exists: `rounding.capped`
 --> main.jadpo:15:10
   |
15 | function cap_total(total: Pence) -> Pence {
   |          ^^^^^^^^^ same as `rounding.capped`
  reason: `cap_total` takes the same parameters, gives the same result and has the same body as `rounding.capped` in `main.jadpo`, apart from the names of its variables.
  next: Call `rounding.capped` instead, and remove `cap_total` (agent)
  rule: library.similar_exists
  help: docs/language/01-project-layout-and-modules.html#library-similar-exists

checked: 1 warning, 1 file
```

### `syntax.expected_declaration`

```jadpo check project=e-expected-declaration expect=syntax.expected_declaration
Count = Int
```

```text output
error: `Count` doesn't start a declaration
 --> main.jadpo:1:1
  |
1 | Count = Int
  | ^^^^^ expected a declaration
  reason: A file holds declarations, such as `type`, `function`, `action`, `query` and `entity`, each starting a line.
  next: Start the declaration with `type`: `type Count = ...` (agent)
  rule: syntax.expected_declaration
  help: docs/language/01-project-layout-and-modules.html#syntax-expected-declaration

check failed: 1 error, 1 file
```

### `syntax.expected_name`

```jadpo check project=e-expected-name expect=syntax.expected_name
type Pair = Object {
    : Int
}
```

```text output
error: This member has no name
 --> main.jadpo:2:5
  |
2 |     : Int
  |     ^ expected a name before `:`
  reason: Every member inside braces is `name: value`.
  next: Add the member's name before `:` (agent)
  rule: syntax.expected_name
  help: docs/language/01-project-layout-and-modules.html#syntax-expected-name

check failed: 1 error, 1 file
```

### `syntax.unexpected_token`

```jadpo check project=e-unexpected-token expect=syntax.unexpected_token
type Count = Int {
    min: 0
}

function f(x: Count -> Count {
    return x
}
```

```text output
error: This parameter list isn't closed
 --> main.jadpo:5:21
  |
5 | function f(x: Count -> Count {
  |                     ^^ expected `)` before `->`
  |           - opened here
  reason: The parameters end with `)`, before `fails`, `->` or the body.
  next: Add `)` after `Count` (agent)
  rule: syntax.unexpected_token
  help: docs/language/01-project-layout-and-modules.html#syntax-unexpected-token

check failed: 1 error, 1 file
```

### `syntax.unexpected_character`

```jadpo check project=e-unexpected-character expect=syntax.unexpected_character
type Count = Int {
    min: 0 $
}
```

```text output
error: `$` isn't part of Jadpo
 --> main.jadpo:2:12
  |
2 |     min: 0 $
  |            ^ not Jadpo
  reason: Jadpo source is names, literals, brackets and operators such as `+` and `==`; this character has no meaning.
  next: Remove `$` (agent)
  rule: syntax.unexpected_character
  help: docs/language/01-project-layout-and-modules.html#syntax-unexpected-character

check failed: 1 error, 1 file
```

### `syntax.unterminated_string`

```jadpo check project=e-unterminated expect=syntax.unterminated_string
type Label = Text {
    pattern: "abc
}
```

```text output
error: This string isn't closed
 --> main.jadpo:2:14
  |
2 |     pattern: "abc
  |              ^^^^ no closing `"` on this line
  reason: A string starts and ends with `"` on the same line.
  next: Add `"` where the string ends (agent)
  rule: syntax.unterminated_string
  help: docs/language/01-project-layout-and-modules.html#syntax-unterminated-string

check failed: 1 error, 1 file
```

### `syntax.invalid_escape`

```jadpo check project=e-escape expect=syntax.invalid_escape
type Label = Text {
    pattern: "a\qb"
}
```

```text output
error: `\q` isn't an escape Jadpo knows
 --> main.jadpo:2:16
  |
2 |     pattern: "a\qb"
  |                ^^ unknown escape
  reason: Inside a string, `\` starts one of `\"`, `\\`, `\n`, `\r` or `\t`.
  next: Write `\\q` for a backslash followed by `q` (agent)
  rule: syntax.invalid_escape
  help: docs/language/01-project-layout-and-modules.html#syntax-invalid-escape

check failed: 1 error, 1 file
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
  (`temporal.in_zone(x)`), and the generated entity operations go through
  the entity (`Todo.get(id)`). This is deliberate (the qualifier shows who owns the
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
jadpo.lock                 pinned provider packages from the registry
providers/                 provider packages imported or written for this app
domains/
  planning/
    entities/
      todo.jadpo           Todo: fields, unique rules, lifecycle, policy
      todo.test.jadpo      its tests, beside it
      user.jadpo
      service.jadpo
      service_credential.jadpo
    types/
      todo_view.jadpo      shared within the domain: used by more than one file
    queries/
      todos_for_owner.jadpo     every query, one per file
    actions/
      complete_todo.jadpo       every action, one per file
      send_due_reminders.jadpo
    failures/
      todo_not_found.jadpo      every failure, one per file
    routes/
      todos.jadpo          route group Todos, prefix /todos: every /todos route
      todos.test.jadpo
      users.jadpo
    schedules/
      overdue_reminders.jadpo
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
    subscribers/  payment_succeeded.jadpo    on Stripe.events.PaymentIntentSucceeded
  notifications/
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
| `entities/` | One entity per file: its fields, unique rules, lifecycle, policy and record caches; the generated operations (`Todo.get`, create, update, delete) belong to it | Queries, actions and failures each have their own folder (areas 4 and 5, 2026-10-12) |
| `types/` | Types used by more than one file in the domain | Single-use ones live beside their user |
| `failures/` | One failure per file, every failure the domain raises | Added 2026-10-12 (area 4); failures raised by two or more domains go in `shared/failures/` |
| `actions/` | Every action, one per file | Called by name (`complete_todo(todo_id)`); called by triggers or, if `public`, by other domains (decided 2026-10-12) |
| `queries/` | Every query, one per file, whatever it reads; a query's cache sits in its file | Decided in area 5 (2026-10-12) |
| `libraries/` | Named groups of helper functions, e.g. `library rounding` | Called like the standard library: `rounding.half_even(x)` |
| `routes/` | Named route groups with a prefix the compiler checks | HTTP trigger: the URL is written in the source |
| `schedules/` | Time triggers: when to run which action | Replaces "jobs" |
| `events/` | Event definitions | |
| `subscribers/` | Event triggers, including long-running coordination | Replaces durable "workflows" |
| (none) | Third-party APIs are provider packages: connections in `app.jadpo`, packages in root `providers/` or pinned in `jadpo.lock` | Amended 2026-10-13 by area 11; the `integrations/` folder and `integration` keyword are gone |
| `emails/` | Email types and their templates, one email per name (`welcome_email.jadpo`, `welcome_email.html`, `welcome_email.txt`) | Added 2026-10-13 (Richard, area 16) |
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
- One exception to file-name uniqueness: a type declared in the same file
  as an action, query or function may fold to that callable's name, as an
  input does (`type CreateTodo = Input` beside `action create_todo` in
  `actions/create_todo.jadpo`), because only one file carries the name. If
  the type is promoted to its own file, the rule applies again.
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
  can be public; `public` on anything else (a function, a route group, a
  schedule, a subscriber, a test, a seed or a fixture) is
  `visibility.public_not_allowed`, and an own-domain action or query called
  as `planning.snooze_todo()` from inside `planning` is
  `semantic.own_domain_qualification`. A public action is the domain's write interface: checkout in
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
  Your own domain's actions and queries are called bare
  (`complete_todo(todo_id)`). Searching for `inventory.` finds calls into that
  domain, but not generated entity operations such as `Order.get`, which are
  named by their entity.
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
public action reserve_for_cart(cart_id: Cart.id) fails OutOfStock -> Reservation { ... }

// domains/ordering/actions/checkout.jadpo
action checkout(cart_id: Cart.id) fails OutOfStock, CartEmpty -> Order {
    var reservation: Reservation = attempt inventory.reserve_for_cart(cart_id)
    var order: Order = attempt place_order(cart_id, reservation)
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
    on: OrderPlaced
    run: send_order_confirmation_email(event)
}

// app.jadpo: a connection to a provider package (area 11)
providers: {
    stripe: {
        package: Stripe.v2024_06_20
        credential: config.stripe_secret_key
        domains: [payments]
    }
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

Built on 2026-10-13 (chunk 1.3): `jadpo check` reports all of these except
`layout.asset_unreferenced` and `library.similar_exists`, which followed
with the graph's digests in chunk 1.7; each has its
example and real output under [Errors](#errors). A folder or file checked
without an `app.jadpo` above it is treated as loose files, one implicit
domain with the layout rules off, as a single file is. Resolution covers
names in signatures, type blocks and calls; lower-case names used as values
are checked with the logic of bodies (area 3).

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
   *Decided 2026-10-12 (see Decisions):* keep `domains/`. Every root entry is then fixed by the
   language and projects differ only inside it, so nobody has to tell a domain
   from a reserved folder.
2. **Should the dot always name the owner of what you call?** This reverses
   the current rule that your own functions are called without a prefix.
   *Decided 2026-10-12 (see Decisions):* yes. Libraries read exactly like the standard library,
   imports disappear (missing imports are among the most common reasons LLM
   code fails to compile), and every call says where its function lives.
   Exact syntax is confirmed in area 3.
3. **Should creating a domain need human approval?** An agent that hits a
   cross-domain error might add a domain or reshuffle code rather than ask.
   *Decided 2026-10-12 (see Decisions):* yes: a new or renamed domain appears in the approval
   review like a policy change, because it changes who owns what.
4. **How is policy protected from agent edits?** Entity policy lives in the
   entity file. *Decided 2026-10-12 (see Decisions):* protect it through the approval check,
   which already fingerprints policy, so any policy change needs human
   approval wherever it lives.
5. **Multiple deployables (API, worker, back-office) as declarations in one
   project, not separate projects?** *Decided 2026-10-12 (see Decisions):* declarations. Each
   deployable selects its routes, schedules and subscribers and is compiled
   from what it reaches.
6. **Large entity files.** An order entity in a real shop could reach 800
   lines. *Decided 2026-10-12 (see Decisions):* no companion files for now; the formatter keeps a
   fixed section order. Revisit with measured sizes from the order/payment app.
7. **Reserved words.** *Decided 2026-10-12 (see Decisions):* build the chosen policy:
   control-flow and declaration words are fully reserved, with a short explicit
   list of words such as `input` and `value` that can be used as names. Built
   in chunk 1.9; the exact lists are under [Naming](#naming).
8. **Documentation comments.** *Decided 2026-10-12 (see Decisions):* add `///` comments that
   attach to the following declaration and flow into generated documentation,
   the library catalogue and OpenAPI descriptions.
9. **Formatter owns layout within a file.** *Decided 2026-10-12 (see Decisions):* the formatter
   decides blank lines (one between declarations) and section order within a
   file, so equivalent code always looks identical.
10. **Root `api/` and `acceptances/` folders** (carried from area 7). Area 7
   keeps each API's record in `api/` and a person's acceptance of a breaking
   change in `acceptances/`, but the decided root list allows only the entries
   above. *Recommendation:* add both folders to the fixed root. *Decided 2026-10-13 (area 18):* yes; the root also gains `providers/` and `jadpo.lock` (area 11).

### Decisions

Decided by Richard on 2026-10-12. Nothing here is built yet; the rework is listed in the roadmap.

| Topic | Decision | Date |
|---|---|---|
| Shape | Same at every size: fixed root, all code in one level of domains, the same fixed folders in every domain, no subfolders | 2026-10-12 |
| Folder names | Each folder is named for what its files contain, usually the declaration keyword in plural; `values/` becomes `types/`, `workflows/` becomes `actions/`; `failures/` holds `type X = Failure` declarations (area 4) | 2026-10-12 |
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
| Names folding together | A type may fold to the name of the callable declared in the same file (an input named after its action); everywhere else `naming.folded_collision` applies | 2026-10-13 |
| Keywords match folders | `schedule` (folder `schedules/`), `integration` (`integrations/`) and `library` (`libraries/`) replace `job` and `service` (Amended 2026-10-13 by area 11: `integration` and `integrations/` are gone; external services are provider packages) | 2026-10-12 |
| Unnamed singletons | Declarations that exist once per project carry no name: `application { }`, `config { }`, `principal { }`, `locales { }` | 2026-10-12 |
| Query and action placement | Every query lives in its domain's `queries/` folder and every action in `actions/`, one per file; written ones are called by name; entity files hold the data definition and the generated operations (area 5, Richard) | 2026-10-12 |
| Root `api/` and `acceptances/` folders | Added to the fixed root, with `providers/` and `jadpo.lock` (areas 11 and 18) | 2026-10-13 |
| Emails folder | Each domain may have `emails/`, holding email types and their templates (from area 16) | 2026-10-13 |
| Flags and monitoring files | `flags.jadpo` joins the fixed root; each domain may hold one `monitoring.jadpo` directly in its folder (Richard, from the architecture plan) | 2026-10-13 |
