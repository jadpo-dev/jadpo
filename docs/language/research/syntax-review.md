# Syntax review: consistency and simplification across the language

**Status:** analysis and recommendations, 2026-10-12. This decides nothing; each
finding is settled in the area page it belongs to.
**Inputs:** the lexer keyword list (`jadpo/crates/syntax/src/lib.rs`) and the
contextual words in `parser.rs` and `parser/delivery.rs`;
[grammar v0.1](../../grammar-v0.1.md) (including §4.1, §4.2, tests and
fixtures, §13, §15, scheduled jobs and the §18 successor candidate);
[syntax draft](../../syntax.md); [naming contract](../../naming-and-qualification.md);
[event model](../../event-model.md); the source in `examples/` and
`tests/validation/fixtures/reminder-delivery-v1.jadpo`; and the review sections
of [area 1](../01-project-layout-and-modules.md) and
[area 2](../02-values-and-types.md).

**How the snippets were checked.** "Current" snippets are quoted from real
source or the grammar. Where the docs and examples disagree, the spelling was
checked with `jadpo check` (build `0cef445`); those cases say *verified*.
`examples/golden-todo/app.jadpo` and `policy.jadpo` are design input that does
not compile (`syntax.expected_declaration`); snippets from them are marked
*design-only*. Event, subscriber and component snippets come from the
unimplemented successor candidate and are marked *candidate*.

## 1. The lens

Each rule is drawn from a decision Richard has already made in areas 1 and 2.
Findings below cite these numbers.

