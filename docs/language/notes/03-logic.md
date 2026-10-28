---
title: Logic, design notes
area: 3
---

# Logic: design notes

Why [Logic](../03-logic.md) is the way it is: the decisions behind it, how it fits the rest of the language and what it carried to other areas. An app author reads the area page, not this one.

## Carried to later areas

| Item | Area | Status |
|---|---|---|
| Whether a runtime construction the declared ranges prove safe (`Stock(per_pack * packs)` with both minimums at 0 or above and no maximum) still needs `attempt` and a mapping | 4 | Decided in area 4, question 6: no |
| `fails` sets, `reject`, `propagate` and failure arms' mapping rules; how overflow, division by zero and recursion depth faults are reported | 4 | Input to area 4 |
| Calling actions or queries inside a `for` loop (one query per item, transaction scope) | 6 | Input to area 6 |
| Callable settings (`consistency:`, `freshness:`, `guard:`) become defaults; reading a cache or projection is declared on the cache | 6 | Follows from a decision |
| Recursive types (`replies: List<Comment>`) are accepted; storing them | 5 | Input to area 5 |
| `assert` and `advance clock by` | 13 | Input to area 13 |
| Sections gain a colon under the colon rule (`policy: { }`, `persistence: { }`, `lifecycle: { }`); `set: { }` and `initial: { }` already follow it | 5, 6, 9 | Follows from a decision |
| Route sections follow the colon rule | 7 | Input to area 7 |
| Entities: `identity: id` and one-value `persistence { store: primary role: authority }` become defaults, leaving `entity Customer { id: Uuid }` | 5 | Proposed |

## Decisions

