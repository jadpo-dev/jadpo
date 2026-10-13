# Naming and qualification contract

**Status:** accepted contract; NAME-P0–P2 implemented for the current language surface

**Approved:** 2026-09-27

**Accepted-contract digest:** `sha256:914e6c32c3d349cccfc9184dfc1dc40eb9671b247b4664d41d964c338901c0b4` (section 2)

**Purpose:** give every authored and compiler-owned name one predictable shape,
owner, import rule, and canonical source spelling

This contract applies across the whole language. A library may add domain
vocabulary, but it may not invent a new casing system, import model, method
style, parameter-order convention, or alias for an existing operation.

> **Superseded in part (2026-10-12):** module names, imports and bare calls within a module (NAME-D03, NAME-D04, NAME-D09) are replaced by domain folders and owner-qualified calls. See the [decision register](decision-register.md#project-layout-domains-and-call-naming--2026-10-12).

## 1. Governing rule

Every name has one semantic owner, and that owner determines how the name is
spelled and called. Dot notation is not a generic object-oriented convenience:
it communicates a checked ownership or access relationship.

The language therefore does not pursue either “dots everywhere” or “no dots.”
It uses one canonical visual form for each semantic category and rejects a
second equivalent spelling.

## 2. Accepted decisions

| ID | Decision |
| --- | --- |
| NAME-D01 | Nominal or type-like declarations use `UpperCamelCase`. This includes semantic types, entities, enums/tagged sums, events, named failures, principals, services, and configuration declarations. |
| NAME-D02 | Runtime names use `lower_snake_case`. This includes bindings, parameters, fields, functions, actions, queries, jobs, capability instances, entity operations, and enum variants. Language keywords remain lowercase. |
| NAME-D03 | Logical module names are explicit stable `lower_snake_case` segments separated by dots, such as `shop.order_pricing`. Filesystem paths do not silently create or rename modules. |
| NAME-D04 | An authored free function, action, or query is called unqualified inside its declaring module and after an explicit selective import. Authored modules do not become runtime namespace objects. |
| NAME-D05 | An entity-owned static operation is qualified by the `UpperCamelCase` entity, such as `Order.by_id(id)`. An operation with a declared receiver may use receiver-dot syntax such as `order.cancel()`. These are the same checked entity operation, not independent aliases. |
| NAME-D06 | Compiler standard-library families use reserved lowercase domain namespaces and mandatory qualified calls, such as `temporal.in_zone(...)` and `collection.count(...)`. Standard namespaces are available without imports and cannot be opened, aliased, shadowed, or converted into receiver methods. |
| NAME-D07 | Validated construction uses the nominal owner directly, such as `Email(value)`. Enum and tagged-sum variants use their nominal owner plus a lowercase variant, such as `OrderStatus.awaiting_payment` or `PaymentOutcome.paid { ... }`. These are construction forms, not standard-library namespaces. |
| NAME-D08 | Dot access to contextual capabilities or typed data is not a callable namespace. Examples include `clock.now`, `config.mail_sender`, `current_principal.user`, and `customer.email`. The compiler retains those distinctions even when punctuation looks similar. |
| NAME-D09 | Imports are explicit and selective: `import shop.pricing { calculate_total }`. Wildcard imports, import aliases, namespace aliases, re-exports, relative imports, and alternate qualified call spellings are not part of the accepted core. |
| NAME-D10 | Standard-library operations use `lower_snake_case`, put the primary value first, then stable environmental context, and put policy/options last as named arguments. Boolean policy flags, reversed overloads, abbreviations, and synonymous verbs are rejected. |
| NAME-D11 | External stable strings follow the convention owned by their boundary rather than pretending to be source identifiers: configuration binding strings use platform conventions, public failure codes use lowercase snake case, and diagnostic rule identifiers use lowercase dotted names. Their mappings remain explicit and audited. |
| NAME-D12 | The compiler, formatter, generated reference, audit, and language server expose only the canonical spelling. Compatibility aliases may exist only for a reviewed external-data migration and must never appear as a second recommended source form. |

## 3. Casing catalogue

| Semantic category | Form | Examples |
| --- | --- | --- |
| module | lower snake segments | `todo.reminders`, `shop.order_pricing` |
| type-like declaration | upper camel | `Instant`, `CreateOrder`, `Order`, `PaymentOutcome` |
| event/failure/service/principal/config declaration | upper camel | `OrderCreated`, `PaymentDeclined`, `ReminderMail`, `Principal`, `AppConfiguration` |
| field, binding, parameter | lower snake | `created_at`, `order_total`, `relative_to` |
| free callable | lower snake | `calculate_total`, `send_receipt`, `orders_due` |
| entity operation | lower snake under entity | `Order.by_id`, `order.cancel` |
| job/capability instance/strategy | lower snake | `overdue_reminders`, `primary`, `signed_session` |
| enum or tagged-sum variant | lower snake under nominal owner | `Zone.europe_london`, `PaymentOutcome.paid` |
| standard-library namespace and operation | lower snake | `temporal.day_bounds`, `collection.count` |
| public failure code string | lower snake | `"payment_declined"` |
| diagnostic rule identifier | lowercase dotted | `time.offset_required` |
| external environment binding string | boundary-owned | `"MAILER_API_KEY"` |

Names should be complete words unless an abbreviation is an established domain
term represented consistently throughout the application. Canonical language
and standard-library names do not acquire short aliases merely to save tokens.

## 4. Imports and calls

An authored callable is imported by declaration and then called by its declared
name:

```text
module shop.pricing

public function calculate_total(items: List<OrderItem>) -> Money {
    // ...
}
```

```text
module shop.checkout

import shop.pricing { calculate_total }
import shop.orders { Order, OrderItem }

action checkout(order_id: Order.id, items: List<OrderItem>) -> Order {
    var total = calculate_total(items)
    var existing = attempt Order.by_id(order_id)
    return attempt existing.confirm(total)
}
```

The import does not create `pricing.calculate_total`, and the language does not
also accept the fully qualified `shop.pricing.calculate_total`. That avoids
three equivalent spellings for one call.

Standard-library families are compiler-known prelude namespaces, not authored
modules and not hidden imports:

```text
var count = collection.count(items)
var local = temporal.in_zone(order.created_at, Zone.europe_london)
```

The following alternatives are invalid:

```text
import temporal { in_zone }
import temporal as time
in_zone(order.created_at, Zone.europe_london)
order.created_at.in_zone(Zone.europe_london)
time.in_zone(order.created_at, Zone.europe_london)
```

Importing an entity brings its declared public operation surface with the
entity. Individual entity operations are not separately imported because their
entity ownership is part of their semantic identity.

## 5. Standard-library consistency

A new standard-library family must demonstrate that its operations form a
coherent domain whose unqualified names would create collisions or lose useful
ownership information. The namespace uses a short descriptive lowercase name;
operations use descriptive verb or query names beneath it.

Within every family:

- the transformed or inspected value is first;
- other values follow in a stable, documented order;
- environmental context follows the value, with `zone` before `locale` where
  both exist;
- ambiguity, overflow, rounding, presentation, and other policies are named
  arguments at the end;
- predicates use one clear name rather than `is_*`, `has_*`, and noun aliases
  for the same question;
- conversion names expose the destination or context, such as `in_zone`;
- exact and calendar-relative operations have visibly different names;
- there are no free-function, namespace, and receiver-method duplicates; and
- a target-runtime convenience API never determines authored naming.

The initial accepted namespaces are:

| Namespace | Ownership |
| --- | --- |
| `temporal` | instants, resolved zone-aware time, calendar dates, durations, bounds, and presentation formatting |
| `collection` | operations common to checked collection values; the exact bounded operation set remains pressure-driven |

Future text, numeric, encoding, or similar libraries must follow this contract
and receive an explicit name rather than placing generic verbs in the global
callable scope.

## 6. Enforcement and diagnostics

The compiler must reject or diagnose:

- casing that contradicts the declaration's semantic category;
- a declaration or module that shadows a standard namespace or prelude owner;
- an unqualified standard-library operation;
- an attempt to import, alias, or selectively open a standard namespace;
- an authored module called as though it were a namespace object;
- a fully qualified authored call when the declaration should be selectively
  imported;
- a separately imported entity operation;
- a receiver-method or free-function synonym for a standard operation;
- a second spelling with reversed argument order or abbreviated naming; and
- generated documentation or completion that recommends a non-canonical form.

Diagnostics should name the semantic owner and show the one accepted repair.
For example, an unqualified `count(items)` suggests
`collection.count(items)`; `instant.in_zone(zone)` suggests
`temporal.in_zone(instant, zone)`.

The language server completes only names valid in the current scope. Completion
after `temporal.` or `collection.` shows the corresponding standard operations;
completion after `Order.` shows only static entity operations and nominal
members; completion after an entity value shows only valid receiver operations
and fields.

## 7. Implementation sequence

### NAME-P0 — Freeze and migrate documentation

- add positive and negative naming/import fixtures;
- migrate provisional unqualified collection examples to `collection.*`;
- bind TIME-001 to the common standard-library rule; and
- expose this contract from the syntax, grammar, semantic model, decision
  register, issue log, and documentation index.

### NAME-P1 — Resolver and diagnostics

- reserve standard namespaces and classify every dotted expression by semantic
  owner;
- enforce casing and the canonical import/call form;
- add targeted diagnostics and repair suggestions; and
- emit ownership and canonical qualified identity in the semantic graph.

### NAME-P2 — Tooling and generation

- update formatter, hover, completion, signature help, rename, audit, and
  generated reference output;
- ensure generated targets may use different internal names without leaking
  them into authored source; and
- run collision, shadowing, rename, import, and deterministic-output evidence.

NAME-P0 freezes compiler fixtures but requires no generated runtime. NAME-P1/P2
may be implemented alongside the first standard-library namespace work; they do
not require a separate runtime subsystem.

Implementation record: the resolver enforces type-like versus runtime casing,
reserves compiler standard namespaces and operations, rejects imports and
modules that would create alternate standard-library spellings, and suggests
the canonical qualified repair. The semantic graph records standard namespace
and operation ownership. Formatter/LSP completion, hover, signature, rename,
semantic-token, and generated-target tests retain only the canonical spelling.
Collision, casing, import, unqualified-operation, Temporal, and deterministic
fixture evidence is part of the compile and workspace suites.

## 8. Stop conditions

Return to owner review if implementation would require:

- two accepted source spellings for one semantic operation;
- general namespace aliases or wildcard imports;
- treating an authored module as a runtime object;
- inferring ownership from filename or directory alone;
- adding a standard namespace for a single convenience function;
- exposing target-language naming in authored source; or
- changing public external codes merely to match source casing.

## 9. Reserved-word policy direction — 2026-10-01

The owner selected hard reservation for control-flow and declaration words,
with explicitly enumerated contextual names allowed only where both their
binding/declaration and their reference forms work. A word such as `return`
must not be accepted as a variable that cannot subsequently be read. This is
an accepted direction for RM-201; the exhaustive word-by-position matrix,
stable diagnostics and implementation remain work. Broader keyword-as-name
parsing was considered and not selected.

The accepted section-2 contract and its digest remain unchanged. RM-201 must
classify every current lexer keyword and identifier-bearing grammar production,
record exact safe contextual exceptions, preserve accepted casing/ASCII rules
and test declaration/reference pairs before claiming conformance. Until that
change passes its required review and gates, the checked implementation remains
the recorded baseline. See the [decision](decision-register.md#keyword-names-and-canonical-formatting--2026-10-01)
and [assessment](archive/work-plans/roadmap-assessment.md) for sequencing.

## 10. Naming reaffirmation and principal simplification — 2026-10-04

The owner locked in the existing two-convention direction after comparing
uniform lowercase, mixed camel case and the current conventions:

- `UpperCamelCase` for named types and declarative contracts: `Todo`,
  `TodoEvent`, `TodoNotFound`, `ReminderMail`, `TodoConfiguration` and
  `TodoApplication`.
- `lower_snake_case` for values, fields, parameters, variants and executable
  operations: `todo`, `todo_id`, `completion_requested`, `emit_event` and
  `complete_todo`. Existing jobs and runtime entry-point naming follow NAME-D02;
  no special extra casing category is introduced for the event work.

Keep `event TodoEvent { ... }`, `entity Todo { ... }`,
`failure TodoNotFound { ... }` and `service ReminderMail { ... }` as one readable
declaration family. General types retain `type Name = Base { ... }`; specialised
keywords already identify their declaration kind. Uppercase identifies a named
type or contract, not necessarily an ordinary freely constructible data record.
Construction, effects and authority still come from checked declarations.

The rationale is the inexpensive visual distinction between a type/contract
and a value, for example `var todo: Todo = attempt Todo.by_id(id)`. One canonical
spelling, compiler diagnostics, formatting and completion should enforce it.
No measured LLM accuracy improvement is claimed. The proposals to lowercase
service/application names, switch operations to lowerCamelCase or make all
identifiers lowercase were considered and not selected.

The owner also selected an **unnamed singleton principal block** for the
successor grammar: `principal { user { ... } service { ... } }`, providing the
compiler-defined `Principal` type and existing `current_principal` value.
References such as `Principal.user` remain explicit. Remove the redundant
authored principal name and principal-selector configuration; do not introduce
an `Actor` alias. Authentication remains the trusted producer, with the existing
closed variants, policy boundaries and protection against forged principals.
This is an accepted design direction pending RM-222 grammar/migration review
and RM-223 implementation. The current named principal grammar and frozen
authentication/naming contracts remain the implemented baseline until that
successor is reviewed and verified. Section 2 and its historical digest are
unchanged. See the [decision](decision-register.md#naming-and-principal-direction--2026-10-04).
