---
title: Project layout and modules
area: 1
status: decided
checked-with: this checkout's jadpo on every gate run (archived examples: v0 3ce094d0)
---

# Project layout and modules

How a Jadpo project is laid out on disk, how one file sees declarations from
another, and the naming rules every name follows.

## Project layout

A project is the folder holding `app.jadpo`. The root holds a fixed set of
files and folders, and all application code lives in business areas
(domains) under `domains/`, one level deep. Every domain holds the same
fixed folders, named after the declarations they hold (`entities/`,
`actions/`, `queries/` and so on), with one primary declaration per file,
named after it, first in the file, and nothing else in the file that it
doesn't use. A file's path follows from its domain and what it declares.
[Decided layout](#decided-layout) gives the full rules, which the compiler
builds (chunks 1.3 and 1.9).

A single file, or a folder with no `app.jadpo` above it, is checked as loose
files: one implicit domain with the layout rules off.

## Source files

File order and declaration order never matter: a declaration can refer to
one defined later or in another file. Names are unique across the whole
project, so a second `Count` in another file is an error (see
`semantic.duplicate_declaration` below). The one exception, decided
2026-10-16 and built in chunk 5.3d: a private function or type needs
a name unique only within its domain (see [Files and names](#files-and-names)),
as does a private constant (area 3, decided 2026-10-16, built in chunk
5.3e).

## Domains instead of modules

There are no modules. A domain owns everything in it and is the visibility
boundary: a declaration is private to its domain unless marked `public`,
and a call into another domain names it (`inventory.reserve_stock(...)`).
Each file names where its other names come from on `use` lines
(decided 2026-10-14, built in chunk 3.5). See
[Ownership and calls replace modules](#ownership-and-calls-replace-modules)
and [Imports](#imports).

## Naming

Every name has one spelling, decided by what kind of thing it names.

| Kind of name | Form | Examples |
|---|---|---|
| Types and declared contracts (entities, failures, services, config, events) | `UpperCamelCase` | `Todo`, `TodoNotFound`, `ReminderMail` |
| Values, fields, parameters, constants, functions, actions, queries, jobs, variants | `lower_snake_case` | `due_at`, `order_total`, `overdue_reminders` |
| Standard library | lowercase namespace, always qualified | `temporal.in_zone(...)` |
| Keywords | lowercase | `function`, `return`, `match` |

Identifiers are ASCII letters, digits and underscores, and cannot start with a
digit. The compiler checks the case and suggests the right spelling. A name
must convert to its file name and back unchanged, so consecutive capitals
are not allowed: `VatRate`, not `VATRate`.

Control-flow and declaration words are reserved (question 7): none of them
names a declaration, field, parameter, variable or variant, and using one is
`syntax.expected_name`. They are `action`, `and`, `assert`, `attempt`,
`else`, `emit`, `entity`, `fails`, `false`, `fixture`, `for`, `function`,
`if`, `in`, `library`, `match`, `mut`, `none`, `not`, `or`, `package`,
`propagate`, `provide`, `public`, `query`, `reject`, `resource`, `return`,
`routes`, `schedule`, `seed`, `subscriber`, `test`, `true`, `type`, `use`
and `var`; `constant` joins them with constants (area 3, decided
2026-10-16, built in chunk 5.3e). A setting the language defines may still use one as its key
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

Line comments start with `//`. They carry no meaning and are kept for tooling.
`#` and `/* */` comments from other languages are `syntax.comment_style`,
whose edit writes them with `//` (or `///` for `/** */`).
A `///` comment attaches to the next declaration and documents it: editors,
generated documentation and the library catalogue show it.

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
16.

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
  reason: `Count` is already a type in `a.jadpo`, declared differently, so these are two things with one name.
  next: Rename one of them, here and where it's used (agent)
  in use:
    Count  a.jadpo
  rule: semantic.duplicate_declaration
  help: docs/language/01-project-layout-and-modules.html#semantic-duplicate-declaration

check failed: 1 error, 2 files
```

The error names both declarations and their files, and says which of two
cases it is (decided 2026-10-16, Richard; built in chunk 5.3d). If the two
are written the same way, ignoring layout, comments and `public`, it is a
copy: remove one and use the other, or move it to `shared/` if two domains
need it. If they differ, they are two things with one name: rename one. It
never invents a name. It lists the names in use that contain the same
words, with their files, so the agent renaming one sees what is taken and
what the convention is. A private function or type needs a name unique
only within its domain, so here `shipping`'s own `Address` is no clash,
while a second one in `billing` is:

```jadpo check project=e-duplicate-domain file=app.jadpo hidden
application {}
```

```jadpo check project=e-duplicate-domain file=domains/billing/actions/send_invoice.jadpo hidden
action send_invoice(address: Address) -> Unit {}

type Address = Object {
    street: Text
}
```

```jadpo check project=e-duplicate-domain file=domains/shipping/actions/ship_parcel.jadpo hidden
action ship_parcel(address: Address) -> Unit {}

type Address = Object {
    postcode: Text
}
```

```jadpo check project=e-duplicate-domain file=domains/shipping/actions/print_label.jadpo hidden
action print_label(address: ShippingAddress) -> Unit {}

type ShippingAddress = Object {
    line_one: Text
}
```

```jadpo check project=e-duplicate-domain file=domains/billing/actions/send_reminder.jadpo expect=semantic.duplicate_declaration
action send_reminder(address: Address) -> Unit {}

type Address = Object {
    street: Text
    town: Text
}
```

```text output
error: `Address` is declared twice in `billing`
 --> domains/billing/actions/send_reminder.jadpo:3:6
  |
3 | type Address = Object {
  |      ^^^^^^^ declared again here
  reason: `Address` is already a type in `domains/billing/actions/send_invoice.jadpo`, declared differently, so these are two things with one name.
  next: Rename one of them, here and where it's used (agent)
  in use:
    Address          domains/billing/actions/send_invoice.jadpo
    Address          domains/shipping/actions/ship_parcel.jadpo
    ShippingAddress  domains/shipping/actions/print_label.jadpo
  rule: semantic.duplicate_declaration
  help: docs/language/01-project-layout-and-modules.html#semantic-duplicate-declaration

check failed: 1 error, 5 files
```

Built with these readings, which the decision left open: the next step
says "Rename one of them" without naming `jadpo rename`, which comes with
the command (area 14, question 5); a name contains another's words when
its words, split as the file name splits them, include them in order
(`ShippingAddress` and `format_address` contain `Address`'s); the list
gives each file's path from the project root, as every other error does,
and shows at most twelve names, the twelfth line counting the rest. A name
declared twice in one scope is this error alone: no import or promotion is
asked for either copy.

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

Two names that become the same file name are an error, except a type beside
the action, query or function in its own file (`CreateTodo` in
`create_todo.jadpo`):

```jadpo check project=e-folded file=due.jadpo
type DueDate = CalendarDate {}
```

```jadpo check project=e-folded file=main.jadpo expect=naming.folded_collision
function due_date(due: DueDate) -> DueDate {
    return due
}
```

```text output
error: `due_date` and `DueDate` share a file name
 --> main.jadpo:1:10
  |
1 | function due_date(due: DueDate) -> DueDate {
  |          ^^^^^^^^ also `due_date.jadpo`
  reason: Both names become `due_date.jadpo`, so a search for one finds the other; `DueDate` is a type in `due.jadpo`.
  next: Rename one of them so their file names differ (agent)
  rule: naming.folded_collision
  help: docs/language/01-project-layout-and-modules.html#naming-folded-collision

check failed: 1 error, 2 files
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

type DueTime = ZonedTime {}

function local_due(at: DueAt, zone: HomeZone) -> DueTime {
    var local: ZonedTime = in_zone(at, zone)
    return DueTime(local)
}
```

```text output
error: `in_zone` is a standard-library operation
 --> main.jadpo:8:28
  |
8 |     var local: ZonedTime = in_zone(at, zone)
  |                            ^^^^^^^ needs its namespace
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

A fixture builds test data, so only tests, other fixtures and seeds call it
(area 13); an action can't, even when the fixture is `public`:

```jadpo check project=e-fixture-from-action file=app.jadpo hidden
application {}
```

```jadpo check project=e-fixture-from-action file=domains/planning/types/todo_count.jadpo hidden
type TodoCount = Int {
    min: 0
}
```

```jadpo check project=e-fixture-from-action file=domains/planning/fixtures/two_open_todos.jadpo hidden
use planning/types { TodoCount }

public fixture two_open_todos() -> TodoCount {
    return 2
}
```

```jadpo check project=e-fixture-from-action file=tests/open_todos.test.jadpo hidden
use planning

test "planning starts with two open todos" {
    assert planning.two_open_todos() == 2
}
```

```jadpo check project=e-fixture-from-action file=domains/planning/actions/count_todos.jadpo expect=semantic.not_callable
use planning/types { TodoCount }
use planning/fixtures { two_open_todos }

action count_todos() -> TodoCount {
    return two_open_todos()
}
```

```text output
error: An action can't call fixture `two_open_todos`
 --> domains/planning/actions/count_todos.jadpo:5:12
  |
5 |     return two_open_todos()
  |            ^^^^^^^^^^^^^^ test data
  reason: A fixture builds test data: only tests, other fixtures and seeds call it, and it is never compiled into a deployable.
  next: Call the actions `two_open_todos` calls, or move this call into a test, fixture or seed (agent)
  rule: semantic.not_callable
  help: docs/language/01-project-layout-and-modules.html#semantic-not-callable

check failed: 1 error, 5 files
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
use shared/types { Quantity }

action reserve_stock(quantity: Quantity) -> Unit {}
```

```jadpo check project=e-private file=domains/ordering/actions/checkout.jadpo expect=visibility.private_cross_domain
use shared/types { Quantity }
use inventory

action checkout(quantity: Quantity) -> Unit {
    inventory.reserve_stock(quantity)
}
```

```text output
error: `reserve_stock` is private to domain `inventory`
 --> domains/ordering/actions/checkout.jadpo:5:15
  |
5 |     inventory.reserve_stock(quantity)
  |               ^^^^^^^^^^^^^ private
  reason: Another domain may use only what a domain marks `public`; `reserve_stock` is an action in `domains/inventory/actions/reserve_stock.jadpo`.
  next: Mark `reserve_stock` `public` if `inventory` should offer it to other domains (human)
  rule: visibility.private_cross_domain
  help: docs/language/01-project-layout-and-modules.html#visibility-private-cross-domain

check failed: 1 error, 4 files
```

A fixture is private to its domain too: another domain's tests call it only
when it is `public` (area 13):

```jadpo check project=e-private-fixture file=app.jadpo hidden
application {}
```

```jadpo check project=e-private-fixture file=domains/planning/types/todo_count.jadpo hidden
type TodoCount = Int {
    min: 0
}
```

```jadpo check project=e-private-fixture file=domains/planning/fixtures/two_open_todos.jadpo hidden
use planning/types { TodoCount }

fixture two_open_todos() -> TodoCount {
    return 2
}
```

```jadpo check project=e-private-fixture file=domains/reminders/actions/send_reminders.jadpo hidden
action send_reminders() -> Unit {}
```

```jadpo check project=e-private-fixture file=domains/reminders/actions/send_reminders.test.jadpo expect=visibility.private_cross_domain
use planning

test "reminders start from planning's open todos" {
    send_reminders()
    assert planning.two_open_todos() == 2
}
```

```text output
error: `two_open_todos` is private to domain `planning`
 --> domains/reminders/actions/send_reminders.test.jadpo:5:21
  |
5 |     assert planning.two_open_todos() == 2
  |                     ^^^^^^^^^^^^^^ private
  reason: Another domain may use only what a domain marks `public`; `two_open_todos` is a fixture in `domains/planning/fixtures/two_open_todos.jadpo`.
  next: Mark `two_open_todos` `public` if `planning` should offer it to other domains' tests (human)
  rule: visibility.private_cross_domain
  help: docs/language/01-project-layout-and-modules.html#visibility-private-cross-domain

check failed: 1 error, 5 files
```

### `visibility.public_not_allowed`

A domain offers other domains its types, entities, queries, libraries and
actions, and their tests, fixtures and seeds its fixtures; nothing else is
`public`:

```jadpo check project=e-public file=app.jadpo hidden
application {}
```

```jadpo check project=e-public file=domains/planning/routes/todos.jadpo expect=visibility.public_not_allowed
public routes Todos {
    prefix: /todos
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

### `visibility.public_unused`

`public` offers a declaration to the rest of the project, so it marks only
what something outside the domain uses: another domain's code or tests, a
root `tests/` scenario, or the signature of a public declaration one of them
uses. The domain's own tests don't count, and a package's public members are
its interface to other projects, so they are exempt (area 11):

```jadpo check project=e-public-unused file=app.jadpo hidden
application {}
```

```jadpo check project=e-public-unused file=domains/planning/types/todo_count.jadpo hidden
type TodoCount = Int {
    min: 0
}
```

```jadpo check project=e-public-unused file=domains/planning/actions/count_todos.test.jadpo
test "two todos are open" {
    assert count_todos() == 2
}
```

```jadpo check project=e-public-unused file=domains/planning/actions/count_todos.jadpo expect=visibility.public_unused
use planning/types { TodoCount }

public action count_todos() -> TodoCount {
    return 2
}
```

```text output
error: `count_todos` is `public`, but nothing outside `planning` uses it
 --> domains/planning/actions/count_todos.jadpo:3:1
  |
3 | public action count_todos() -> TodoCount {
  | ^^^^^^ offered to no one
  reason: `public` offers a declaration to other domains, and no other domain's code or tests, no root `tests/` scenario and no public declaration they use names `count_todos`; `planning`'s own tests don't count.
  next: Remove `public` (agent)
  rule: visibility.public_unused
  help: docs/language/01-project-layout-and-modules.html#visibility-public-unused

check failed: 1 error, 4 files
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
  reason: The dot names who owns what you call, and `snooze_todo` belongs to `planning` itself, so code there calls it by its bare name.
  next: Call `snooze_todo(...)` without `planning.` (agent)
  rule: semantic.own_domain_qualification
  help: docs/language/01-project-layout-and-modules.html#semantic-own-domain-qualification

check failed: 1 error, 3 files
```

### `import.missing`

Every name a file uses that it doesn't declare comes in on a `use` line
(see [Imports](#imports)). The edit adds it, and `jadpo fmt` adds every
missing import at once:

```jadpo check project=e-import-missing file=app.jadpo hidden
application {}
```

```jadpo check project=e-import-missing file=shared/types/pence.jadpo hidden
type Pence = Int {
    min: 0
}
```

```jadpo check project=e-import-missing file=domains/bakery/actions/price_loaf.jadpo expect=import.missing
action price_loaf(each: Pence) -> Pence {
    return each
}
```

```text output
error: `Pence` isn't imported
 --> domains/bakery/actions/price_loaf.jadpo:1:25
  |
1 | action price_loaf(each: Pence) -> Pence {
  |                         ^^^^^ not imported
  reason: Every name a file uses that it doesn't declare comes in on a `use` line, so a reader sees where it lives; `Pence` is a type in `shared/types/pence.jadpo`.
  next: Add `use shared/types { Pence }`, or run `jadpo fmt`, which adds every missing import (agent)
  rule: import.missing
  help: docs/language/01-project-layout-and-modules.html#import-missing

check failed: 1 error, 3 files
```

### `import.unknown`

A `use` line names a domain, a folder of a domain or of `shared/`, or a
root file, and only names that are there. A name imported from the wrong
folder gets the edit that moves it:

```jadpo check project=e-import-unknown file=app.jadpo hidden
application {}
```

```jadpo check project=e-import-unknown file=shared/types/pence.jadpo hidden
type Pence = Int {
    min: 0
}
```

```jadpo check project=e-import-unknown file=domains/bakery/actions/price_loaf.jadpo expect=import.unknown
use shared/failures { Pence }

action price_loaf(each: Pence) -> Pence {
    return each
}
```

```text output
error: `Pence` isn't in `shared/failures`
 --> domains/bakery/actions/price_loaf.jadpo:1:23
  |
1 | use shared/failures { Pence }
  |                       ^^^^^ not in this folder
  reason: A `use` line brings in names from the folder it names; `Pence` is a type in `shared/types/pence.jadpo`, so it comes in from `shared/types`.
  next: Import `Pence` from `shared/types` (agent)
  rule: import.unknown
  help: docs/language/01-project-layout-and-modules.html#import-unknown

check failed: 1 error, 3 files
```

### `import.unused`

```jadpo check project=e-import-unused file=app.jadpo hidden
application {}
```

```jadpo check project=e-import-unused file=shared/types/pence.jadpo hidden
type Pence = Int {
    min: 0
}
```

```jadpo check project=e-import-unused file=shared/types/percent.jadpo hidden
type Percent = Int {
    min: 0
    max: 100
}
```

```jadpo check project=e-import-unused file=domains/bakery/actions/discount_loaf.jadpo hidden
use shared/types { Percent }

action discount_loaf(reduction: Percent) -> Unit {}
```

```jadpo check project=e-import-unused file=domains/bakery/actions/price_loaf.jadpo expect=import.unused
use shared/types { Pence, Percent }

action price_loaf(each: Pence) -> Pence {
    return each
}
```

```text output
error: `Percent` is imported but not used
 --> domains/bakery/actions/price_loaf.jadpo:1:27
  |
1 | use shared/types { Pence, Percent }
  |                           ^^^^^^^ not used
  reason: A `use` line lists only the names its file uses, so each says where it comes from; nothing here uses `Percent`.
  next: Remove `Percent` from this line (agent)
  rule: import.unused
  help: docs/language/01-project-layout-and-modules.html#import-unused

check failed: 1 error, 5 files
```

### `import.unneeded`

Built-ins, what the file declares, what a test file's subject declares and
the file's own domain are seen without an import:

```jadpo check project=e-import-unneeded file=app.jadpo hidden
application {}
```

```jadpo check project=e-import-unneeded file=shared/types/pence.jadpo hidden
type Pence = Int {
    min: 0
}
```

```jadpo check project=e-import-unneeded file=domains/menu/actions/price_item.jadpo
use shared/types { Pence }

/// A loaf's price before any choices.
function loaf_price() -> Pence {
    return 250
}

action price_item() -> Pence {
    return loaf_price()
}
```

```jadpo check project=e-import-unneeded file=domains/menu/actions/price_item.test.jadpo expect=import.unneeded
use menu/actions { loaf_price }

test "a loaf costs 250 pence" {
    assert loaf_price() == 250
}
```

```text output
error: `loaf_price` is declared in this test's subject
 --> domains/menu/actions/price_item.test.jadpo:1:20
  |
1 | use menu/actions { loaf_price }
  |                    ^^^^^^^^^^ needs no import
  reason: A test file sees everything its subject declares without importing it, and `loaf_price` is in `price_item.jadpo`.
  next: Remove this line (agent)
  rule: import.unneeded
  help: docs/language/01-project-layout-and-modules.html#import-unneeded

check failed: 1 error, 4 files
```

### `import.duplicate`

```jadpo check project=e-import-duplicate file=app.jadpo hidden
application {}
```

```jadpo check project=e-import-duplicate file=shared/types/pence.jadpo hidden
type Pence = Int {
    min: 0
}
```

```jadpo check project=e-import-duplicate file=shared/types/percent.jadpo hidden
type Percent = Int {
    min: 0
    max: 100
}
```

```jadpo check project=e-import-duplicate file=domains/bakery/actions/price_loaf.jadpo expect=import.duplicate
use shared/types { Pence }
use shared/types { Percent }

action price_loaf(each: Pence, reduction: Percent) -> Pence {
    return each
}
```

```text output
error: `shared/types` has two `use` lines
 --> domains/bakery/actions/price_loaf.jadpo:2:5
  |
1 | use shared/types { Pence }
  |     ------------ first line
2 | use shared/types { Percent }
  |     ^^^^^^^^^^^^ a second line
  reason: Each folder has one `use` line, listing every name the file takes from it; `use shared/types` is written twice.
  next: Move its names onto the first `shared/types` line (agent)
  rule: import.duplicate
  help: docs/language/01-project-layout-and-modules.html#import-duplicate

check failed: 1 error, 4 files
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
use planning/types { DueDate }

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
use planning/actions { SnoozeDays }

action postpone_all(days: SnoozeDays) -> Unit {}
```

```jadpo check project=e-promotion file=domains/planning/actions/snooze_todo.jadpo expect=layout.promotion_required
action snooze_todo(days: SnoozeDays) -> Unit {}

type SnoozeDays = Int {
    min: 1
    max: 30
}
```

```text output
error: `SnoozeDays` is used beyond this file
 --> domains/planning/actions/snooze_todo.jadpo:3:6
  |
3 | type SnoozeDays = Int {
  |      ^^^^^^^^^^ used by another file
  reason: A type used by one file lives in it; once another file uses it, it moves to a file of its own in `domains/planning/types/`. `domains/planning/actions/postpone_all.jadpo` uses it too.
  next: Move `SnoozeDays` to `domains/planning/types/snooze_days.jadpo`, or run `jadpo fmt --layout`, which moves it and updates the imports (agent)
  rule: layout.promotion_required
  help: docs/language/01-project-layout-and-modules.html#layout-promotion-required

check failed: 1 error, 3 files
```

Moving to the right folder is not enough: a second declaration in a file
moves to a file of its own once another file uses it, so every name a `use`
line brings in is `<folder>/<name>.jadpo`. The edit makes the move, and
`jadpo fmt --layout` makes it and updates the imports:

```jadpo check project=e-promotion-file file=app.jadpo hidden
application {}
```

```jadpo check project=e-promotion-file file=domains/sales/types/basket.jadpo expect=layout.promotion_required
type Basket = Object {
    code: DiscountCode
}

type DiscountCode = Text {
    max_length: 12
}
```

```text output
error: `DiscountCode` is used beyond this file
 --> domains/sales/types/basket.jadpo:5:6
  |
5 | type DiscountCode = Text {
  |      ^^^^^^^^^^^^ used by another file
  reason: A declaration another file uses has a file of its own named after it, so its `use` line says where it is; `DiscountCode` is in `basket.jadpo`, named after another declaration, and moves to `domains/sales/types/`. `domains/sales/actions/apply_code.jadpo` uses it too.
  next: Move `DiscountCode` to `domains/sales/types/discount_code.jadpo`, or run `jadpo fmt --layout`, which moves it and updates the imports (agent)
  rule: layout.promotion_required
  help: docs/language/01-project-layout-and-modules.html#layout-promotion-required

check failed: 1 error, 3 files
```

```jadpo check project=e-promotion-file file=domains/sales/actions/apply_code.jadpo
use sales/types { Basket, DiscountCode }

action apply_code(basket: Basket, code: DiscountCode) -> Unit {}
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

Beside a file's primary declaration, a supporting type or helper is there
for it: the primary uses it, directly or through another supporting
declaration it uses. One only the file's tests use is dead code too:

```jadpo check project=e-unused-beside file=app.jadpo hidden
application {}
```

```jadpo check project=e-unused-beside file=domains/kitchen/actions/set_oven.jadpo expect=layout.unused_declaration
/// The temperature for a bake on `shelf`: a high shelf runs hot.
action set_oven(shelf: Shelf) -> OvenTemperature {
    if (shelf > 0) {
        return 190
    }
    return 200
}

type Shelf = Int {
    min: -2
    max: 2
}

type OvenTemperature = Int {
    min: 0
    max: 300
}

function top_shelf() -> Shelf {
    return 2
}
```

```text output
error: `top_shelf` isn't used by `set_oven`
 --> domains/kitchen/actions/set_oven.jadpo:19:10
   |
19 | function top_shelf() -> Shelf {
   |          ^^^^^^^^^ `set_oven` doesn't use this
  reason: Everything in a file besides the declaration it is named after is there for that declaration, used by it directly or through one another; anything else is dead code. Tests don't count as users.
  next: Remove `top_shelf` (agent)
  rule: layout.unused_declaration
  help: docs/language/01-project-layout-and-modules.html#layout-unused-declaration

check failed: 1 error, 3 files
```

The file's test calls `top_shelf`, which doesn't count:

```jadpo check project=e-unused-beside file=domains/kitchen/actions/set_oven.test.jadpo
test "the top shelf is turned down" {
    assert set_oven(top_shelf()) == 190
}
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
use shared/types { Discount }

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

A top-level `var` is this error too, and its `next:` points to `constant`,
because at the top of a file a fixed value is a constant (area 3,
[Constants](03-logic.md#constants); decided 2026-10-16 by Richard, built in
chunk 5.3e):

```jadpo check project=e-expected-declaration-var expect=syntax.expected_declaration
type VatRate = Decimal {
    scale: 4
}

var vat_rate: VatRate = 0.2000
```

```text output
error: `var` doesn't start a declaration
 --> main.jadpo:5:1
  |
5 | var vat_rate: VatRate = 0.2000
  | ^^^ expected a declaration
  reason: `var` names a value inside a body; at the top of a file a fixed value is a `constant`.
  next: Write `constant vat_rate: VatRate = 0.2000` (agent)
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

### Habits from other languages

People and agents write constructs from other languages out of habit
(decision register, 2026-10-15). Each has an error of its own that names
the habit and the Jadpo form, with an edit, and parsing carries on as if the
Jadpo form had been written, so a line gives one error. When the Jadpo
form only renames or drops words, the rest of the project is checked too.
The catalogue is `jadpo_syntax::habits`; [area 3](03-logic.md#habits-from-other-languages)
explains how a line shows one error at a time.

### `syntax.comment_style`

A comment starts with `//`, or `///` to document a declaration; `#` and `/* */` are other languages'.

```jadpo check project=e-habit-comment expect=syntax.comment_style
# Counts start at zero.
type Count = Int {
    min: 0
}
```

```text output
error: Jadpo writes comments with `//`, not `#`
 --> main.jadpo:1:1
  |
1 | # Counts start at zero.
  | ^ write `//`
  reason: `#` starts a comment in Python, Ruby, shell scripts and YAML; a Jadpo comment starts with `//`, or `///` to document a declaration.
  next: Use `//` instead of `#` (agent)
  rule: syntax.comment_style
  help: docs/language/01-project-layout-and-modules.html#syntax-comment-style

check failed: 1 error, 1 file
```

### `syntax.import_statement`

A file brings names in with `use <folder> { Names }`; `jadpo fmt` writes those lines.

```jadpo check project=e-habit-import expect=syntax.import_statement
import { Count } from "./shared/types"

function same(count: Count) -> Count {
    return count
}
```

```text output
error: Jadpo writes `use`, not `import`
 --> main.jadpo:1:1
  |
1 | import { Count } from "./shared/types"
  | ^^^^^^ write `use`
  reason: `import … from` brings names into a JavaScript or TypeScript file, `from … import` into a Python module and `import a.b.C` into a Java or Kotlin file; a Jadpo file brings names in from a folder with `use <folder> { Names }`, and `jadpo fmt` writes those lines for you.
  next: Write `use shared/types { Count }` (agent)
  rule: syntax.import_statement
  help: docs/language/01-project-layout-and-modules.html#syntax-import-statement

check failed: 1 error, 1 file
```

### `syntax.export_keyword`

Every declaration is visible across its domain without `export` or `pub`; `public` opens one to other domains.

```jadpo check project=e-habit-export expect=syntax.export_keyword
type Count = Int {
    min: 0
}

export function same(count: Count) -> Count {
    return count
}
```

```text output
error: Jadpo has no `export`
 --> main.jadpo:5:1
  |
5 | export function same(count: Count) -> Count {
  | ^^^^^^ remove `export`
  reason: `export` makes a name visible to other files in JavaScript and TypeScript, and `pub` outside its module in Rust; every Jadpo declaration is visible across its domain, and `public` opens it to other domains.
  next: Remove `export` (agent)
  rule: syntax.export_keyword
  help: docs/language/01-project-layout-and-modules.html#syntax-export-keyword

check failed: 1 error, 1 file
```

## Review

### Status

The compiler builds the decided layout (chunks 1.3 and 1.9), with each
rule's real output under [Errors](#errors).

| Feature | Status |
|---|---|
| Root, domains and their folders | Built (chunk 1.3): a file where the layout has no place for it is `layout.unknown_location`, and a declaration in the wrong folder `layout.wrong_folder`; a folder or file with no `app.jadpo` above it is checked as loose files. |
| Files and names | Built (chunks 1.3 and 1.9): one primary declaration per file, named after it; names unique project-wide; the round-trip and folded-collision rules, with the exception for a type folding to its callable's name (chunk 1.9). |
| Names per domain | Built (chunk 5.3d): private functions and types unique only within their domain, each domain's code reading its own; their identity keys carrying the domain, with promotion to `shared/` writing a registry line; `semantic.duplicate_declaration` saying copy or clash and listing the names in use with the same words. `jadpo rename` refusing a name in use comes with `rename` (area 14). |
| Order independence | Built: declaration and file order never matter. |
| Domains, `public` and calls | Built: owner-qualified calls and per-declaration `public`, with `visibility.private_cross_domain`, `visibility.public_not_allowed` and `visibility.public_unused` (chunk 2.8). |
| Reserved words | Built: a reserved word can't name anything. |
| Comments | Built: `//` and `///` documentation comments. |
| Similar helpers | Built (chunk 1.7): the `library.similar_exists` warning. |
| Constants | Built (chunk 5.3e): `constant` as a reserved word and a declaration placed and shared as helper functions are, a private one unique within its domain, `constant.similar_exists`, and a top-level `var` pointed at `constant` (area 3). |
| Imports | Built (chunk 3.5): `use <folder> { Names }` and `use <domain>`, with `import.missing`, `import.unknown`, `import.unused`, `import.unneeded` and `import.duplicate`; `jadpo fmt` adds, removes and sorts imports, `jadpo fmt --layout` makes promotion moves, and go to definition works from every use of a name, a `use` line's included. |
| A file holds only what its primary declaration uses, primary first | Built (chunk 3.9): `layout.unused_declaration` for a supporting declaration the primary declaration doesn't use, directly or through one another, and `jadpo fmt` moving the primary declaration to the top. |
| Referenced assets | Decided, not built: `layout.asset_unreferenced` waits for the reference forms of areas 11 and 16. |

### Consistency with the rest of the language

- **Two ways to organise a project.** Folders carry roles (`entities/`,
  `routes/`) while modules carry visibility, and the two are unrelated: a file's
  folder says nothing about its module and vice versa.
- **Import lists are the only comma-separated brace block.** Every other `{ }`
  block in the language holds one `name: value` item per line; `import m { A, b }`
  holds a comma list on one line. The 2026-10-14 `use <folder> { Names }`
  keeps one, deliberately: see [Imports](#imports).
- **Three call spellings, by rule.** Your own functions are called bare
  (`add_delivery(x)`), the standard library is always qualified
  (`temporal.in_zone(x)`), and the generated entity operations go through
  the entity (`Todo.get(id)`). This is deliberate (the qualifier shows who owns the
  operation) and consistently enforced.
- **Declaration headers differ.** `type Name = Base { }` uses `=`, while
  `entity Name { }`, `failure Name { }` and `service Name { }` do not. This
  spans several areas and is flagged for the end-of-review syntax pass.
- **Separation risks duplication, not breakage.** Every name is unique
  wherever it can be used (a private function or type within its domain,
  everything else across the project), so every reference resolves to one
  declaration wherever it lives, but an agent working in one domain can
  write a helper that already exists in another, and since 2026-10-16 two
  domains may each hold a private one with the same name. Placement cannot prevent
  two copies. Mitigations: agents look things up in the compiler's project map
  (`jadpo inspect`, the language server) rather than browsing folders, and a
  possible compiler warning for near-identical functions or types.
  The compiler also generates a catalogue of every library function with its
  signature and documentation comment; the Jadpo agent skill tells agents to
  consult it before writing a helper. Nothing is maintained by hand.

### Decided layout

Shaped in discussion with Richard on 2026-10-11 and 12, on top of three
pieces of research in `docs/language/research/` (what developers like and
dislike in other frameworks' layouts, six realistic applications from this
todo app up to a 72-entity platform, and candidate layouts tested against
them) and an independent review of this proposal against that research and
fresh evidence. Decided on 2026-10-12, and built in chunks 1.3 and 1.9
except where a part says otherwise.

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
- `app.jadpo` also records the project's Jadpo language version.
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
| `libraries/` | Named groups of helper functions and constants, e.g. `library rounding` | Called like the standard library: `rounding.half_even(x)`, `pricing.vat_rate` (constants decided 2026-10-16, area 3) |
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
  helpers used only by that file live in it too. A declaration any other
  file uses (its own test file aside) has a file of its own, named after it,
  so every name a `use` line brings in is `<folder>/<name>.jadpo`
  (2026-10-14, see [Imports](#imports)).
- Everything in a file besides its primary declaration is used by the
  primary declaration, directly or through one another. One that only the
  file's tests use, or nothing uses, is dead code, reported as
  `layout.unused_declaration` like an unused shared declaration, so a file
  never collects helpers for something else (2026-10-15, Richard; built in
  chunk 3.9).
- The primary declaration comes first in its file, after any opening
  comment. `jadpo fmt` moves it there and leaves everything else in the
  order it was written, so opening a file shows what it is for
  (2026-10-15, Richard; built in chunk 3.9).
- A name must convert to its file name and back unchanged, so consecutive
  capitals are not allowed: `VatRate`, not `VATRate` (Rust's guideline too).
- Names are unique across the whole project and across kinds, with one
  exception: a private `function` or `type` (other than a failure or an
  email, `type X = Failure` or `EmailMessage`) needs a name unique only
  within its domain, whether it has a file of its own or sits beside the
  declaration that uses it (2026-10-16, Richard; built in chunk 5.3d). A private `constant` follows the same rule (area 3,
  2026-10-16, Richard; built in chunk 5.3e). Everything else stays unique across the project:
  - anything `public`, and anything in `shared/` or the root files;
  - every other kind of declaration, whatever its visibility, since most
    have a name outside the code: entities (tables), failures (public error
    codes), events, actions, queries, routes, schedules, subscribers
    (stored deliveries and schedule state name them), services, fixtures
    and emails;
  - a type an API's record names, private or not: a route's input or
    output, or a type reached through their fields, is an OpenAPI schema
    name;
  - library names, which also qualify their functions
    (`rounding.half_even`).

  A project-unique name is unique against domain-scoped ones too: a private
  `Address` in `billing` and a public or shared `Address` clash, while two
  private `Address` types in `billing` and `shipping` do not. Two names
  folding to one file name (`naming.folded_collision`) follow the same
  scope. So a search for a name finds one file, or one file per domain for a
  private function or type. Domain, library and standard-library names are
  unique among themselves, and local variables cannot reuse them.
  Standard-library names are fixed per language version, so a new standard
  namespace never breaks an existing library.
- Why names are per domain for private functions and types: agents working
  in parallel workspaces on different domains would otherwise clash on
  private helper names when their work merges. Imports stay exact: a
  private name can only be imported inside its own domain, where it is
  unique, and anything importable across domains is unique across the
  project, so `import.missing` can still name the exact `use` line.
- Rejected (2026-10-16): making each whole domain a namespace. Tables,
  error codes, OpenAPI schema names, event names and identity keys all need
  one unique name anyway, so it would only move the prefix into generated
  names, and `Customer` would mean different things by folder in a diff.
- A clash is `semantic.duplicate_declaration`, which says whether the two
  are a copy (use the existing one, or move it to `shared/`) or two things
  with one name (rename one with `jadpo rename`), never invents a name, and
  lists the names in use containing the same word. `jadpo rename` refuses a
  new name already in use before editing anything (area 14). An agent
  working in parallel merges `main` before pushing, runs `jadpo check` (it
  builds nothing: 0.2 s on the 66-file library-loans example), fixes any
  clash, then pushes; generated CI runs `jadpo check` first so a clash fails
  in seconds (area 14, [Tools for agents](14-tooling.md#tools-for-agents)).
- One exception to file-name uniqueness: a type declared in the same file
  as an action, query or function may fold to that callable's name, as an
  input does (`type CreateTodo = Input` beside `action create_todo` in
  `actions/create_todo.jadpo`), because only one file carries the name. If
  the type is promoted to its own file, the rule applies again. Built in
  chunk 1.9.
- Tests sit beside their subject as `<name>.test.jadpo`. Scenarios that span
  several domains go in the root `tests/`. The kind of test (pure,
  action, route, schedule) is known from what it calls, so it is not a
  folder; `jadpo test --kind route` runs one kind.
- Non-Jadpo files (provider contracts, email templates, search mappings) sit
  beside the file that uses them and must be referenced from it.

#### Sharing and promotion

- A type, failure or helper used by one file lives in that file. When a
  second file in the domain uses it, it must move to a file of its own in the
  domain's `types/`, `failures/` or `libraries/`; when a second domain does,
  to `shared/` (pure types, failures and libraries only). A constant
  (area 3) is placed as a helper function is: used by one file it sits at
  the top of that file, and used by a second it moves into a library and
  is read through it, `pricing.vat_rate` (2026-10-16, Richard; built in
  chunk 5.3e). Moving to the
  right folder is not enough: `DiscountCode` in `types/basket.jadpo` used by
  another file must move to `types/discount_code.jadpo` (2026-10-14). Tests never count as users, a route only counts when
  nothing else uses the type, and a failure counts where it is raised.
- Promotion is required: the compiler reports it with a one-step fix, applied
  through the editor or `jadpo fmt --layout`, never silently on save.
  Demotion is never forced: a promoted declaration stays where it is until
  nothing uses it, when it becomes a dead-code error. This avoids code moving
  back and forth, broken `git blame` and merge conflicts on moves.
- Promotion is a move, not a rewrite: the library keeps its name, so calls
  such as `rounding.half_even(x)` and the function text stay identical and
  behaviour cannot change.
- Promoting a private function or type to `shared/` makes its name unique
  across the project, so a private one of the same name in another domain
  then clashes, as `semantic.duplicate_declaration`. The move writes one
  line in the identity registry, so it reads as a move and not as a removal
  and an addition (area 18, question 8; 2026-10-16, built in chunk 5.3d:
  `layout.promotion_required`'s edits for a type moving to `shared/types/`
  add the line to `database/identities.json`, so `jadpo fmt --layout`
  writes it. A failure raised by a second domain gets no edit, so its move
  by hand writes no line, and a library keeps its name in `shared/`, so
  its functions' keys need none).
- A helper that depends on one domain's types (`order_math.total(order)`)
  stays in that domain and becomes public when another domain calls it.
  Depending on two domains' types is an error that needs a design decision.
  Making a helper generic, for example extracting `money.sum(amounts)` for
  `ordering` and `payments` to share, is a deliberate refactor.
- Two constants of the same type holding the same value get the
  `constant.similar_exists` warning, so parallel agents each inventing
  `vat_rate` find each other's (area 3, 2026-10-16; built in chunk 5.3e).
- Duplicates are caught at the point of writing: when a new helper looks like
  an existing one, `jadpo check` says so and names it ("a similar library
  exists: `rounding.half_even` in `shared/libraries/rounding.jadpo`"). The
  compiler also generates a catalogue of every library function with its
  signature, documentation comment and callers.
- A function's description is written once, as a `///` comment above it.
  The catalogue, the caller list and the domains that use it are computed
  and shown in the editor, never written into source files.
- Before shared or public code changes, its reach is visible in three ways:
  find-references in the editor, a `jadpo impact <name>`
  command listing calling domains, entry points and covering tests, and
  `jadpo check` itself reporting which domains a change affects and running
  their tests.

#### Ownership and calls replace modules

- A domain owns everything in it and is the visibility boundary.
  Declarations are private to their domain unless marked `public`.
- Entity identities, types, failures, events, queries, libraries and actions
  can be public, and so can a fixture, which is still callable only from
  tests, fixtures and seeds: another domain's tests call a public fixture
  through its domain, `planning.alice_with_todos()` (area 13). `public` on
  anything else (a function, a route group, a schedule, a subscriber, a
  test or a seed) is `visibility.public_not_allowed`, and `public` on a
  declaration nothing outside its domain uses is `visibility.public_unused`
  (another domain's code or tests, a root `tests/` scenario, or the
  signature of a public declaration one of them uses; the domain's own
  tests don't count). An own-domain action or query called
  as `planning.snooze_todo()` from inside `planning` is
  `semantic.own_domain_qualification`. A public action is the domain's write interface: checkout in
  `ordering` can call `inventory.reserve_stock(...)` and both changes commit
  or roll back together in one database transaction. Another domain never touches the entities
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
  named by their entity. A call names one owner: a library is called by its
  own name, never through its domain, so `pantry.sacks.grams_in(1)` or
  `pantry.nonexistent.thing(x)` is `semantic.unknown_callee`; only a
  connection's operations take three names (area 11). A call that reaches
  nothing is always reported, never left to fail when it runs (built in
  chunk 2.7).
- There are no `module` headers. Imports came back on 2026-10-14 in a
  different form: see [Imports](#imports).

#### Imports

Decided by Richard on 2026-10-14 and built in chunk 3.5. With
no imports, a name read in one file could be declared in that file, in its
domain's `types/`, `failures/` or `entities/`, in `shared/`, or in another
domain: an agent had to search for it and a reader had to click through
files. Imports make the place every name comes from readable from the file
itself, without searching.

Each `use` line names a folder, then the names it brings in, written exactly
as the code uses them, so finding a name in the page finds its import:

```jadpo fragment
// domains/sales/actions/price_basket.jadpo
use shared/types { Pence, Percent }
use shared/libraries { money }
use sales/types { Basket, DiscountCode, Receipt }
use sales/failures { BasketEmpty, CodeUnknown, DiscountTooLarge }
use sales/actions { price_lines }
use menu
```

- **Every name not declared in the file is imported**, your own domain's
  included. A name brought in from `sales/types` is in
  `domains/sales/types/<name>.jadpo`; one from `shared/types` in
  `shared/types/<name>.jadpo`. The path leaves out `domains/`, so `shared`,
  `app`, `config`, `authentication`, `policy` and `flags` cannot name a
  domain.
- **What the project declares is imported, root files included**: a
  connection in `app.jadpo` comes in as `use app { ... }`.
- **Built-ins are never imported.** Built-in types (`Text`, `Int`,
  `List<T>`) and standard-library namespaces (`collection.count(...)`) are
  fixed per language version and can't be declared by a project. An
  entity's generated operations (`Todo.get(id)`) come with the entity.
- **Calls into another domain keep the domain's name at the call.** `use menu`
  brings in the domain, and its public actions, queries and fixtures are
  called as `menu.price_item(...)`, so a call that crosses domains is
  visible on its own line. Another domain's public types and failures come
  in by folder like any other name: `use menu/types { MenuItem }`.
- **A test file sees everything its subject file declares** without
  importing it, so `price_item.test.jadpo` calls the `loaf_price` helper
  in `price_item.jadpo` directly. Anything else it uses is imported.
- **Reading a name therefore has three answers:** it is on a `use` line, so
  it is in `<folder>/<name>.jadpo`; it is a built-in; or it is declared in
  this file.
- **The formatter keeps imports.** One line per folder, sorted, and
  `jadpo fmt` or the editor adds a missing import and removes an unused one.
  Every name a file can import is unique where it can import it (a private
  function or type within its own domain, anything importable across
  domains across the project, 2026-10-16), so the import for a name is
  never in doubt and a forgotten one is fixed, not a compile failure for an
  agent.
  `jadpo check` still reports an import that names nothing, and one the
  file doesn't use.
- **Every name clicks through in the editor.** Go to definition from any
  use of a name, in a body, a signature or a `use` line, opens its
  declaration in its `.jadpo` file.

This brings back the one comma-separated `{ }` list in the language (see
[Consistency](#consistency-with-the-rest-of-the-language)), accepted because
the names have to appear exactly as the code writes them.

Built in chunk 3.5, with these readings (each on the
[Decisions](#decisions) table):

- **Where the lines go.** `use` lines open the file, below the comments
  that open it up to a blank line. `jadpo fmt` sorts them as the example
  above is: names before domains, from the root files through `shared/` to
  each domain's folders by domain name, with a domain's or `shared/`'s
  folders in the order the layout lists them (`entities/`, `types/`,
  `failures/` ...), then the `use <domain>` lines; the names in the braces
  are sorted, and a line too long for 100 columns puts one name per line.
  `use` is a reserved word, so nothing is named `use`.
- **What `jadpo check` reports**, each with the edit that fixes it:
  `import.missing` for a name used without its import, `import.unknown`
  for a line naming a folder, domain or name that isn't there (a name
  imported from the wrong folder gets the edit moving it to the right
  one), `import.unused` for one the file doesn't use, `import.unneeded`
  for a built-in, the file's own declaration, its test subject's or its
  own domain, and `import.duplicate` for a name or folder written twice. A
  private name of another domain is still `visibility.private_cross_domain`
  where the file uses it, and a call that reaches nothing is still
  resolution's to report: an import is never asked for what the file can't
  use.
- **`jadpo fmt` fixes imports, then formats**, by applying those edits, once
  every file in the project parses; `jadpo fmt --check` fails on a file
  whose imports it would change.
- **Before a promotion.** A declaration in a file named after another, such
  as `SnoozeDays` beside `snooze_todo`, comes in from the folder its file
  is in (`use planning/actions { SnoozeDays }`) while
  `layout.promotion_required` asks for the move. The move's edit makes
  it, and `jadpo fmt` then moves each import to the name's new folder;
  `jadpo fmt --layout` does both in one step.
- **An entity's provided not-found failure** (area 6) comes in from the
  entity's folder: `use planning/entities { Todo, TodoNotFound }`.
- **Loose files** have no layout, so no folders to import from: every name
  in them is seen without a `use` line, and a `use` line there is
  `import.unknown`. A migration's `changes.jadpo`, which `jadpo migrate
  new` writes and which names what changed (`Todo.rank: { ... }`), imports
  nothing either.
- **A misspelling is one error.** While a name the file writes names
  nothing, no import is called unused, since the name may be a misspelt
  import; `jadpo fmt` removes none either.
- **The generated namespaces** (`clock`, `config`) are called through
  their names, as the standard library is, and need no import.

#### Database

- `database/identities.json` is the registry that gives every table, column
  and index a stable identity, so renaming a field renames its column instead
  of dropping it. Compiler-maintained, committed and reviewed. One sorted
  entry per line, so two branches that change the schema produce a readable
  merge.
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
11. **No warnings: every finding an error, some of them declinable?**
   *Decided 2026-10-17 (see Decisions):* yes, as recommended; built in
   chunk 5.4b. Until then a warning never
   fails `check` (`library.similar_exists`, built in chunk 1.7, and
   `constant.similar_exists`, chunk 5.3e), so an agent can leave one in
   place for ever and nobody decides it. The proposal:
   - **No warning severity.** Every finding is an error. Some rules are
     *declinable*: the author applies the fix, or declines on the
     declaration itself with a required reason. An agent or a person may
     decline.
   - **One decline form for every declaration** (type, field, function,
     action, constant): a line directly above it, where a `///` comment
     sits, naming the rule and the reason.

     ```jadpo fragment
     decline library.similar_exists: "Rounds to the nearest 5p for the till; rounding.capped rounds down"
     function till_round(price: Pence) -> Pence { ... }
     ```

     `decline` becomes a reserved word; the reason is non-empty text; a
     decline covers that one rule on that one declaration. A decline whose
     rule no longer fires there is an error (`decline.unused`), so declines
     don't outlive their reason, and naming a rule that can't be declined is
     `decline.not_declinable`.
   - **Compared with a per-rule option** such as area 19's
     `not_standard: "..."`: an option needs a new name per rule, fits only
     where a declaration has a settings block (types, fields, entities; not
     functions or actions, which have none, area 3), and can't be listed
     or cleared by one command. *Recommendation:* the generic form, so
     `not_standard:` becomes `decline type.standard_exists: "..."`.
   - **Declinable rules are the judgement calls:**

     | Rule | Area | Why it can be right to decline |
     |---|---|---|
     | `library.similar_exists` | 1 | Two helpers can differ on purpose |
     | `constant.similar_exists` | 3 | Two constants can share a value by coincidence (a VAT rate and a fee) |
     | `type.standard_exists` (proposed) | 19 | A shape can match a standard type by chance |
     | A personal field nothing reads | 15 | Kept for a legal reason and read only by export |
     | Rate limits counted in memory | 16 | One instance, by design |
     | An email that would fail the deliverability checks | 16 | An image-only notice can be deliberate |
     | A write refusable after a changing call (proposed) | 11, question open there | The condition may be unable to change |
     | #88's warnings (an unhandled failure, an unused template field) | #88 | Sorted when #88 merges |

     Plain mistakes are never declinable: every other rule, including area
     11's integration retries that outlast a route's deadline, which can't
     work as written.
   - **Rules triggered by time passing** (area 16's flag past its
     `expires:` date) are judged at today's date by `check`, so CI fails a
     new change, but at the revision's commit date by `jadpo build`, so
     rebuilding a merged revision for a hotfix is never blocked by a date
     passing. They aren't declinable: extending the date is the reviewed
     choice (area 16).
   - **Seeing declines.** The approval report (`jadpo approval`, area 18)
     lists every decline a change adds or edits with its reason, so
     declining by reflex shows in review. `jadpo declines list` prints
     every decline: rule, place and reason, filtered by `--rule <id>` and a
     path, with `--json`.
   - **Resetting them.** `jadpo declines clear`, with the same filters,
     removes the decline lines, so each finding comes back and `check`
     fails until it is fixed or declined again, which is the point. It is
     for after a new standard type lands, after a compiler upgrade makes a
     rule smarter, and for a periodic sweep. The editor's "Decline with a
     reason" and "Remove decline" code actions run the same code as the
     commands, as "Make this a setting" and `jadpo config move` share one
     (area 10). Output follows the
     [CLI design language](research/14-cli-design-language.md):

     ```text
     $ jadpo declines list --rule type.standard_exists
     Declines
       type.standard_exists  domains/maps/types/grid_square.jadpo:3  Grid squares, not WGS84 degrees
       type.standard_exists  domains/quotes/types/quote.jadpo:5      A quote's figure, not money owed

     ✓ 2 declines · 1 rule · 2 files

     $ jadpo declines clear --rule type.standard_exists
     ✓ Removed 2 declines from 2 files

     → jadpo check  shows each finding again, to fix or decline
     ```
   - **Cost.** The two built warnings become declinable errors, so code
     that passes `check` today can fail it; `warning` leaves the CLI design
     language's closed set of headline words and area 14's error shape;
     the language server shows them as errors.

   *Recommendation:* yes, and a roadmap chunk **"Declinable errors" `opus ·
   high`** right after 5.4, before 5.5 builds `type.standard_exists`: the
   `decline` line, `decline.unused` and `decline.not_declinable`, the two
   built warnings converted, `jadpo declines list` and `clear`, the
   editor's code actions on the same path, the approval report's list, and
   the date rule for time-triggered checks. Added to the roadmap as 5.4b.

### Decisions

Decided by Richard from 2026-10-12, each on the date shown. Status, above, says what is built.

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
| Call naming | The dot names the owner of what you call; own-domain actions and queries are bare; no imports (Amended 2026-10-14: imports are back, see Imports) | 2026-10-12 |
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
| Public fixtures | A fixture may be `public`, offered to other domains' tests, fixtures and seeds and to scenarios in root `tests/`, which call it through its domain (`planning.alice_with_todos()`); only tests, fixtures and seeds ever call a fixture, and a private one stays its own domain's. Area 13 wins over the earlier rule here that `public` on a fixture was `visibility.public_not_allowed` (Richard) | 2026-10-14 |
| Unused `public` | `public` on a declaration nothing outside its domain uses is the error `visibility.public_unused`, whose edit removes `public`. Users are another domain's code, root `tests/` scenarios, and the signature of another public declaration that is itself used outside; the domain's own tests don't count. A package's public members are its interface to other projects and are exempt (area 11) (Richard) | 2026-10-14 |
| Imports | Every name not declared in a file is imported with `use <folder> { Names }`, own domain included; built-ins are never imported; another domain is brought in with `use <domain>` and its calls keep the domain's name; a test file sees its subject's declarations; the formatter adds and removes imports; go to definition works from every use of a name. Reverses "no imports" (Richard) | 2026-10-14 |
| A file per shared name | A declaration any other file uses has a file of its own named after it; supporting declarations used only by their file (and its test) stay in it (Richard) | 2026-10-14 |
| Imports: where and in what order | `use` lines open the file, below its opening comments up to a blank line; sorted names before domains, root files, then `shared/`, then each domain's folders by domain name, folders in the layout's order, then `use <domain>` lines; names in the braces sorted; one name per line past 100 columns; `use` is reserved (chunk 3.5) | 2026-10-15 |
| Imports: errors | Five rules, each with its edit: `import.missing`, `import.unknown` (folder, domain or name not there, or the wrong folder), `import.unused`, `import.unneeded` (built-in, own declaration, test subject's, own domain) and `import.duplicate`; privacy stays `visibility.private_cross_domain` at the use, and nothing the file can't use is asked to be imported (chunk 3.5) | 2026-10-15 |
| Imports: fixing | `jadpo fmt` applies the import edits once every file parses, then formats; `--check` fails on a file whose imports would change; `--layout` makes the moves `layout.promotion_required` asks for (chunk 3.5) | 2026-10-15 |
| Imports: before a promotion, provided failures, loose files, migrations, generated namespaces | A declaration beside another file's main one is imported from that file's folder until promoted; an entity's provided not-found failure comes from the entity's folder; loose files have no imports, and a `use` line there is `import.unknown`; a migration's `changes.jadpo`, which `jadpo migrate new` writes and which names what changed (`Todo.rank:`), imports nothing; `clock` and `config` need no import; while a name the file writes names nothing, no import is called unused (chunk 3.5) | 2026-10-15 |
| A file per shared name: where it applies | A second declaration in any domain or `shared/` file, `types/` and `libraries/` included, moves to a file of its own once another file uses it; one only its own file uses stays; the edit makes the move, and `jadpo fmt --layout` makes it and updates the imports (chunk 3.5) | 2026-10-15 |
| A file holds only what its primary declaration uses | The primary declaration is the one of the folder's kind named after the file (the first of that kind when the file is misnamed), or the seed in `database/seeds/`; every type or helper function beside it is used by it, directly or through one another, or is `layout.unused_declaration`. Tests never count. One another file uses is moved by `layout.promotion_required`, not reported unused. While the primary, or a declaration it uses, writes a name that names nothing, perhaps a misspelling of the one it meant, nothing beside it is called unused, as with imports. Test files, root files and loose files have no primary declaration, and a library's functions keep their own rule: one nothing calls is unused (chunk 3.9) | 2026-10-15 |
| The primary declaration first | `jadpo fmt` moves the primary declaration below the file's opening comments, up to their last blank line, and its `use` lines; it takes the comments written above it and the one after it on its line, and everything else keeps its written order. `jadpo fmt --check` fails on a file whose primary declaration isn't first; loose files keep their order (chunk 3.9) | 2026-10-15 |
| Names per domain | A private function or type (not a failure, an email, or a type an API's record names) needs a name unique only within its domain; everything public, in `shared/` or the root files, and every other kind (entities, failures, events, actions, queries, routes, schedules, subscribers, services, fixtures, emails) stays unique across the project, as do domain, library and standard-library names among themselves. Why: agents in parallel workspaces on different domains would otherwise clash on private helper names at merge; imports stay exact. Rejected: a namespace per domain, which only moves the prefix into tables, error codes, OpenAPI and event names, and makes one name mean different things by folder. The identity key of a function or type declared in a domain carries the domain, and promotion to `shared/` writes a registry move line (amends area 18's question 8). `semantic.duplicate_declaration` says copy or clash, never invents a name, and lists the names in use with the same word; `jadpo rename` refuses a name in use; agents merge `main` and run `jadpo check` before pushing, and generated CI runs `jadpo check` first. Built in chunk 5.3d, but for `jadpo rename`, which comes with the command (Richard) | 2026-10-16 |
| Constants in the layout | A `constant` (area 3) is placed and shared as a helper function is: used by one file it sits at the top of that file, never as a primary declaration, and used by a second it moves into a library and is read through it (`pricing.vat_rate`), so a library holds functions and constants; a private constant needs a name unique only within its domain; `constant` is reserved; a top-level `var` is `syntax.expected_declaration` whose `next:` writes `constant`; two constants of one type holding one value warn `constant.similar_exists`. Built in chunk 5.3e (Richard) | 2026-10-16 |
| Declinable errors (question 11) | No warning severity: every finding is an error, and the judgement-call rules (question 11's table) are declinable. A declaration declines one rule with a `decline <rule>: "<reason>"` line directly above it, the reason required; an agent or a person may decline; a decline whose rule no longer fires is `decline.unused`, and naming a rule that can't be declined is `decline.not_declinable`. Plain mistakes are never declinable. `jadpo approval` lists every decline a change adds or edits; `jadpo declines list` and `clear`, with `--rule <id>`, a path and `--json`, show and reset them through the same code as the editor's actions. A rule triggered by time passing (area 16's expired flag) is judged at today's date by `check` and at the commit's date by `jadpo build`, so it blocks new changes but never the rebuild of a merged revision (Richard) | 2026-10-17 |
