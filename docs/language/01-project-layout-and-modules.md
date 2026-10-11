---
title: Project layout and modules
area: 1
status: in-review
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

File order and declaration order never matter: a declaration can refer to one
defined later or in another file. Without module headers the whole project
shares one namespace, and every declaration is visible everywhere. The golden
todo app works this way.

Each name can be declared once per project. A second `Count` in another file is
an error (see `semantic.duplicate_declaration` below).

## Modules

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

Line comments start with `//`. They carry no meaning and are kept for tooling.
There are no block comments and no documentation comments yet.

## Every error has the same shape

Each compiler error gives the location, a reason, a suggested next step and who
should act on it, plus a stable rule ID with a help link. Tools read the same
fields as JSON with `jadpo check <project> --diagnostic-format=json`.

## Errors

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
- **The formatter keeps your blank lines and import order.** Two people can
  format the same file differently and both pass `jadpo fmt --check`.

### Proposed layout (preliminary)

Based on three pieces of research in `docs/language/research/`: what
developers like and dislike in other frameworks' layouts
(`layout-patterns.md`), six realistic applications from this todo app up to a
72-entity, eight-team platform (`layout-scenarios.md`), and three candidate
layouts tested against all of them (`layout-mapping.md`). Everything here is a
proposal for review; nothing is built.

**The shape.** Role folders, as today, while the app is small. When an app
grows into separate business areas, it declares domains, and the same role
folders move one level down into a folder per domain. A todo app never sees a
domain folder; a 72-entity app has one folder per team.

```text
app.jadpo                  application, principal, readiness
config.jadpo               configuration
authentication.jadpo       authentication methods and their failures
entities/
  todo.jadpo               Todo, plus the types and failures only it uses
  todo.test.jadpo          its tests, beside it
  user.jadpo
  service.jadpo
  service_credential.jadpo
values/
  todo_view.jadpo          promoted: used by Todo and User
  user_status.jadpo        promoted: used by User and authentication
workflows/
  send_due_reminders.jadpo
routes/
  api_todos.jadpo          every /todos route
  api_todos.test.jadpo
  api_users.jadpo          every /users route
  api_health.jadpo
jobs/
  overdue_reminders.jadpo
services/
  reminder_mail.jadpo      the mail provider, its messages and failures
  reminder_mail.contract.json
.jadpo/                    committed compiler data (schema identities)
build/                     generated output, never edited
```

The same rules at scale, once domains are declared:

```text
app.jadpo  config.jadpo  authentication.jadpo  policy.jadpo
shared/
  values/       money.jadpo  country_code.jadpo  address.jadpo
domains/
  ordering/
    entities/   cart.jadpo  order.jadpo  order.test.jadpo  order_line.jadpo
    queries/    order_history.jadpo
    workflows/  checkout.jadpo
    routes/     v1_orders.jadpo  v2_orders.jadpo  backoffice_orders.jadpo
    events/     order_event.jadpo
  payments/
    services/   stripe.jadpo  stripe.contract.json
    routes/     webhooks_stripe.jadpo
  catalogue/  fulfilment/  identity/
```

#### Where things go

- Every kind of declaration has exactly one home. Entities in `entities/`,
  cross-entity reads in `queries/`, multi-entity actions in `workflows/`,
  routes in `routes/`, and likewise `events/`, `subscribers/`, `services/`,
  `jobs/` and `migrations/`.
- One main declaration per file, and the file is named after it:
  `ServiceCredential` lives in `service_credential.jadpo`.
- Types, failures and helper functions live in the one file that uses them.
  When a second file starts using one, it moves to `values/` or `functions/`.
  The compiler enforces this in both directions and `jadpo fmt --layout` does
  the move, so `values/` only ever holds things that are genuinely shared.
- Routes are filed by audience and URL: `api_todos.jadpo` holds every `/todos`
  route. Audiences and API versions (`admin`, `partner`, `v1`, `v2`,
  `webhooks`) are declared once with their URL prefix, so `/admin/orders`
  lives in `admin_orders.jadpo`. The URL in a log line tells you the file.