| Topic | Decision |
|---|---|
| Match arm arrow (question 1) | `->` for match arms, the same arrow as result types; the formatter rewrites `=>` (area 2) |
| `var` or `let` (question 2) | `var` ("variable"), immutable by default, and `var mut` when it can change. Every keyword carries baggage from another language (`let` is mutable in JavaScript), and `var` with an explicit `mut` reads clearly on its own |
| Shadowing (question 3) | A name in scope cannot be declared again, parameters included, extending area 1's rule that locals cannot reuse domain and library names; for a reviewer reading a diff, one name meaning one value is worth more than Rust's shadowing |
| Standalone statements (question 4) | An unused variable is an error, so a value nobody needs is never bound to a name. Three forms stand alone instead: a call whose result is `Unit`; `attempt call(...)` with any result, meaning "this must succeed"; and a bare `return` in a `Unit` callable. A pure call whose value is thrown away is an error, because it cannot do anything |
| Conditions (question 5) | Always in brackets, `if (total > limit) { ... }`, with `else if` chains; the brackets end the condition before the block. `if` is a statement; choosing a value is a `match` |
| Operator operand types (question 6) | `+`, `-` and every comparison need both sides to be the same named type after widening (or one a literal); `*` and `%` may mix types, since multiplying a price by a quantity is meaningful and adding them is not, as with F#'s units of measure. Ordering applies to numbers and time values only; text order depends on locale and is `list.sort_by` with a text key |
| Division (question 7) | `/` divides two values of the same named type, like `+` and `-`. `Int` division drops the remainder (`10 / 3` is `3`), as in Rust, Go, Java and C#; `Decimal` division is exact, rounded half to even when the result is stored in a type with a fixed `scale`. Dividing values of different types, or another rounding, is `decimal.divide(total, people, rounding: Rounding.half_up)` or `integer.divide(...)`. Division by zero is ruled out by the compiler: the divisor's type must exclude zero (`type People = Int { min: 1 }`), a literal divisor must not be `0`, and a computed divisor is constructed into such a type first, the one place the zero case is handled. `%` is `Int`-only |
| One `match` (question 8) | One `match` for values and call outcomes: the subject's type decides which patterns apply, every `match` works as a statement or an expression, an arm is an expression, `reject`, `propagate` or a block, and failure arms have no `failure` prefix because failure names are already unique types. `attempt` is the short form for passing every failure on |
| Payload binding (question 9) | A `match` arm names a variant's data in brackets, `PaymentOutcome.paid(payment) -> payment.receipt_id`, the same shape as `some(subtitle)` and `success(value)`: brackets after a case name the data it carries |
| Wildcard on closed types (question 10) | No `_` on enums, `Bool` or nullable values; every case is listed, and `_` is only for text and numbers. Adding a variant then fails to compile at every `match` that must consider it, which is the review prompt a new state needs |
| `attempt` placement (question 11) | Only at the start of a statement (`var`, assignment, `return` or on its own), and only on a call that can fail. Every early exit is then visible at the start of a line, which Rust's `?` does not give, without Go's `if err != nil` repetition |
| Loops (question 12) | `for` over a list or a set (a map through `map.keys`) is the only loop, with `return` allowed inside; no `while`, `break` or `continue`. Every loop ends after its collection, which keeps request deadlines predictable, and searching or stopping early is `list.filter` or a `return`. No outside call (a provider operation, a package action, a file store's `put` or `read`, a `documents` render) inside a `for` body, in any action; a loop of calls is `batches.each`, the one way (area 12) |
| Passing functions (question 13) | Inline functions only as arguments to standard-library operations, written `todo -> todo.title`: one expression, pure, its parameter's type taken from the operation. No function types in your own signatures and no functions stored in variables; capturing surrounding variables is safe because values never change. The same form is the `each:` and `finished:` of the batch helpers, where with no destination its body is one call to an action in the same domain, and what it captures is saved with the run, so must be storable (area 12) |
| Recursion (question 14) | Functions only; an action or query in a call cycle is an error, because repeating effects has no data reason and an unbounded one never finishes. A runtime depth limit ends in a defined fault instead of a crash |
| Recursion limits | 1,000 frames and 400 inline functions running at once, past either the `recursion_limit` fault at the call or operation, the same in every executor and on every target; see [Recursion](../03-logic.md#recursion) |
| Callable settings (question 15) | None: the safe choice is the default, so an action is only its signature and its logic. Every action is all-or-nothing; a write to a loaded entity always checks it was not changed in the meantime; every read is current unless it reads a cache or projection, declared where that cache is (area 6). Who may call an action is policy (area 9). The cases this does not fit have better answers: a large batch that should keep partial progress is a schedule or subscriber handling one item per action; a call to an external provider comes before any write in the action and carries an idempotency key, so a retry repeats it harmlessly (area 11, question 7); a counter that must not conflict is updated by the database in one step, `set: { view_count: Post.view_count + 1 }` (area 6) |
| Generics and unions (question 16) | None in your own code; the standard library stays generic, and a parameter that accepts several types is an enum whose variants carry them, which keeps every value's type named and identifiers apart, with no type-parameter errors for agents to fight. Revisit only if the order and payment application shows repeated helpers that differ by type alone |
| Colon rule (language-wide) | Every member inside braces is `name: value`, and a value is one thing or a `{ ... }` block (`paid: { ... }`, `persistence: { ... }`); a variant with no data is just its name; a field's own settings still follow its type (`name: ProductName { optional: true }`); top-level declarations, statements and `match` arms are not members; route entries inside a `routes` group, keyed by method and path, are the one exception (area 7). Terraform's block-versus-attribute split is a known source of confusion |
| Constants | `constant name: Type = value`, at the top of a file or in a library; see [Constants](../03-logic.md#constants). Not a top-level `var`, functions with no arguments, values attached to their type or a root `constants { }` block ([Rejected](../03-logic.md#rejected)) |
| A constant or a setting | A value is a constant or a setting, never both, so it has one source and one way to read it; a setting's `default:` is a literal and never a constant. See [A constant or a setting](../03-logic.md#a-constant-or-a-setting) and area 10 |
| Outside calls in loops, checked | `effect.call_in_loop` at the call, not declinable: any outside call (a provider's operation, a store's `put` or `read`), one that only reads included, made in a `for` body directly or by an action or fixture called there, in actions and fixtures; tests and functions make none |