| # | Rule | Where it comes from |
|---|---|---|
| 1 | **One spelling per concept.** No synonyms, no second accepted form, no legacy form kept alive. | `value` dropped because four keywords declared objects |
| 2 | **One declaration shape.** Every type is `type Name = Base { ... }` (derived: `type X = Output from Todo { ... }`). Declarations that are more than a shape (entity, failure, route, ...) are `keyword Name { ... }`. | Area 2 Q10, Q14 |
| 3 | **Settings go in a `{ }` block, one `name: value` per line.** No space-separated trailing modifiers, no sentence-like clauses, no several settings on one line. | Area 2 Q20 rejecting `name: ProductName optional` |
| 4 | **Closed option sets are lower_snake_case names** (`kind: not_found`, `version: v7`). Numbers only for real quantities. | Area 2 Q17 |
| 5 | **One symbol, one meaning,** and no second symbol where one would do. | `->` and `=>` challenge |
| 6 | **One word, one meaning.** A keyword never means two things, and is never also a usable variable name. | Area 1 reserved words |
| 7 | **No vague words or abbreviations.** A word must not read as something else. | Area 1 naming |
| 8 | **Every shape has a name.** No anonymous inline objects. | Area 2 Q19 |
| 9 | **Folder name = keyword plural.** | Area 1 folder decision |
| 10 | **The dot names the owner of a call** (`rounding.half_even(x)`, `Todo.get(id)`). No module or import. | Area 1 call naming |
| 11 | **Explicit for reviewers, never redundant.** State what a reader needs (every variable's type); don't restate what the same declaration already says. | Area 2 Q15, Q16 |
| 12 | **Fewer concepts.** If an existing construct can express it, don't add another. | Area 2 Q18, Q19 |

## 2. Syntax at a glance

Current spelling of every form, side by side. **Impl** = the compiler accepts
it today; **design** = design input only; **cand.** = successor candidate.

### Declarations

| Form | Current spelling | State |
|---|---|---|
| Scalar type | `type Pence = Int { min: 0 }` | impl |
| Type with no block | `type ReminderIntentId = Uuid` | impl |
| Object | `type User = Object { id: Uuid }` and `value X { }`, `input X { }`, `output X { }` | impl |
| Enum | `type S = Enum { a  b { f: T } }` and `enum S { a b }` | impl |
| Entity | `entity Todo { fields  identity: id  persistence { }  lifecycle { }  policy { }  membership { }  function/action/query ... }` | impl |
| Legacy storage | `persist Todo { identity: id  unique: x  index: y  references ...  inverse ...  constraint n: unique(a, b) }` | impl |
| Failure | `failure F { kind: NotFound  code: "f"  message: "..."  public { }  internal { } }` | impl |
| Callable | `action name(p: T) consistency: atomic fails A, B -> R { policy { } ... }` (also `function`, `query ... freshness: authoritative`) | impl |
| Entity operation receiver | `function g(self: ref) ...`, `self: value`, `guard: reload` | impl |
| Route | `route GET /todos/{id} { auth: none  path: { id: T }  query: T  headers: { h: T from "X-H" optional }  input: T  output: T  success: created  deadline: 2s  run: f(...) }` or `action: fails F { ... }` | impl |
| Configuration | `config Name { f: T { binding: "F"  secret: true  default: 30d } }` | impl |
| Application | `application Name { authentication { principal: Principal  revocation { mode: bounded  maximum_delay: 5m } } }` | impl |
| Principal | `principal Principal { user { fields }  service { fields } }` | impl |
| Authentication | `authentication name { transport { cookie: "x" }  validators { v { mode: signed  principal: user  secret: config.k  credentials { ... } } }  claims { a -> Principal.user.b }  resolution user { authority: E.f  active: expr  mappings { a -> P.b }  inactive: F }  exchange { ... } }` | impl |
| Service | `service Name { import { file  version  sha256 }  credential { scheme  header  secret }  base_url: "..."  egress: host:443  retry: exponential(max_attempts: 3, ...)  operation op  POST /path  input T  output T  request_key: ... equals header "X"  maps provider.x -> F }` | impl (parse) |
| Scheduled job | `job name every 15m { concurrency: singleton  run: f(...)  retry: next_schedule  delivery: reminder_v1 { selection: { }  hooks: { }  service: { }  authority: { }  completion: { } } }` | impl (parse; build fails closed) |
| Locales | `locales { default: "en-GB"  supported: ["en-GB", ...]  unsupported: fallback_to_default }` | impl |
| Fixture | `fixture n { clock: fixed Instant("...")  config { k: v  s: secret("...") }  service X: fake { op => accept T { } } }` | impl |
| Test | `test "sentence" using fixture { ... }` | impl |
| Modules | `module a.b`, `import a.b { X, y }`, `public type ...` | impl (being removed) |
| Readiness | `readiness database { access: deployment_plane  timeout: 2s  required: true }` | design |
| Policy file | `policy P version "v" { authentication { default required ... }  User { read when ...  fields { id: server_identity immutable } } }` | design |
| Event | `event E { variant { fields  policy { scope: f  Role: [invoke, read] } } }` | cand. |
| Subscriber | `subscriber name { on E.variant(payload) { ... } }` | cand. |
| Component, triggers | `component X { modules: todo }`, `triggers: [E.a, E.b]` | cand. |

### Entity items

| Item | Current spelling |
|---|---|
| Field | `name: T`, `name: T?` |
| Field facts, trailing | `id: Uuid identity`, `handle: Text unique`, `group: Text index`, `created_at: Instant generated { on: create }` |
| Field facts, block | `id: Uuid { generated: identity }`, `owner_id: User.id { role: TodoRole.owner  immutable: true }`, `email: Email { policy {} }` |
| Both on one field | `created_at: Instant { policy {} } generated { on: create }` |
| Identity | `identity: id` |
| Persistence | `persistence { store: primary  role: authority  unique: name  references owner_id: User.id as: owner on_delete: restrict  inverse todos: many Todo via: Todo.owner_id }` |
| Derived store | `cache n { store: s  from: x  strategy: y  delivery: durable }`, `projection n { ... }` |
| Lifecycle | `lifecycle { initial: { f: v }  visible when expr  transition n { from: expr  set: { } }  purge after expr from field }` |
| Policy | `policy { Role.x: [create, read]  operations { op { Role.y: [update] } } }` |
| Membership | `membership { scope: f  member: f  role: f }` |

### Statements and expressions

| Form | Current spelling |
|---|---|
| Bindings | `var x = e`, `var x: T = e`, `var mut x = e`, `x = e` |
| Control | `return e`, `reject F { ctx }`, `if c { } else { }` and `if (c) { }` |
| Value match | `match x { S.a => { }  P.paid { a, b } => { }  some(v) => { }  none => { }  _ => { } }` (statement only) |
| Outcome match | `match f(x) { success(v) => e  failure F => e / reject G / propagate }` (expression) |
| Propagation | `attempt e` |
| Create | `attempt create T { fields } conflict: F`, `conflict T.c: F` |
| Read | `query required T { where: f == e  missing: F { ctx } }`, `query optional`, `query many T { where  order_by: f asc  limit: n offset: n  include: r into: T order_by: ... }`, `include: r required into: T` |
| Page | `query page T -> P { where: ...  and: f matches optional e  and: f <= optional e  order_by: a desc, b desc  after: optional e  limit: e  project: V  cursor: C(a, b) }` |
| Update, delete | `update required T { where  set: { f: e }  patch: i  empty: F  set: { f: e when i.f supplied }  transition: n  missing: F  conflict: F }`, `delete required T { ... }` |
| Construction | `T(x)`, `T { f: e }`, `E.v`, `E.v { f: e }`, `Entity.field(x)` |
| Calls | `f(x)`, `Entity.op(x)`, `temporal.op(x, name: v)`, `Service.op(x)` |
| Context values | `config.f`, `clock.now`, `current_principal.user`, `path.x`, `query.x`, `headers.x`, `input` |
| Test-only | `assert e`, `advance clock by e`, `call f()` |
| Operators | `== != < <= > >= + - * / % and or not`, `T?`, `List<T>` |
| Candidate / design | `attempt emit_event(E.v { })`, `for x in xs`, `x exists`, `require ...`, `project x as T` |

## 3. Findings

Impact: **low** = parser-local, formatter or docs; **medium** = parser,
checker and the examples; **high** = many compiler subsystems or most source
files. Points already raised in areas 1 and 2 are listed once in §3.15 rather
than repeated.

### 3.1 Layout and modules (area 1)

| # | Area | Current form | Breaks | Proposed form | Impact |
|---|---|---|---|---|---|
| 1 | 1 | Keywords don't match the decided folders: `job overdue_reminders ...` goes in `schedules/`, `service ReminderMail { }` in `integrations/`; area 1's `library rounding { }` has no keyword yet | 9 | `schedule overdue_reminders { }`, `integration ReminderMail { }`, `library rounding { }` | medium |
| 2 | 1 | Singletons carry names nobody uses: `application TodoApplication {`, `config TodoConfiguration {`, `principal Principal {`; but `locales {` has none. Config is read as `config.mail_sender`, never by its name | 1, 11 | `application { }`, `config { }`, `principal { }`, `locales { }` (the unnamed principal was already selected on 2026-10-04) | low |

### 3.2 Values and types (area 2)

| # | Area | Current form | Breaks | Proposed form | Impact |
|---|---|---|---|---|---|
| 3 | 2 | Lists are written three ways: bare commas `pick: id, title`, `order_by: created_at desc, id desc`; brackets `TodoRole.owner: [create, read]`, `triggers: [..]`, `supported: ["en-GB", ...]`; braces `import m { A, b }`, `P.paid { receipt_id, paid_at }` | 1, 5 | `[a, b]` is the only list, matching the list literal of area 2 Q4: `pick: [id, title]`, `order_by: [created_at desc, id desc]`. Braces never hold commas. A signature keeps its comma lists (parameters, `fails A, B`) | low |

### 3.3 Logic (area 3)

| # | Area | Current form | Breaks | Proposed form | Impact |
|---|---|---|---|---|---|
| 4 | 3 | `->` for result types and `=>` for match arms (`S.a => { }`, `success(v) => v`) | 5 | `->` for both; then `->` consistently means "produces" | medium (formatter can rewrite) |
| 5 | 3 | Two `match`es. A value match is a statement only: `return match s { S.a => N(1) ... }` fails with `failure.outcome_pattern_invalid` (*verified*). An outcome match is an expression over a call, with arms `success(v)` and `failure F` | 1, 6 | One `match`: the subject's type decides which patterns apply, and every match works as a statement or an expression. Outcome arms drop the `failure` prefix, since failure names are already unique types: `TodoNotFound -> reject X` | medium |
| 6 | 3 | A variant's data is bound four ways: `some(x)`, `success(x)`, `P.paid { receipt_id, paid_at }`, *candidate* `on TodoEvent.completed(payload)` | 1, 5 | `Variant(name)` binds the whole payload everywhere: `PaymentOutcome.paid(payment) -> payment.receipt_id`. This removes the one comma-in-braces pattern | medium |
| 7 | 3 | `var x = e` declares an **immutable** binding; `var` means mutable in JavaScript, C# and Kotlin | 7 | `let x: T = e`, `let mut x: T = e` (Rust's exact pair, which `var mut` already mirrors) | high (every body; mechanical rewrite) |
| 8 | 3 | `if (x == N(1)) {` and `if x == N(1) {` both compile and the formatter keeps the parentheses (*verified*); the seed uses one, outcome-sequencing the other | 1 | No parentheses; the formatter removes them | low |
| 9 | 3 | `-> N fails F` still parses after the arrow ("Keep reading the former position", `parser.rs`) as well as `fails F -> N` (*verified*) | 1 | Reject the after-arrow form | low |
| 10 | 3 | Callable settings are split. `consistency:`, `freshness:` and `guard:` sit between the parameters and `fails`; `policy { }` is the first item inside the body. Formatted output puts `freshness:` and `fails` at different indents (`entities/todo.jadpo`) | 1, 3 | Signature on the header line (`action name(params) fails A, B -> R {`); every setting at the top of the body, one per line: `consistency: atomic`, `freshness: ...`, `policy { }`, then statements. The same rule covers tests (#71) | medium |
| 11 | 3 | `syntax.md` §6.1 still teaches `if due_at exists { }` and `some date => schedule(date)`; the compiler rejects both (*verified*) | 1 | Remove them from the docs; absence is tested with `!= none` or `match` | low |

### 3.4 Failures (area 4)

| # | Area | Current form | Breaks | Proposed form | Impact |
|---|---|---|---|---|---|
| 12 | 4 | Mapping a condition to a failure has five spellings: `missing: TodoNotFound`, `conflict Account.handle: AccountHandleTaken`, `inactive: UserDisabled`, `maps provider.rate_limited -> ReminderTemporarilyUnavailable`, and `failure LookupMissing => reject LookupRejected`; area 2 Q1 adds `invalid: F` | 1, 3, 5 | Always `condition: Failure` inside a block. For several conflicts: `conflict { Account.handle: AccountHandleTaken  Account.tenant_owner: AccountTenantOwnerTaken  other: AccountMutationConflict }`. For services: `failures { provider.rate_limited: ReminderTemporarilyUnavailable ... }` | medium |
| 13 | 4 | `failure TodoNotFound { code: "todo_not_found" ... }` repeats the name in snake case, in every failure in the repo | 11 | Derive `code` from the name; write `code:` only to keep the old public code after a rename (the approval check already flags public-code changes) | low |
| 14 | 4 | `public { }` in a failure means "sent to the client"; `public` also marks cross-domain visibility (`public action reserve_for_cart`) | 6 | `client { }` and `internal { }` | low |
| 15 | 4 | `maps provider.authentication -> fault Misconfigured`, but the same fault is bare in `fails ... Misconfigured, Unavailable, OutcomeUnknown` (`tests/compile/pass/170_checked_service_operation.jadpo`) | 1, 3 | Bare everywhere: `provider.authentication: Misconfigured` | low |

### 3.5 Data and entities (area 5)

| # | Area | Current form | Breaks | Proposed form | Impact |
|---|---|---|---|---|---|
| 16 | 5 | The identity is declared four ways: `id: Uuid identity` (policy-runtime), `identity: id` (migration), `id: Uuid { generated: identity }` plus `identity: id` (todo.jadpo), `persist X { identity: id }`. The word `identity` also means a credentials role (`identity: ServiceCredential.id`) and a delivery key | 1, 6, 11 | The identity is always the field `id`, generated as UUID v7 (area 2 Q13 already writes `entity Order { id: Uuid }`). Delete `identity:`, `generated: identity` and trailing `identity` | medium |
| 17 | 5 | Field facts are split between trailing words (`Text unique`, `Text index`, `generated { on: create }`) and the block (`{ role: ..., immutable: true }`), and some fields use both: `created_at: Instant { policy {} } generated { on: create }` | 3 | One block after the type: `created_at: Instant { generated: create  policy: server_only }` (area 2 Q20 extended to entities) | medium |
| 18 | 5 | `generated` has two shapes: `{ generated: identity }` in the block and `generated { on: create }` trailing; `{ generated: create }` is rejected (*verified*) | 1, 3 | `generated: create` or `generated: create_or_change`, in the block | low |
| 19 | 5 | Uniqueness has four spellings: `handle: Text unique`, `persistence { unique: name }`, `persist X { unique: email }`, and the call-shaped `constraint tenant_owner: unique(tenant, owner_email)` (still compiles in persistence-seed) | 1, 3 | Single field: `{ unique: true }` (and `{ index: true }`). Compound: `unique tenant_owner { fields: [tenant, owner_email] }` inside `persistence`, the same shape as `transition delete { }` | medium |
| 20 | 5 | `references owner_id: User.id as: owner on_delete: restrict`: a keyword, two settings and the field's type repeated, all on one line, away from the field | 3, 11 | On the field: `owner_id: User.id { relationship: owner  on_delete: restrict }`. The field type `User.id` already says what is referenced, so `references` and `as` go | medium |
| 21 | 5 | `inverse todos: many Todo via: Todo.owner_id`: a `name:`, a bare `many`, the child named twice | 3, 11 | `inverse todos { via: Todo.owner_id  cardinality: many }`; `zero_or_one` replaces `optional` | low |
| 22 | 5 | `persistence { store: primary  role: authority }` is repeated in every entity with the only values used. `role` also means an access role (`role: TodoRole.owner`, `membership { role: role }`) | 6, 11, 12 | Default to the primary store; drop `role: authority`, since `cache` and `projection` already name the non-authoritative forms. Write `persistence { }` only for a second store | low |
| 23 | 5 | Legacy `type Customer = Object { }` plus `persist Customer { }` still compiles alongside `entity` (persistence-seed, jadpo-seed, module-seed) | 1, 12 | Remove `persist`; migrate those three examples to `entity` | medium |
| 24 | 5 | Receivers `self: ref` and `self: value`, where `value` is also a declaration keyword. The compiler rewrites them to `Todo.Ref` and `Todo`, but `self: Todo` itself is rejected (*verified*) | 6, 11 | Write the real type: `self: Todo.Ref`, `self: Todo` | low |
| 25 | 5 | A lifecycle has four grammars in one block: `initial: { deleted_at: none }`, `visible when deleted_at == none`, `transition delete { from: ...  set: { } }`, `purge after config.soft_delete_retention from deleted_at` | 3 | `visible: deleted_at == none`; `purge { after: config.soft_delete_retention  since: deleted_at }`; `transition delete { when: deleted_at == none  set: { } }`; `initial: { }` unchanged | low |
| 26 | 5 | `from` has six meanings: transition precondition (`from: deleted_at == none`), purge timestamp (`from deleted_at`), header wire name (`from "X-H"`), derived-type source (`Output from Todo`), derived-store source (`cache { from: x }`), *design* `write from job overdue_reminders` | 6 | Keep `from` for "built from" only (derived types, `cache`/`projection`); preconditions use `when:` (already used by `visible when` and the patch `when ... supplied`), purge uses `since:`, headers use `header:` (#42) | low |

### 3.6 Reads and writes (area 6)

| # | Area | Current form | Breaks | Proposed form | Impact |
|---|---|---|---|---|---|
| 27 | 6 | Cardinality is a bare word between the keyword and the entity: `query required Todo {`, `query page Todo -> TodoPage {`, `update required`, `delete required` | 3 | The selected successor: `query(Todo) { cardinality: required ... }`, and `update(Todo) { }`, `delete(Todo) { }` (exactly-one, so no cardinality) | high |
| 28 | 6 | Create puts its failure **outside** the braces, `create Todo { ... } conflict: F`; inside, it is read as a field (`T has no conflict field`, *verified*). Update and delete put `conflict:` inside | 1 | `create(Todo) { values: { ... }  conflict: F }` (the successor candidate) | medium |
| 29 | 6 | The left side of a predicate is a bare field and the right side a variable, so `where: id == id` (`Todo.by_id`, policy-runtime) reads as always true. The same bare-field rule applies in `active: status == UserStatus.active` and `visible when deleted_at == none`, while the delivery block writes `Todo.due_at` and the *design* policy `Todo.owner_id` | 7, 10 | Name the owner: `where: Todo.id == id`, `active: User.status == UserStatus.active`. One rule for every predicate | medium |
| 30 | 6 | `and` is both a boolean operator and a clause key. Page queries chain `and: status matches optional ...`; required queries reject both `where: a and b` (`and` is read as the operator, so the field is an unknown name) and `and:` (*verified*) | 1, 6 | Repeat `where:`, one condition per line, all of which must hold. `and` is only the operator | medium |
| 31 | 6 | Optional filters use `status matches optional input.status`, `due_at <= optional input.due_before` and `after: optional input.after`. `matches` duplicates `==`, and `optional` gains its sixth meaning (field setting, query cardinality, include cardinality, inverse cardinality, predicate marker, `after:` marker) | 1, 6, 7 | `where_present: Todo.status == input.status`, which applies only when the input is present; `after: input.after` (nullable under area 2 Q3) | medium |
| 32 | 6 | One line holds five settings: `include: todos into: UserTodos order_by: id asc limit: 100 offset: 0`; the parent writes `limit: 20 offset: 0`. In the *design* source, an include's own `where:` is not delimited from the parent's | 3 | `include todos { into: UserTodos  order_by: [Todo.id asc]  limit: 100 }`, one setting per line | medium |
| 33 | 6 | Include cardinality is a bare word: `include: owner required into: TodoOwner`, `include: profile optional into: X` | 3 | Inside the include block: `cardinality: required` or `cardinality: zero_or_one` | low |
| 34 | 6 | `cursor: TodoCursor(created_at, id)` looks like a constructor call, names a type, and repeats `order_by` | 5, 11 | `cursor: TodoCursor` (the successor candidate; its fields come from `order_by`) | low |
| 35 | 6 | `project: TodoView` is a verb that reads as a noun and clashes with the entity item `projection`. The migration copies `TodoView { id: todo.id  title: todo.title ... }` field by field three times | 7, 12 | With derived types, `TodoView(todo)` builds a derived type from its source, reusing `Type(value)`. In a page query: `items: TodoView` | medium |
| 36 | 6 | `query page Todo -> TodoPage {` uses the result-type arrow in the middle of an expression | 5 | `into: TodoPage` inside the block (successor), the same word `include` already uses | low |
| 37 | 6 | `freshness: authoritative` on every named query, and `consistency:` on actions, written even though only the default value is ever used | 11, 12 | Default to `authoritative` and `atomic`; write the setting only for the weaker or slower choice, which is what a reviewer needs to see | low |

### 3.7 HTTP (area 7)

| # | Area | Current form | Breaks | Proposed form | Impact |
|---|---|---|---|---|---|
| 38 | 7 | Path parameters are typed in a separate anonymous block: `route GET /todos/{todo_id} { path: { todo_id: Todo.id } ... }`, and the compiler checks the two match. The *design* source used `/:todo_id`, which now fails (`route.path_binding_extra`, *verified*) | 8, 11 | Type them in place: `GET /{todo_id: Todo.id} { ... }`, still read as `path.todo_id` | medium |
| 39 | 7 | `query` means three things: the route item `query: ListTodos` (the query string, read as `query.status`), the `query` declaration, and the `query required` expression | 6 | One `input:` per route. Its source follows the method: GET and DELETE read the query string, POST, PUT and PATCH the JSON body. The `query:` item goes | medium |
| 40 | 7 | `success: created` and `success: no_content` set the HTTP status, but `success` is also the outcome-arm keyword `success(value)` | 6, 7 | `status: created`, `status: no_content` (default `ok`) | low |
| 41 | 7 | `auth: none` and `auth: fresh` abbreviate a word spelled in full everywhere else (`authentication browser_session`) | 7 | `authentication: none`, `authentication: fresh` | low |
| 42 | 7 | `headers: { request_id: RequestId from "X-Request-Id" optional }`: an anonymous shape, a `from "..."` clause and a trailing `optional` (*verified*) | 3, 8 | Header fields on the route's named `Input`: `request_id: RequestId { header: "X-Request-Id"  optional: true }` | medium |
| 43 | 7 | Two behaviour forms: `run: f(...)` and an inline anonymous action, `action: fails F { ... }`, whose `fails` follows the colon | 8, 12 | `run:` only; every route calls a named action | low |

### 3.8 Identity (area 8)

| # | Area | Current form | Breaks | Proposed form | Impact |
|---|---|---|---|---|---|
| 44 | 8 | `principal` has five uses: the declaration keyword, `principal: Principal` in `application`, `principal: user` in a validator, `principal: ServiceCredential.service_id` in `credentials`, and the usual parameter name `principal: Principal.user` (beside `current_principal`) | 6 | Unnamed `principal { }` (selected); delete the application setting; validator `for: user`; credentials `service: ServiceCredential.service_id` | medium |
| 45 | 8 | Mapping blocks use `->` where every other block uses `name: value`: `claims { subject -> Principal.user.subject }`, `mappings { id -> Principal.user.user_id }` | 3, 5 | Fill the principal the way an object is built: `resolution user { authority: ...  active: ...  user_id: User.id  subject: User.authentication_subject  inactive: UserDisabled }`; claims `authentication_strength: claims.authentication_strength` | medium |
| 46 | 8 | `secret` has five meanings: the config flag `secret: true`, a validator key `secret: config.session_signing_key`, the service credential `secret: config.mail_api_key`, the fixture wrapper `secret("...")`, and *design* `secret: ... only_to` | 6 | Keep `secret: true` on config fields only; validators `signing_key:`, services `token:`; fixtures write the plain value (#73) | low |
| 47 | 8 | `mode: signed`, `mode: jwt`, `mode: api_key`, `mode: opaque` on validators: `mode` says nothing about what is chosen | 7 | `credential: signed_token`, `jwt`, `api_key`, `opaque_session` | low |
| 48 | 8 | The transport key picks the variant and its value is a detail: `transport { cookie: "todo_session" }` or `transport { bearer: authorization_header }`, where `authorization_header` is the only accepted value | 4, 11 | `transport { kind: cookie  cookie_name: "todo_session" }` or `transport { kind: bearer }` | low |
| 49 | 8 | `application TodoApplication { authentication { principal: Principal  revocation { ... } } }` is a wrapper whose only real content is the revocation limit | 12 | `application { revocation { mode: bounded  maximum_delay: 5m } }` in `app.jadpo`, beside the language version (area 1) | low |

### 3.9 Policy (area 9)

| # | Area | Current form | Breaks | Proposed form | Impact |
|---|---|---|---|---|---|
| 50 | 9 | An operation's policy can be written in two places: in the entity, `policy { operations { disable_user { UserRole.self: [update] } } }`, or at the top of the callable, `policy { ApplicationRole.operator: [invoke] }` | 1 | Only on the operation. Entity `policy { }` holds only the entity matrix | medium |
| 51 | 9 | An empty `policy {}` on a field silently means no client may read or write it (`authentication_subject: Text { policy {} }`) | 7 | Name it: `policy: server_only` | low |
| 52 | 9 | *Candidate* event policy mixes a setting with role entries in one block: `policy { scope: todo_id  TodoRole.owner: [invoke, read] }` | 3 | `scope:` beside the policy rather than in it, as `membership { scope: }` already does for entities | low |
| 53 | 9 | *Design-only* `policy TodoPolicy version "..." { default required ...  id: server_identity immutable ...  permit from job overdue_reminders }`: a second policy language of space-separated sentences, superseded by entity matrices | 1, 3 | Retire it from the golden design input | low (docs) |
| 54 | 9 | `enum UserRole { self }` and `UserRole.self: [read]` share the word with the `self` receiver | 6 | `UserRole.account_holder` (or any non-keyword) | low |

### 3.10 Configuration and time (area 10)

| # | Area | Current form | Breaks | Proposed form | Impact |
|---|---|---|---|---|---|
| 55 | 10 | `database_url: DatabaseUrl { binding: "DATABASE_URL" ... }`: every binding in the repo is the field name in capitals | 11 | `DATABASE_URL` by default; `binding:` only to differ | low |
| 56 | 10 | Durations are written `30d`, `5m` in config, jobs, deadlines and services, but `Duration("PT5M")` in code and fixtures. Each place rejects the other spelling (*verified*: config rejects `"PT5S"`, an expression rejects `5s`). Area 2 notes this | 1 | `5m` everywhere; ISO 8601 only on the JSON wire | medium |
| 57 | 10 | `locales { default: "en-GB"  supported: ["en-GB", "en-US"] }` uses text, but code uses the enum `Locale.en_gb` | 1, 4 | `default: en_gb`, `supported: [en_gb, en_us, fr_fr]` | low |
| 58 | 10 | `temporal.format(value, locale: Locale.en_gb, components: { day: numeric  month: long ... })` passes an anonymous object as an argument | 8 | A named type: `type ShortDate = TimeFormat { day: numeric ... }`, passed as `format: ShortDate` | low |
| 59 | 10 | Named standard-library arguments qualify enum values (`overlap: LocalOverlap.earlier`, `zone: Zone.europe_london`), but area 2 Q8 writes `rounding: half_even` | 1, 4 | Qualified in code (area 2 Q17: bare only in settings); correct the Q8 example to `rounding: Rounding.half_even` | low |

### 3.11 Services and health (area 11)

| # | Area | Current form | Breaks | Proposed form | Impact |
|---|---|---|---|---|---|
| 60 | 11 | An operation is a run of flat lines with no braces: `operation send_overdue_reminder` / `POST /v1/messages` / `input ReminderMessage` / `output ReminderReceipt`. In a route, `input CreateOrder` without a colon is a syntax error | 1, 2, 3 | `operation send_overdue_reminder { endpoint: POST /v1/messages  input: ReminderMessage  output: ReminderReceipt  idempotency { }  failures { } }` | medium |
| 61 | 11 | Sentence clauses: `request_key: input.idempotency_key equals header "Idempotency-Key"`, `receipt_key: response header "Idempotency-Key" equals request_key`, where `equals` duplicates `==` | 3, 5 | `idempotency { key: input.idempotency_key  header: "Idempotency-Key"  receipt: echoed_header }` | low |
| 62 | 11 | Call-shaped settings with commas: `retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)` | 3 | `retry { strategy: exponential  max_attempts: 3  max_elapsed: 30s  jitter: full }` | low |
| 63 | 11 | `import { file: ... version: ... sha256: ... }` reuses the module-import keyword for a provider contract | 6 | `contract { file: "reminder_mail.contract.json" ... }`, matching area 1's contract file beside the integration | low |
| 64 | 11 | `egress: mail.example.invalid:443` is an unquoted token beside `base_url: "https://mail.example.invalid"`, and mostly repeats it | 1, 11 | Derive egress from `base_url`; otherwise quote it | low |
| 65 | 11 | *Design-only* health: a hand-written liveness route calling a magic `liveness()`, the types `PublicHealth` and `HealthStatus`, and `readiness database { access: deployment_plane  timeout: 2s  required: true }`. None of it parses today | 12 | One generated `health { checks: [database]  timeout: 2s }` in `app.jadpo` (area 1 places health there); no route or types written by hand | medium |

### 3.12 Jobs and events (area 12)

| # | Area | Current form | Breaks | Proposed form | Impact |
|---|---|---|---|---|---|
| 66 | 12 | `job overdue_reminders every 15m {` puts a setting in the header | 3 | `schedule overdue_reminders { every: 15m  run: send_due_reminders() }` (area 1 sketch) | low |
| 67 | 12 | `concurrency: singleton` and `retry: next_schedule` are each required exactly once and each accepts exactly one value (`SYN_JOB_CLAUSE_VALUE_INVALID` otherwise) | 11, 12 | Defaults; add a setting when a second option exists | low |
| 68 | 12 | `delivery: reminder_v1 { selection: { ... }  hooks: { ... }  service: { ... }  authority: { ... }  completion: { ... } }`: a 45-line closed mini-language that restates a query (`order_by: Todo.due_at asc, Todo.id asc`, `limit: 500`), mixes `key: value { }`, and cannot run (`type.job_delivery_binding_not_implemented`, *verified*) | 1, 3, 12 | Delete it. A schedule calls an ordinary action whose query and integration call are ordinary code, as the design source already shows | low (unbuilt) |
| 69 | 12 | *Candidate* `attempt emit_event(TodoEvent.completed { ... })` is a bare lowercase intrinsic with no owner, beside `reject F { }` for failures | 10, 12 | `attempt emit TodoEvent.completed { todo_id: input.id }`, the statement form `reject` already uses | low |
| 70 | 12 | *Candidate* `component TodoManagement { modules: todo }` maps modules to owners, but area 1 makes domains the owners and removes modules | 12 | Drop `component`; `triggers:` refers to the domain | low |

### 3.13 Testing (area 13)

| # | Area | Current form | Breaks | Proposed form | Impact |
|---|---|---|---|---|---|
| 71 | 13 | `test "fixture clock advances" using controlled_runtime {` puts a setting in the header | 3 | `test "fixture clock advances" { fixture: controlled_runtime ... }`, a setting at the top of the body (the rule in #10) | low |
| 72 | 13 | `clock: fixed Instant("2026-01-15T12:00:00Z")`: a modifier word whose only value is `fixed`, and a constructor that literal typing makes unnecessary | 3, 11 | `clock: "2026-01-15T12:00:00Z"` | low |
| 73 | 13 | `api_key: secret("test-only-api-key")` restates that the config field is `secret: true` | 11 | `api_key: "test-only-api-key"` | low |
| 74 | 13 | `var r = call read_fixture_runtime()` and `var r = read_fixture_runtime()` both compile (*verified*) | 1 | Drop `call` | low |
| 75 | 13 | `advance clock by Duration("PT5M")` is a sentence statement | 3, 10 | `clock.advance(5m)`, owned by `clock` like `clock.now` | low |
| 76 | 13 | `service ReminderMail: fake { send_overdue_reminder => accept ReminderReceipt { ... }  send_overdue_reminder => provider.rate_limited }`: `keyword Name: value {`, `=>` meaning "next response", and an `accept` keyword | 2, 5 | `fake ReminderMail.send_overdue_reminder { responses: [provider.rate_limited, ReminderReceipt { accepted_at: "..." }] }` | low |

### 3.14 Tooling (area 14)

| # | Area | Current form | Breaks | Proposed form | Impact |
|---|---|---|---|---|---|
| 77 | 14 | The formatter leaves several items on one line (`type O = Object { a: Bool b: Bool }`, `validators { signed_user { mode: signed principal: user } }`, `limit: 20 offset: 0`) and keeps `if (...)` (*verified* with `jadpo fmt`) | 1, 3 | The formatter writes one item per line and removes redundant parentheses, so `fmt --check` enforces the rule | low |

### 3.15 Already in hand

Raised in areas 1 and 2; not repeated above.

| Point | Where |
|---|---|
| `module` and `import` removed; own-domain calls bare | Area 1, decided |
| `var return = x`, `var match = x`, `var set = x` compile (*verified*) | Area 1 reserved words, decided |
| `value`, `enum`, `input`, `output` keywords; inline `Object { }` (still compiles, *verified*) | Area 2 Q10, Q18, Q19, decided |
| `type OrderId = Uuid` with no block | Area 2 Q6 |
| `kind: NotFound` becomes `kind: not_found` | Area 2 Q17 |
| `optional` and `default` as trailing words | Area 2 Q20 |
| `Todo.status(TodoStatus.open)` narrowing in `create` | Area 2 Q2 |
| `var x: T` required; literals take the expected type | Area 2 Q15, Q16 |

## 4. Simplifications

Concepts and keywords that could go or merge. The first group removes
something; the second was considered and kept.

### Remove or merge

| Remove | Replaced by | Why |
|---|---|---|
| `persist`, `constraint ... unique(...)` | `entity` | A second storage model kept only for compatibility (#19, #23) |
| `identity:` item, `generated: identity`, trailing `identity` | The field `id` | Three spellings of one fact the name already states (#16) |
| `references ... as:`, `inverse ... many ... via:` sentences | Field and `inverse` settings blocks | The field type already names the target (#20, #21) |
| Delivery descriptor (`delivery: reminder_v1 { ... }`) | An ordinary action called by a schedule | A whole sub-language that duplicates queries and cannot run (#68) |
| Inline route `action:` | `run:` a named action | Two behaviour forms (#43) |
| Route `query:` and `path: { }` | One `input:`; typed path placeholders | Three input channels become one, plus the path (#38, #39) |
| `matches`, `and:` clause, `optional` as a predicate marker | Repeated `where:`; `where_present:` | Operators and words that duplicate `==`, `and` and `optional` (#30, #31) |
| `project:`, *design* `project x as T`, hand-written field copying | `TodoView(todo)` for derived types | Reuses construction (#35) |
| `failure` arm prefix; the separate outcome `match` | One `match` with type-directed patterns | One construct, one arrow (#4, #5) |
| `{ a, b }` payload patterns | `Variant(name)` | The only comma list in braces (#6) |
| `call`, `advance clock by`, `fixed`, `secret(...)` in tests | Plain calls, `clock.advance(5m)`, plain values | Test-only words with no meaning of their own (#72–75) |
| `component` (candidate) | Domains | Modules are gone, so there is nothing to map (#70) |
| `emit_event(...)` (candidate) | `emit Event.variant { }` | Mirrors `reject` (#69) |
| Settings with one legal value (`concurrency: singleton`, `retry: next_schedule`, `store: primary`, `role: authority`) and settings that always hold the default (`freshness: authoritative`, `consistency: atomic`) | Defaults | Ritual lines a reviewer learns to skip (#22, #37, #67) |
| Singleton names, `binding:` and failure `code:` | Derived | They restate a name (#2, #13, #55) |
| `claims { a -> b }` and `mappings { a -> b }` | Object-construction form | Reuses `field: expression` (#45) |
| `equals`, `exponential(...)`, `maps ... -> fault`, `fault` | `==`, settings blocks, a `failures { }` block | Service-only grammar (#12, #15, #61, #62) |
| *Design-only* `policy P version` language, `readiness`, `liveness()` | Entity matrices; generated `health { }` | Superseded or never built (#53, #65) |

### Considered and kept

| Kept | Why |
|---|---|
| Route `output:` (although `run:`'s result type implies it) | The same choice as `var x: T`: the reviewer reading the route sees the HTTP contract without opening the action |
| `fails A, B` as a comma list in the signature | It belongs to the signature, like the parameter list; settings use `[ ]` |
| `auth: none` on the route and `Access.public: [invoke]` on the action | Authentication and authorisation are separate gates; merging them would weaken the secure default |
| `event` as a keyword rather than `type X = Event` | Like `entity`, it carries more than a shape (per-variant policy, registration, delivery), and the `events/` folder follows the keyword. Its body should be identical to an `Enum` body |
| `attempt` alongside outcome `match` | Propagating and handling are different decisions |
| `var mut` (or `let mut`) | Mutation should stay conspicuous |
| Uppercase HTTP methods as keywords | They read as HTTP, and the convention is universal |

## 5. Top 10 by value

Ranked by clarity gained for the cost. Each combines related findings.

| Rank | Change | Findings | Why it ranks here | Cost |
|---|---|---|---|---|
| 1 | **One place for entity field facts.** The identity is `id`; `unique`, `index`, `generated`, the relationship and policy all go in the field's block; compound keys get named `unique name { fields: [...] }` | 16–20, 23 | Today the same fact has three or four spellings in one file (`todo.jadpo` versus `user.jadpo`), and every entity hits it | medium |
| 2 | **Delete the delivery descriptor** | 68 | Removes a whole closed sub-language that duplicates queries, for almost nothing: it was never built | low |
| 3 | **Qualify fields in predicates** (`where: Todo.id == id`) and stack `where:` lines | 29, 30, 31 | `where: id == id` reads as a bug to every reviewer; `and` stops meaning two things; `matches` and `optional` leave predicates | medium |
| 4 | **Finish the data-operation shape**: `query(Todo) { cardinality: ... }`, create's `values:` and `conflict:` inside the braces, named include blocks, `cursor: TodoCursor`, `into:` | 27, 28, 32–34, 36 | It completes a direction already chosen, and removes five one-off spellings at once | high |
| 5 | **One failure-mapping form**: `condition: Failure` in a block, everywhere | 12, 15 | Five spellings become one; services, queries, authentication and outcomes read alike | medium |
| 6 | **One route input**: `input:` by method, typed path placeholders, `status:` instead of `success:` | 38–40, 42 | Removes `path:`, `query:` and `headers:` (two of them anonymous shapes) and two overloaded keywords | medium |
| 7 | **One `match`**: `->` arms, `Variant(name)` binding, no `failure` prefix | 4–6 | Every match reads the same; no comma braces; one arrow | medium |
| 8 | **Integration operations as blocks** (`integration`, `operation x { endpoint: ... }`, `idempotency { }`, `retry { }`, `contract { }`) | 1, 60–63 | The least consistent declaration in the language becomes ordinary settings | medium |
| 9 | **Delete ritual and repeated names**: singleton names, `binding:`, failure `code:`, one-value settings, default freshness and consistency | 2, 13, 22, 37, 55, 67 | Low cost, and each removes a line reviewers learn to skip | low |
| 10 | **One meaning per word**: `from`, `optional`, `principal`, `secret`, `public`, `self` | 14, 24, 26, 44, 46, 54 | Each word currently has three to six meanings that an agent must tell apart by position | low to medium |

Not in the top 10 but worth deciding in area 3: `var` to `let` (#7). It is
high clarity but touches every body, so it is cheapest if done together with
the `var x: T` rewrite that area 2 Q16 already requires.