- Tests sit beside their subject as `<name>.test.jadpo`. Non-Jadpo files
  (provider contracts, email templates, search mappings) sit beside the file
  that uses them and must be referenced from it.
- Fixed root files: `app.jadpo`, `config.jadpo`, `authentication.jadpo` and
  `policy.jadpo`. A tiny project may keep everything in a single `app.jadpo`.
- A `.jadpo` file anywhere else is an error. Folders without `.jadpo` files
  (`docs/`, `.github/`) are ignored.

#### Names

- A name must convert to its file name and back unchanged, so consecutive
  capitals are not allowed: `VatRate`, not `VATRate`. This avoids the acronym
  problem Rails has with file-name mapping.
- Names stay unique across the whole project and across kinds, so every file
  name is unique too: no screen full of identical tab names.

#### Domains replace modules

- A domain folder is the owner of everything in it and the visibility
  boundary. Declarations are private to their domain unless marked `public`.
- Only data, queries, events and pure functions can be public. One domain can
  read another's data through its public queries, but can only cause changes
  in another domain by emitting an event.
- `shared/` holds only pure types and functions used across domains.
- There are no `module` headers and no `import` lists.
- Moving a file within a domain changes nothing. Moving it to another domain
  changes its owner, and that shows up in review.

#### What the compiler would check

New errors would include `layout.unknown_location`, `layout.wrong_home`,
`layout.file_name_mismatch`, `layout.supporting_home`,
`layout.route_file_mismatch`, `layout.test_without_subject`,
`layout.asset_unreferenced`, `naming.round_trip`, `naming.folded_collision`,
`visibility.private_cross_domain` and `layout.call_direction`. Most have
exactly one fix, which `jadpo fmt --layout` applies.

#### Hard to reverse

Domains as owners, the name-to-file rule, route files keyed by audience and
URL, and project-wide unique names. Everything else is a mechanical move,
because moving a file never changes a declaration's meaning or database
identity.

### Open questions

1. **Adopt the proposed layout?** Role folders while small, domain folders
   above them when an app grows, one home per kind, files named after their
   declaration, and single-use types kept beside their user.
   *Recommendation:* yes. A small app looks like today's layout, a large one
   gets one folder per team, and an agent never has to choose where a new file
   goes.
2. **Should domain folders replace modules?** This deletes `module` and
   `import`, and reverses the event design's current rule that owners come
   from module names rather than folders. *Recommendation:* yes. Otherwise
   ownership needs a second hand-maintained mapping, which is the "two ways
   to organise a project" problem this page already flags.
3. **How is policy protected from agent edits?** The original golden app kept
   policy in a separate human-owned file. Under this layout, entity policy
   lives in the entity file. *Recommendation:* protect it through the approval
   check, which already fingerprints policy, so any policy change needs human
   approval wherever it lives. Protection by file path would need a policy file
   beside every entity.
4. **Multiple deployables (API, worker, back-office) as declarations in one
   project, not separate projects?** *Recommendation:* declarations. Each
   deployable selects its audiences and entry points and is compiled from what
   it reaches. Separate projects would need a package system that doesn't
   exist.
5. **Large entity files.** An order entity in a real shop could reach 800
   lines. *Recommendation:* no companion files for now; the formatter keeps a
   fixed section order. Revisit with measured sizes from the order/payment app.
6. **Reserved words.** *Recommendation:* build the chosen policy:
   control-flow and declaration words are fully reserved, with a short explicit
   list of words such as `input` and `value` that can be used as names.
7. **Documentation comments.** *Recommendation:* add `///` comments that
   attach to the following declaration and flow into generated documentation
   and OpenAPI descriptions. Leave intent and decision annotations out.
8. **Formatter owns layout within a file.** *Recommendation:* the formatter
   decides blank lines (one between declarations) and section order within a
   file, so equivalent code always looks identical.

### Decisions

| Question | Decision | Date |
|---|---|---|
| 1 | | |
| 2 | | |
| 3 | | |
| 4 | | |
| 5 | | |
| 6 | | |
| 7 | | |
| 8 | | |
