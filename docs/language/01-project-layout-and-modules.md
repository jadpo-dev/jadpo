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

### Open questions

1. **Adopt one strict project layout?** Today only the entity rule is checked,
   and roots and made-up folders accept anything. *Recommendation:* make the
   layout fully enforced and closed: `app.jadpo` at the root for the
   application, configuration and authentication; `entities/`, `values/`,
   `queries/`, `workflows/`, `routes/`, `jobs/`, `services/` and `tests/` as the
   only folders; every declaration kind has exactly one home; each file is
   named after its main declaration; no other folders or root files. Role
   first rather than feature first, so any project can be navigated without
   knowing its domain. Revisit sub-folders only when a real app outgrows flat
   role folders.
2. **Drop the module system?** Names are already unique across the project,
   the compiler already knows the dependency graph, and the golden app does
   not use modules. With a strict layout, modules add a second organising
   system, 22 error messages and import lists to maintain, mainly in exchange
   for `public`/private visibility. *Recommendation:* remove modules and
   imports, and let the layout organise the code. If privacy turns out to be
   needed (helpers private to one entity file, say), add a single `private`
   modifier then.
3. **Reserved words.** *Recommendation:* build the chosen policy:
   control-flow and declaration words are fully reserved, with a short explicit
   list of words such as `input` and `value` that can be used as names.
4. **Documentation comments.** *Recommendation:* add `///` comments that
   attach to the following declaration and flow into generated documentation
   and OpenAPI descriptions. Leave intent and decision annotations out.
5. **Formatter owns layout within a file.** *Recommendation:* the formatter
   decides blank lines (one between declarations) and declaration order within
   a file, so equivalent code always looks identical.

### Decisions

| Question | Decision | Date |
|---|---|---|
| 1 | | |
| 2 | | |
| 3 | | |
| 4 | | |
| 5 | | |
