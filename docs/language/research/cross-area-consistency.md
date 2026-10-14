# Cross-area consistency audit (areas 1–14)

Audit of `docs/language/01-*.md` to `14-*.md` on 2026-10-12, against every
decided rule (Decisions rows with a date, *Decided* open questions) plus the
setting-options rule Richard added on 2026-10-12 (below). `::: today` boxes,
`hidden` blocks and `## Errors` examples are exempt. References are
`NN:line` where `NN` is the area number (`01` = `01-project-layout-and-modules.md`,
`06` = `06-reads-and-writes.md`, and so on). Nothing on the pages was changed.

Status of the pages: areas 1–5 `decided`; areas 6–14 `in-review` (only their
"Carried" rows are binding).

**New decided rule (Richard, 2026-10-12), replacing area 2's "bare
lower_snake_case names in settings":** every setting with a fixed set of
options is written as an enum value `Type.value`, the same form as in code. A
bare word in a setting is always a name the author declared (`store: cache`,
`api: admin`). Option values read as values or moments, not verbs. This
supersedes the dated rows 02:1715, 04:1191, 04:1212, 05:2888, 08:2460,
09:1296, 10:1036 and 11:1583, and the text of 02:1666–1672 (question 17) and
04:127; those rows need a superseding note rather than a silent edit.

## 1. Decided-rule violations (mechanical fixes)

### 1a. Proposed code and prose that contradict a decided rule

| # | Where | Problem | Rule broken | Exact replacement |
|---|---|---|---|---|
| M1 | 01:911–913 | `on: OrderEvent.placed(order) -> attempt postmark.send(confirmation_for(order))`: a subscriber calls an integration directly, and `attempt` sits inside a setting value | Triggers call actions (01:990); `attempt` only at statement start (03:1936) | `subscriber send_order_confirmation {` / `    on: OrderPlaced` / `    run: send_order_confirmation_email(event)` / `}` |
| M2 | 02:369 | Note says "Question 20 proposes moving `optional` and `default` into a settings block" | Q20 decided (02:1718) | "…today's spelling is collapsed below. Decided (question 20): `optional` and `default` go in a settings block after the field's type." |
| M3 | 02:375–389 | `some(subtitle) => {` / `none => {`, block arms | `->` arms (03:1926) | `function headline(listing: Listing) -> ProductName {` / `    return match listing.subtitle {` / `        some(subtitle) -> subtitle` / `        none -> listing.name` / `    }` / `}` |
| M4 | 02:424–431 | `name: ProductName optional`, `page_size: PageSize default 25`, `subtitle: ProductName? default none` | Field settings in a block, one per line (02:1718) | Replace with the block at 02:1471–1485 verbatim (`name: ProductName {` / `optional: true` / `}` …) |
| M5 | 02:459–466 | Bullets describe trailing `optional`/`default 25` and say leaving a field out "is different from sending `none`" | Q20 (02:1718) and Q3 (02:1701: reading an optional field gives `T?`; only `patch:` keeps the distinction) | "- `optional: true` lets the request leave the field out; reading it gives `T?`, and a field left out reads as `none`. Only an update's `patch:` tells "left out" from "sent as `none`" (area 6). - `default: 25` fills in a value when the request leaves the field out, checked against the field's rules while compiling. - `default: none` fills in `none` for a nullable field." |
| M6 | 02:527–539 | `=>` arms, `PaymentOutcome.paid { receipt_id, paid_at }` braces-binding, `OutcomeLabel("pending")` constructor on a literal | `->` arms; `Variant(data)` binding (03:1934); literal typing (02:1713); braces never hold comma lists (02:1719) | `function label(outcome: PaymentOutcome) -> OutcomeLabel {` / `    return match outcome {` / `        PaymentOutcome.pending -> "pending"` / `        PaymentOutcome.paid -> "paid"` / `        PaymentOutcome.declined -> "declined"` / `    }` / `}` (03:407: a bare variant pattern matches a variant whose data the arm does not read) |
| M7 | 02:1258–1261 | `code: "quantity_out_of_range"` restates the default | Code defaults to the name; `code:` only to differ (04:1197) | Delete the `code:` line |
| M8 | 02:1462–1465 | `code: "todo_not_found"` restates the default | as M7 | Delete the `code:` line |
| M9 | 02:1302–1304 | `var nickname` is never used | Unused variable is an error (03:1929) | `action update_profile(request: UpdateProfile) -> Nickname? {` / `    var nickname: Nickname? = request.nickname    // left out reads as none` / `    return nickname` / `}` |
| M10 | 03:52 | `attempt Todo.mark_done(todo)`: a written action called through the entity | Written actions called by name; generated operations are get/create/update/delete (01:1009, 05:2908) | `var done: Todo = Todo.update(todo) {` / `    set: {` / `        status: TodoStatus.done` / `    }` / `}` / `return TodoView(done)` (drop the `var view` line; `TodoView(x)` matches 06:426, see C14) |
| M11 | 03:229 | Statement table: `if condition { } else { }` | Conditions in brackets, `else if` (03:1930) | `` `if (condition) { } else if (condition) { } else { }` `` |
| M12 | 03:311–313 | Operator table: "`/` always gives `Decimal`"; `<` on "two `Text` values"; `+`/`-` on any numbers | Operand types and division (03:1931–1932) | Row 2: "`*` and `%` may mix named types; `/` needs the same named type: `Int` drops the remainder, `Decimal` is exact (question 7)". Row 3: "the same named type after widening, or a literal; `+` also joins two `Text` values". Row 4: "two numbers or two time values of the same named type; text has no order (question 6)" |
| M13 | 03:545 | `return attempt Todo.by_id(todo_id)`: a query called through the entity | Queries live in `queries/` and are called by name (01:1009) | `return attempt todo_by_id(todo_id)                    // a return` |
| M14 | 03:1872 | "(`let`, assignment, `return` or on its own)" | `var`, not `let` (03:1927) | "(`var`, assignment, `return` or on its own)" |
| M15 | 03:1909–1910 | `on: ImportEvent.row_received(row) -> attempt import_product(row)`; `emit OrderEvent.placed(order)` | `attempt` placement (03:1936); triggers call actions | "`subscriber import_row { on: RowReceived  run: import_product(event) }`; `emit OrderPlaced { order_id: order.id }` in the action" |
| M16 | 03:1762 | "`var mut` already mirrors Rust's `var mut`" (factual slip) | — | "mirrors Rust's `let mut`" |
| M17 | 04:32 | Banner says "In review … areas 1 to 3 and this page's recommendations"; page is decided | Status `decided` | "> Decided on 2026-10-12. Code on this page shows the language after the rework, applying the decisions from areas 1 to 4. Today's compiled syntax is collapsed beneath each section that changes." |
| M18 | 04:41 | "Declared by: you, with `failure`" | `type X = Failure` (04:1209) | "you, as `type X = Failure`" |
| M19 | 04:127–129 | "The 'raised by' column is question 3's recommendation" | Q3 decided (04:1199) | "The 'raised by' column is decided (question 3); today every kind can still be declared and rejected." |
| M20 | 04:414–419 | Inline `attempt query required Todo { … }` inside an action | Every query in `queries/`, one per file (01:1009, 05:2907) | `// a read that finds nothing: queries/todo_by_id.jadpo (area 6)` / `query todo_by_id(todo_id: Todo.id) fails TodoNotFound -> TodoView {` / `    from: Todo` / `    where: Todo.id == todo_id` / `    missing: TodoNotFound {` / `        todo_id: todo_id` / `    }` / `}` |
| M21 | 04:425–431 | `attempt create Account { … }` and an `other:` catch-all | Generated operation `Account.create` (05:2908); no catch-all is area 6 Q6 (alignment) | `var account: Account = attempt Account.create {` / `    set: {` / `        ...` / `    }` / `    conflict: {` / `        Account.handle: HandleTaken` / `    }` / `}` |
| M22 | 04:597–602 | Inline `attempt query required Todo { … }` | as M20 | `var todo: Todo = attempt Todo.get(todo_id)` (keep the two comment lines) |
| M23 | 06:306–328 | `query project_board` in `queries/project_board.jadpo` and `type ProjectBoard` in `types/project_board.jadpo`: two files with one name | Names unique across kinds so every file name is unique (01:770–772); a type used by one file lives in it (01:784) | Rename the type `ProjectBoardView`, delete the `// domains/planning/types/project_board.jadpo` line so it sits in the query file, and change the header to `-> ProjectBoardView` |
| M24 | 06:698–702 | A standalone `route GET /todos { … }` outside a group | Every route in a named group (07:1067, carried from 01) | `// in routes Todos, domains/planning/routes/todos.jadpo (area 7)` / `GET / {` / `    input: ListTodos` / `    output: Page<TodoView>` / `    run: todos_for_owner(current_principal.user.id, input)` / `}` (see C1, C18 for the call) |
| M25 | 07:175–179 | `status: TodoStatus { optional: true }` and `{ header: "X-Request-Tag"  optional: true }` on one line | Field settings one per line (02:1718) | `type ListTodos = Input {` / `    status: TodoStatus {` / `        optional: true` / `    }` / `    limit: PageSize {` / `        default: 25` / `    }` / `    request_tag: RequestTag {` / `        header: "X-Request-Tag"` / `        optional: true` / `    }` / `}` |
| M26 | 08:447 | Service entity in `domains/integrations/` | Domains named after a business area (01:733); `integrations` is a domain folder keyword (01:759) | `// domains/identity/entities/service.jadpo` |
| M27 | 09:204–209 | Inline `attempt query required Article { … }` | as M20 | `var article: Article = attempt Article.get(article_id)    // policy adds: and the caller holds a role in its company` |
| M28 | 11:76–85 with 11:320–321 | `ReminderMessage` and `ReminderReceipt` are declared in the integration file but also used by `actions/send_reminder_email.jadpo` | A second user in the domain forces promotion to `types/` (01:784–789) | Move them out with path comments: `// domains/planning/types/reminder_message.jadpo` and `// domains/planning/types/reminder_receipt.jadpo` |
| M29 | 12:99 | `overdue_todos(clock.now)` passes a raw `Instant` to a query | Signatures use named types built on `Instant` (02:1709) | `var cutoff: ReminderCutoff = ReminderCutoff(clock.now)` / `var todos: List<Todo> = overdue_todos(cutoff)` (the same shape as 10:294–295) |
| M30 | 13:149, 13:233, 13:235, 13:350 | `.requests.count`, `overdue_todos(alice).count`, `sent.count` | Lists are counted with `collection.count` (02:1702) | `assert collection.count(ReminderMail.send_reminder.requests) == 1`; `assert collection.count(overdue_todos(cutoff)) == 0` / `== 1` (argument per C18); `assert collection.count(sent) == 2` |
| M31 | 13:224 | Test for the query `overdue_todos` placed at `actions/overdue_todos.test.jadpo` | Tests sit beside their subject (01:993); queries live in `queries/` | `// domains/planning/queries/overdue_todos.test.jadpo` |
| M32 | 13:395 | File `fixtures/sample_todos.jadpo` holds `fixture alice_with_todos` | One primary declaration per file, named after it (01:764) | `// domains/planning/fixtures/alice_with_todos.jadpo` |
| M33 | 14:44 | "`jadpo api update` … Decided in area 7" | Area 7 Q10 is open | "Proposed in area 7 (question 10)" |
| M34 | 14:68 | "(decided in area 13)" | Area 13 Q8 is open | "(proposed in area 13, question 8)" |
| M35 | 14:605 | "Area 12 decided the states" | Area 12 is open | "Area 12 proposes the states" |
| M36 | 12:22 | Banner: "applies the decisions from areas 1 to 4" | Area 5 is decided too | "areas 1 to 5" |

### 1b. Compiled blocks outside `::: today` that contradict a decided rule

These compile today, so they cannot be rewritten in place. Wrap each in
`::: today` and, where useful, add the fragment given.

| # | Where | Problem | Fix |
|---|---|---|---|
| T1 | 01:52–63 | `identity: id`, `values/` folder | Wrap in `::: today` (section already carries a "Changing" note) |
| T2 | 02:127–143 | `type Price = Decimal { min: 0.0 }` without the required `scale:` (02:1706); `return Quantity(1)` / `return Price(0.0)` instead of literals (02:1713) | Wrap; add fragment `type Price = Decimal {` / `scale: 2` / `min: 0.00` / `}`, `return 1`, `return 0.00` |
| T3 | 02:305–328 | `identity: id` on `Customer` and `Supplier` | Wrap; fragment `entity Customer {` / `    id: Uuid` / `    email: Email` / `}` |
| T4 | 03:324–351 | Untyped `var` (02:1714); `Int / Int` assigned to `Ratio`; text ordering | Wrap; the fragment at 03:357–361 already shows the decided rules |

### 1c. Bare option values under the new `Type.value` rule

Type names chosen once, for use on every page:

| Setting | Enum type | Values used on the pages |
|---|---|---|
| failure `kind:` | `FailureKind` | `not_found`, `conflict`, `invalid_value`, `unavailable`, `internal_fault`, … |
| `on_delete:` | `OnDelete` | `cascade`; `restrict` → **`restricted`**, `set_null` → **`cleared`** (renamed: the old names are verbs) |
| `generated:` | `Generated` | `on_create`, `on_change` (coordinator's names; replace `create`, `create_or_change`) |
| `Uuid` `version:` | `UuidVersion` | `v4`, `v7` |
| `Text` `format:` | `TextFormat` | `email` |
| `rounding:` (standard library) | `Rounding` | `half_even`, `half_up`, `up` |
| store `engine:` | `Engine` | `postgres`, `redis` |
| migration field step | `FieldChange` | `retire` → **`retired`**, `drop` → **`dropped`** |
| route `status:` | `SuccessStatus` | `ok`, `created`, `accepted`, `no_content` |
| route `authentication:` | `RouteAuthentication` | `none`, `fresh` |
| API `authentication:` list | `AuthenticationMethod` | `browser_session`, `api_token`, `oidc`, `service_key` |
| acceptance `change:` | `ApiChange` | `output_field_removed`, … |
| grant effects | `Effect` | `create`, `read`, `update`, `delete` (verbs: see P15) |
| field `policy:` | `FieldAccess` | `server_only` |
| credential `scheme:` | `CredentialScheme` | `bearer`, `header`, `basic` |
| webhook `signature:` | `SignatureScheme` | `stripe`, `standard_webhooks`, `hmac_sha256` |
| test request `method:` | `HttpMethod` | `get`, `post`, `put`, `patch`, `delete` |
| time format components | `ComponentStyle` | `numeric`, `short`, `long` |
| index `kind:` (05:364 prose) | `IndexKind` | `full_text`, `json`, `partial`, `geographic` |
| principal strength (08:70 prose) | `Strength` | `primary`, `multi_factor` |
| enum-typed config default | the field's own enum | `LogLevel.info`, `Zone.europe_london` |

Every proposed example with a bare option value:

| # | Where | Today on the page | Replacement |
|---|---|---|---|
| E1 | 02:1259 | `kind: invalid_value` | `kind: FailureKind.invalid_value` |
| E2 | 02:1423, 02:1459 | `version: v4` | `version: UuidVersion.v4` |
| E3 | 02:1455 | `format: email` | `format: TextFormat.email` |
| E4 | 02:1463 | `kind: not_found             // was: kind: NotFound` | `kind: FailureKind.not_found  // was: kind: NotFound` |
| E5 | 02:1372, 03:360 | `rounding: half_even` | `rounding: Rounding.half_even` |
| E6 | 03:361 | `rounding: up` | `rounding: Rounding.up` |
| E7 | 03:1838 (prose) | `rounding: half_up` | `rounding: Rounding.half_up` |
| E8 | 02:1627 (prose) | `{ version: v4 }` | `{ version: UuidVersion.v4 }` |
| E9 | 02:1667–1672, 02:1715 (prose, decided text) | `version: v7`, `format: email`, `kind: not_found`; "bare name" rule | Superseding note; examples become `UuidVersion.v7`, `TextFormat.email`, `FailureKind.not_found` |
| E10 | 04:50, 05:492 | `kind: not_found` | `kind: FailureKind.not_found` |
| E11 | 04:58 | `kind: conflict` | `kind: FailureKind.conflict` |
| E12 | 04:151 | `kind: unavailable` | `kind: FailureKind.unavailable` |
| E13 | 04:156 | `kind: internal_fault` | `kind: FailureKind.internal_fault` |
| E14 | 04:127, 04:1191, 04:1212 (prose) | "kinds are lower_snake_case option names", "written by its name (`kind: not_found`)" | Superseding note: "written `FailureKind.not_found`" |
| E15 | 05:79, 10:163 | `engine: postgres` | `engine: Engine.postgres` |
| E16 | 05:83 | `engine: redis` | `engine: Engine.redis` |
| E17 | 05:2873–2874 (decided text) | `engine: postgres` | `engine: Engine.postgres` |
| E18 | 05:135, 05:255, 08:451, 09:72 | `on_delete: restrict` | `on_delete: OnDelete.restricted` |
| E19 | 05:258 | `on_delete: set_null` | `on_delete: OnDelete.cleared` |
| E20 | 09:93, 09:96, 09:173 | `on_delete: cascade` | `on_delete: OnDelete.cascade` |
| E21 | 05:245–249 (table) | `restrict`, `cascade`, `set_null` | `OnDelete.restricted`, `OnDelete.cascade`, `OnDelete.cleared` |
| E22 | 05:163 | `generated: create` | `generated: Generated.on_create` |
| E23 | 05:166 | `generated: create_or_change` | `generated: Generated.on_change` |
| E24 | 05:447 | `Todo.legacy_flag: retire` | `Todo.legacy_flag: FieldChange.retired` |
| E25 | 05:2888 (decided row) | "closed options are lower_snake_case names (`on_delete: restrict`)" | Superseding note: `OnDelete.restricted` |
| E26 | 07:235 | `status: created` | `status: SuccessStatus.created` |
| E27 | 07:403, 11:559 | `status: no_content` | `status: SuccessStatus.no_content` |
| E28 | 07:224–229 (table) | `ok`, `created`, `accepted`, `no_content` | `SuccessStatus.…` in the first column |
| E29 | 07:487 | `change: output_field_removed` | `change: ApiChange.output_field_removed` |
| E30 | 08:395 | `authentication: none` | `authentication: RouteAuthentication.none` |
| E31 | 08:401 | `authentication: fresh` | `authentication: RouteAuthentication.fresh` |
| E32 | 08:424 | `authentication: [browser_session, api_token]` | `authentication: [AuthenticationMethod.browser_session, AuthenticationMethod.api_token]` |
| E33 | 08:428 | `authentication: [service_key]` | `authentication: [AuthenticationMethod.service_key]` |
| E34 | 08:406–410, 08:2442 (prose) | `authentication: none` / `fresh` | `RouteAuthentication.none` / `.fresh` |
| E35 | 09:78, 09:107, 09:178 | `[create, read, update, delete]` | `[Effect.create, Effect.read, Effect.update, Effect.delete]` (pending P15) |
| E36 | 09:179 | `[read, update]` | `[Effect.read, Effect.update]` |
| E37 | 09:180 | `[read]` | `[Effect.read]` |
| E38 | 09:281 | `[read, update]` | `[Effect.read, Effect.update]` |
| E39 | 09:285, 09:1279 (prose) | `policy: server_only` | `policy: FieldAccess.server_only` |
| E40 | 10:119 | `default: info` | `default: LogLevel.info` |
| E41 | 10:122 | `default: europe_london` | `default: Zone.europe_london` |
| E42 | 10:385, 10:387 | `day: numeric`, `year: numeric` | `day: ComponentStyle.numeric`, `year: ComponentStyle.numeric` |
| E43 | 10:386 | `month: short` | `month: ComponentStyle.short` |
| E44 | 10:455–456 | `default: en_gb`, `supported: [en_gb, en_us, fr_fr]` | Keep bare: `locales { }` is where these names are declared (a bare word is a declared name). Confirm (P16) |
| E45 | 11:51 | `scheme: bearer` | `scheme: CredentialScheme.bearer` |
| E46 | 11:341–345 (table) | `bearer`, `header`, `basic` | `CredentialScheme.…` |
| E47 | 11:551, 11:1568 (prose) | `signature: stripe` | `signature: SignatureScheme.stripe` |
| E48 | 11:564–565 (prose) | `stripe`, `standard_webhooks`, `hmac_sha256` | `SignatureScheme.…` |
| E49 | 13:124 | `method: post` | `method: HttpMethod.post` |
| E50 | 06:121, 06:317, 06:656 | `order: [Todo.created_at desc]` / `asc` | Undecided (P17): keep `asc`/`desc` as reserved list keywords, or `SortDirection.descending` |
| E51 | 02:1182 | `optional: all` | Undecided (P17): keep `all` as a reserved word, or `optional: Fields.all` |
| E52 | 08:2460, 09:1296, 10:1036, 11:1583 (carried rows) | "closed options are lower_snake_case names" | Superseding note pointing at the new rule |
| E53 | 10:955–957 (consistency note) | "bare names are only for settings" | "every option is written `Type.value`, in settings and in code" |

Not changed (declared names, so bare is correct): `store: cache`, `api: admin`
/ `webhooks`, `scope: company_id`, `member:`, `role:`, `tenant: Company`, fake
response conditions such as `rate_limited` (13:346, declared in the
integration's `failures:` block), `on: ReminderDue`, field names in `pick:`
and `unique:`.

## 2. Conflicts between open proposals on different pages

| # | Question | Position A | Position B (and others) | Recommended single resolution | Why |
|---|---|---|---|---|---|
| C1 | How do actions get the caller? | 08 Q5 (08:2422–2427, 08:326–330): routes pass `current_principal` as a typed parameter; "actions, queries and functions receive the caller as a parameter"; policy "reads it from the request itself" | 09 carried 09:1231 and rule 09:19–21: policy never needs it passed and code never filters by owner; 13 Q10 (13:1055–1060) passes callers into actions *and* queries (`overdue_todos(alice)` 13:233, `complete_todo(todo.id, bob)` 13:201) | The runtime carries the caller for the whole operation and policy reads it from there. An action takes a `Principal.user` parameter only when its own logic needs the identity (setting `owner_id`); queries never take one. Tests set the operation's caller with an `as:` test setting (the same word route tests use), not by passing it | Direct-call tests (13:201) otherwise prove nothing about policy, because policy never sees a parameter |
| C2 | Who do schedules and subscribers act as? | 08:331: "has no caller" | 09:431: "a declared service principal with its own roles"; 12 Q8 (12:1062, 12:221): "the application itself, limited to its own domain" | 12 Q8: the application, no user; policy grants it through 09 Q4's `actions:` list. Delete 09:431–432's sentence | One answer, no new principal kind, and nothing has to declare a worker service |
| C3 | `Principal.user(user)` in tests | 13 Q10: tests, fixtures and seeds build callers with `Principal.user(user)` | 08: no such constructor; a caller "only comes from signing in" (13:191 says so of app code) | Keep it test-only (tests, fixtures, seeds), and add it to area 8's Who-is-calling section | Needed for C1's `as:` setting and for seeds |
| C4 | Route group file naming | 01:764 and 07:1067 (decided): file named after the group | 07 Q1 (07:1001–1008, 07:111–112): file name from the prefix's fixed segments | The group's name is the UpperCamelCase of the prefix's fixed segments (`/admin/orders` → `AdminOrders`), the compiler checks it, and the file is named after the group as decided | Both rules hold and every existing example already satisfies it; only 07 Q1's wording changes |
| C5 | What a route's `run:` calls | 01:990 decided "routes call actions"; 03:62 "Route `run:` one action / not a query (area 7)"; 07:1068 carried | 07 Q4 (07:275–277): `GET` calls a named query; 06 Q12 and 05:2822 (decided) "display reads from routes may use caches" | Adopt 07 Q4 and amend 01:990 to "routes call an action, or a query for `GET`" (P1) | Area 5's decided cache rule already presumes routes that read |
| C6 | Event delivery order | ASYNC-001 per-key FIFO, owner-approved 2026-10-04 (`docs/async-plan.md:51–57`, 164–167) | 12 Q6 (12:1050–1054, 12:301): no order; 13 Q9 (13:521) "subscribers run in order" in tests | Adopt 12 Q6 only with Richard's explicit reversal (P4). Note what goes with it: ASYNC-001 also blocks later items on the same key while one is `outcome_unknown`; without ordering a second reminder for the same todo can be sent. Tests may still deliver in a fixed order | Reverses an approved decision |
| C7 | Provider call inside an action | 11 Q7 (11:442–445, 11:1551–1554): call first, then the writes commit after it | 06:557 and 06 Q8: rerunning an action is safe "because an action calls no outside service before it commits"; 03:1904–1906 (decided text): a provider call "runs after the commit, never inside it" | Adopt 11 Q7, rewrite 06:556–557 to "safe because an action's one outside call carries an idempotency key, so a rerun repeats it harmlessly", and confirm the amendment to 03's text (P5) | 11 Q7 is the only way to record a receipt without an extra event per call |
| C8 | Acceptance and review mechanism | 07 Q10 (07:1053–1061): `api/main.api` record + one `acceptances/` file per breaking change, CODEOWNERS | 14 Q12 (14:597–603): acceptances in a host-protected folder, CI runs `check`; 01:1000–1001, 09:421–429, 10 Q1 "approval review"; 11:531 "approval record"; 14:45 `jadpo approval --before` report | One mechanism: `jadpo approval` writes the report; changes that need a person need an `acceptances/` file; the folder is protected on the git host. Call it "acceptance" everywhere and drop "approval record" (11:531, 11:1539) | 07 and 14 already agree; only names and the root list (P3) differ |
| C9 | Root `policy.jadpo` holding `actions:` | 09 Q4 (09:1266–1273): `actions: { domain.action: [Role] }` in root | 01:643, 01:991: root policy holds "application-wide policy, such as tenancy" | No conflict: action exceptions are application-wide. Keep 09 Q4 | — |
| C10 | Seeds and ids | 05 Q1 (05:2807–2808, decided): "only seeds and migration backfills may set one" | 05 Q13 (05:2868–2871, decided): seeds create data through actions, which never accept an id | Amend Q1 to "only migration backfills may set an id" (P8) | Both are dated decisions; Q13 is the later, more specific one |
| C11 | What a seed may call | 05:469–473: `planning.create_todo(...)` from `database/seeds/`, so only public actions | 13 Q4 and 13:418: a seed may call fixtures | Seeds call public fixtures (`planning.alice_with_todos()`); fixtures may call private actions | Keeps actions private and makes demo and test data share builders |
| C12 | Integration call naming | 11 Q2 (11:1528–1531): `ReminderMail.send_reminder(...)` | 04:528 `mail.send_reminder(todo)`; 01:912 `postmark.send(...)` | 11 Q2 | Matches `Todo.get`; lowercase reads as a domain or library |
| C13 | Webhook authentication reference | 11 Q11: `authentication: StripeV2024.signature` | 07:401, 08:434, 08:2384: `stripe.signature` | `StripeV2024.signature` | Names the integration version that reads the event (11:521–522) |
| C14 | Building an `Output` from its source | 06:426 `TodoView(changed)`; M10 needs the same | 02 (decided) defines no `Derived(source)` constructor | Allow `Derived(source)` for a pick-only derived type, filling the picked fields; record it as an area 2 amendment | Otherwise every route action builds views field by field |
| C15 | Route output must equal the action result | 07 Q3 (07:1016–1020): `output:` is exactly the action's result | 05:497 and 12:171 `complete_todo -> Todo` while 07:287 says `output: TodoView`; 07:183 `output: TodoPage` while 06:117 returns `Page<TodoView>` | Route-called actions return the `Output`: change 05:497 and 12:171 to `-> TodoView` with `return TodoView(todo)`; 07:183 → `output: Page<TodoView>` | 07 Q3 makes the mismatch a compile error |
| C16 | Event shape | 12 Q3/Q5: one `type X = Event` per event, `subscriber { on: X  run: action(event) }` | 01:690 `events/order_event.jadpo`; 01:911–913 and 03:1909–1910 families with `->` arms | 12 Q3/Q5 (fixes M1, M15; 01:690 → `order_placed.jadpo`) | — |
| C17 | Health routes | 07 Q6: generated only, no authored liveness | 08:394–398 authors `GET /health/live`; 10:220 "health checks are area 11"; 01:640 `app.jadpo` holds "health, readiness" | 07 Q6. 08:394 → a different public route (`GET /status/{code}`), 10:220 → "area 7", and 01:640 loses "health, readiness" (P12) | — |
| C18 | Hand-written owner filters | 06:117–121 `todos_for_owner(owner_id, …)` with `where: Todo.owner_id == owner_id`; 07:184, 06:701 call it with the caller | 09:19–21: "Code never … filters by owner or tenant itself" | Rename to `my_todos(filter: ListTodos)` with no owner parameter or owner `where:`; policy scopes it. Calls become `run: my_todos(input)` | Agents copy examples; the reference must not teach the pattern policy forbids |
| C19 | `apis` prefixes | 07:361 `main: { prefix: /v1 }` | 07:380 "`main`, at `/`"; 07:381 a group's prefix starts with its API's; 08:423 `prefix: /` | 07:361 → `prefix: /`, or move `/v1` to a `v1` API | With `/v1`, `routes Todos { prefix: /todos }` is invalid on its own page |
| C20 | Include syntax for children | 06 Q4 (06:312–320): a list include is a block with `via:`, `order:`, `limit:` | 05:308–310: `todos: Todo.project_id` short form for a list | 06 Q4; 05:308–310 → `todos: {` / `via: Todo.project_id` / `order: [Todo.created_at asc]` / `limit: 50` / `}` | 05 defers syntax to 06 |
| C21 | Bare or qualified fields in conditions | 06 Q2: `where: Todo.status == …` | 05 (decided) lifecycle: `visible: deleted_at == none`; 06:3163–3166 notes it | Inside an entity's own declaration fields are bare; in queries and writes they are qualified. Write that rule down in 06 Q2 | No ambiguity inside the entity; keeps the decided form |
| C22 | `empty:` patch mapping | 04:444 (decided page) lists `empty:` → `invalid_value`; 04 owns `failure.patch_empty_not_invalid_value` | 06 Q7: empty patch succeeds, `empty:` and that rule go | 06 Q7; area 4 drops the row and the rule moves to "removed" | RFC 7396 behaviour; area 4 only listed the condition |
| C23 | "Fixture" meaning | 13 Q2/Q4: fixtures are shared test data | 10:189 "Tests get their values from test fixtures"; 10:305 "through fixtures" | 13. 10:189 → "Tests take each field's `test:` value (area 13)"; 10:305–306 → "Tests set the clock with `clock:` and move it with `clock.advance` (area 13)" | — |
| C24 | Webhook key setting name | 10 consistency 10:948–950 and 08: `secret` only as the config flag; keys are `signing_key:` | 11:552 `secret: config.stripe_webhook_secret` | `signing_key: config.stripe_webhook_secret` | Same word as area 8 |
| C25 | Signature drift in shared examples | `create_todo`: 05:471 `(alice.id, "Buy milk")`, 07:236 `(input, current_principal.user)`, 08:313 `(input: CreateTodo, caller: Principal.user)`, 13:229 `(CreateTodo {…}, alice)` | `complete_todo`: 01, 03, 05, 07, 12 `(todo_id)`; 13:201, 13:440 `(todo.id, bob)` | After C1: `create_todo(input: CreateTodo, caller: Principal.user)` everywhere it sets `owner_id`; `complete_todo(todo_id)` everywhere, with the test caller from `as:` | — |
| C26 | Other example drift | `overdue_todos(clock.now)` 12:99 vs `overdue_todos(alice)` 13:233; `ReminderMail.send_reminder` (11) vs `send_overdue_reminder` (13:144, 149, 346, 349); `reserve_for_cart(cart_id)` 01:877 vs `(cart)` 03:175; `inventory.reserve_for_order(order)` 04:365 passes another domain's entity | — | Use the area 11/12/01 forms: `overdue_todos(cutoff)`, `send_reminder`, `reserve_for_cart(cart_id)`, `reserve_for_order(order.id)` | — |
| C27 | Batch reminders | 12 batch pattern (12:86–117): `queue_overdue_reminders()` | 01:658, 01:907 `send_due_reminders`; 10:293 `send_due_reminders()` | `queue_overdue_reminders` in 01; keep 10:293's name only if it is renamed to something that does not send (`find_due_reminders`) | 12:980 calls the one-action sender a design problem |
| C28 | Deployables and APIs | 01 Q5 (decided): deployables (API, worker, back-office) are declarations | 07 Q7: `apis: { }` in `application` | APIs group routes for OpenAPI, records and authentication; deployables choose what runs where. State that in 07 and carry deployable syntax to area 14 (see section 4) | Same words ("API", "back-office") on both sides |
| C29 | Users' domain | 01:645–649 and 05, 13: `User` in `planning` | 06:713 `identity.users_by_ids`; 09:350 `accounts.disable_user`; 08:447 `integrations` | `identity` for cross-domain examples (01:702 already lists it); keep `planning` for the golden app | — |
| C30 | Test kind names | 01:776–778 "pure, integration, route, schedule" | 13:102–112 renames "integration" to "action" | 13 (P13) | `integration` is now the keyword for third-party APIs |

## 3. Proposals or decided text that contradict a decided rule (for Richard)

| # | Proposal | Decided rule it breaks | What Richard needs to say | Recommendation |
|---|---|---|---|---|
| P1 | 07 Q4 and 07:282: `GET` calls a named query | 01:990 "Routes … call actions"; 03:62 table; carried 07:1068 | Amend the trigger rule | Yes: "an action, or a query for `GET`" (C5) |
| P2 | Route entries `GET / { … }` inside `routes Todos { }` (01:891–897, 07:99–106) | Colon rule 03:1942: every member inside braces is `name: value`; only declarations, statements and match arms are exempt | Are route entries an exemption, like match arms? | Yes: add "route entries" to the colon rule's exemptions; `GET /x: { }` reads worse and nothing else needs it |
| P3 | 07 Q10 and 14 Q12: root `api/` and `acceptances/` folders | 01:714–716: "Only these entries" in the root | Amend the root list | Add both |
| P4 | 12 Q6: no delivery order | ASYNC-001 per-key FIFO, approved 2026-10-04 | Reverse the 2026-10-04 choice? | Yes, with the `outcome_unknown` consequence stated (C6) |
| P5 | 11 Q7: provider call, then writes | 03:1904–1906 (decided Q15 text): a provider call "runs after the commit, never inside it" | Amend that sentence | Yes (C7) |
| P6 | 13 Q4 and 13:392: fixtures have no `fails` list but use `attempt` (13:398–401) | 04:1201, 04:233–237: `fails` lists exactly what can leave a callable | A stated exception for fixtures | Yes: any failure in a fixture fails the test; say so in area 4 |
| P7 | 10 config fields use `default:` (10:47, 10:60, 10:119) | 02:1701: `optional` and `default` only on `Input` fields | Is config exempt? | Yes: a setting's default is not an object field; area 2 already cited config as the model (02:1488–1494) |
| P8 | — (decided vs decided) 05 Q1 vs 05 Q13 | Seeds may set an `id` vs seeds only call actions | Pick one | Q13; Q1 becomes "migration backfills only" (C10) |
| P9 | — (decided vs decided) 01:642 `authentication.jadpo` holds "methods and their failures" | 04:1213: every failure in `failures/` | Pick one | 08 Q6's built-in `user_disabled` / `service_disabled` removes the failures from `authentication.jadpo`; edit 01:642 |
| P10 | — (decided vs decided) 04 Q3 counts `not_visible` among the eight runtime-only kinds (04:137) | 04 Q10: policy reports `not_found`, `not_visible` never used | Drop the kind? | Drop it: 14 kinds, 7 declarable, 7 runtime |
| P11 | — (decided vs decided) 02 Q1 writes `attempt Quantity(n) invalid: F` after the call | 04:1203: `condition: Failure` "inside the operation's block" | Confirm the trailing form for constructions | Resolved 2026-10-14: constructions use the block like every other operation, keyed by rule, with `invalid:` covering only a construction's rules (area 2, "Construction failures by rule"); built in chunk 2.5, and the trailing form is a syntax error |
| P12 | 07 Q6: health generated only | 01:640 (decided layout text): `app.jadpo` holds "health, readiness" | Edit area 1's text | Yes |
| P13 | 13:110–112 renames area 1's "integration" test kind to "action" | 01:776–778 and carried row 13:1067 | Accept the rename | Yes |
| P14 | 14 Q7: `check` reports affected tests but runs none | 01:813–814 (decided layout text): `check` reports domains "and running their tests" | Edit area 1's text | Yes, `jadpo test --affected` |
| P15 | New option rule says values are not verbs | Grant effects `create/read/update/delete` (09), HTTP methods (13:124), `OnDelete.restrict`/`set_null` | Are effects and HTTP methods exempt from "not verbs"? Accept `OnDelete.restricted` / `.cleared`? | Exempt effects and HTTP methods (their standard names are verbs); accept `restricted` and `cleared` |
| P16 | 10 Q8 locales as bare names | New option rule | Are `locales { }` values declared names? | Yes: bare inside `locales { }`, `Locale.en_gb` everywhere else |
| P17 | `asc`/`desc` in `order:` (06) and `optional: all` (02:1182, decided) | New option rule: a bare word in a setting is a declared name | Keywords or enum values? | Reserved keywords inside those two settings; `SortDirection.descending` would be noise on every query |
| P18 | 06 Q5: generated per-transition operations (`User.disable(user)`, 06:407); 06 Q6: compiler-provided `<Entity>NotFound` with no file | 01:1009, 05:2908: generated operations are get/create/update/delete; 04:1213: every failure in a file | Confirm both extensions | Yes to both; the compiler-provided failure appears in the failure catalogue |
| P19 | Example naming such as `CreateTodo` (type) beside `create_todo` (action), `PatchTodo`/`patch_todo`, `ImportTodos`/`import_todos` (07, 08, 13) | 01:768–772: names convert to file names and back, unique across kinds; `naming.folded_collision` (01:929) | Does folded uniqueness apply to a single-use type living in another file? (inference: yes, given `naming.folded_collision`) | Yes; name inputs for the data (`NewTodo`, `TodoChanges`), not the action |
| P20 | 10 Q5: rename `Time` to `ZonedTime` | 02:1709 decided signature rule names `Time` | Amend the decided row if adopted; 03:177, 10:330, 10:339 change too | — |

## 4. Carried items not addressed by their destination

| From → to | Item (source line) | Destination state |
|---|---|---|
| 1 → (none) | Deployable declarations (01:965–968, decided 01:1002): syntax never carried to any area | Unowned; carry to 7 or 14 |
| 5 → 6, 9, 10 | GDPR review (05:2801) | Every destination defers it (09:413–419, 10:509–514); no owner |
| 6 → 5 | `expiry:` on Redis entities (06:3190) | Area 5 decided an expiry is required but has no syntax |
| 6 → 7 | Write revision header and cookie (`jadpo-revision`), non-browser clients echoing it (06:3183) | Not in area 7 |
| 6 → 12 | Consistent snapshot read for large exports (06:3186) | Not in area 12 |
| 6 → 13 | Asserting how many records and queries a query costs (06:3187) | Not in area 13 |
| 7 → 1 | `api/` and `acceptances/` root folders (07:987) | Not in area 1 (P3) |
| 7 → 12 | Background work after `status: accepted` (07:991) | Area 12 never mentions it |
| 7 → 9, 1 | Policy weakening and new domains accepted with the shared form (07:989) | 09:428 says "listed for a person to accept" but not via the acceptance file |
| 8 → 7 | Generated `POST /auth/exchange` in the route inventory and OpenAPI; security schemes per route (08:2382) | Not in area 7 |
| 8 → 7 | Each API's accepted methods `authentication: [...]` (08:2383) | 07:356–370 example omits them; 07:382 mentions only a "default authentication" |
| 8 → 9 | Who may issue, list and revoke service keys (08:2386) | Not in area 9 |
| 8 → 10 | Key rotation for signing keys (08:2387) | Not in area 10 |
| 8 → 13 | A service caller in tests (08:2390, 13:986) | Not in area 13 |
| 8 → 5 | `lifecycle { }` ignored on `id: Uuid identity` (08:2392) | Not in 05's status list; moot once the rework lands |
| 9 → 6 | Generated query plans, indexes for policy conditions, optional PostgreSQL row-level security (09:1236) | Not in area 6 |
| 11 → 9 | Who may call an integration, replacing `external_effect` (11:1509) | Not in area 9 |
| 11 → 14 | `jadpo integration add` / `update`, contract diff, adapter conformance runs (11:1514) | Area 14's command table (14:29–49) has none of them |
| 13 → 7 | `request { }` form, `Response` type, its `status`, `code` and typed output (13:985) | Not in area 7 |
| 13 → 8 | `Principal.user(user)` and `as:` (13:986) | Not in area 8 (C3) |
| 13 → 10 | `test:` values on configuration fields (13:988) | Not in area 10; 10:189 still says fixtures (C23) |
| 13 → 11 | Checking a fake against the contract or sandbox (13:989) | Not in area 11 |
| 13 → 12 | `overdue_reminders.run()`, subscriber order and retries in tests (13:990) | Not in area 12 |
| 13 → 6 | Caches in tests once Redis exists (13:991) | Not in area 6 (carried backwards) |
| 4 → 9 | Data classification of internal fields for log redaction (04:1060) | Deferred to the unowned GDPR review |

## 5. Terminology drift

| Thing | Names used | Where | Recommendation |
|---|---|---|---|
| Third-party API | integration; service; external service; provider | Keyword `integration` (01, 11); page title "External services" (11:2, 11:19); rule IDs `semantic.service_*` (11 front matter); "services" in 01:144 | "Integration" everywhere; retitle area 11 "Integrations"; rename the rules `semantic.integration_*` in the rework. Keep "provider" for the company |
| Machine caller | service; service principal; worker | `Service` entity, `service_key` (08); "declared service principal" (09:431) | "Service" only for this |
| The signed-in party | principal; caller; `current_principal`; `Principal.user`; "acting as a user" | 08 throughout, 09, 13:187 | "Caller" in prose, `Principal` as the type, `current_principal` as the route value |
| What a principal's id field is called | `caller.id` (08:314); `current_principal.user_id` (06:701); `principal.user_id` (09:1194, today) | 06, 08 | `id` (08 Q2) |
| Scheduled work | schedule; job | Keyword `schedule`; page title "Jobs and events" (12:2, 12:16); 01:145 "jobs"; `toolchain.target_job_*` | Retitle area 12 "Schedules and events" |
| Declared data home | store; database | `stores:`, `main` (05); `database/` folder; "database transaction" (01, 06); "test database" (13) | "Store" for the declared thing; "database" for the folder and the engine's own database |
| Named read | query; read; display read | 06:95 "Every read is a named query"; 05, 06 "display read" | "Query" for the declaration, "read" for any reading call including `Todo.get` |
| Route audience | API; surface; deployable | 07 Q7 `apis:`; 07:1007 "surface"; 01 Q5 "deployables (API, worker, back-office)" | "API" for a route audience, "deployable" for a runnable unit (C28); drop "surface" |
| Human sign-off | approval review; approval check; approval record; acceptance; sign-off | 01:1000–1001, 09:1292, 10:977, 11:531, 07 Q10, 14:272 | "Approval report" for what `jadpo approval` writes; "acceptance" for the file a person writes (C8) |
| `fixture` | Test environment (today); shared test data | 10:189, 10:305 vs 13 Q2/Q4 | Data only (C23) |
| `secret` | Config flag; webhook key; credential | 10, 11:552, 08 `signing_key:` | Config flag only; `signing_key:` and `token:` elsewhere (C24) |
| Route authentication word | `auth:`; `authentication:` | 09:49, 09:357, 09:1232 vs 07, 08 | `authentication:` (08 Q8) |
| Webhook check reference | `stripe.signature`; `StripeV2024.signature` | 07:401, 08:434 vs 11 | C13 |
| `public` | Visible to other domains; `Access.public`; "public API record"; "public route" | 01, 09:47, 07:469, 08 | Keep `public` for domain visibility; "API record" and "unauthenticated route" elsewhere |
| Test kind | integration test; action test | 01:776 vs 13:102 | "Action test" (P13) |
| Time-in-zone type | `Time`; `ZonedTime` | 02, 03:177, 10 vs 10 Q5 | Follows P20 |
| `kind` | Failure kind; index kind; test kind (`--kind`) | 04, 05:364, 14:34 | Acceptable; the enum types (`FailureKind`, `IndexKind`) separate them |
| Event naming | `TodoEvent.completed` families; `TodoCompleted` | 01:690, 01:912, 03:1909–1910 vs 12 | `TodoCompleted` (C16) |
| Owner-domain name for users | `planning`; `identity`; `accounts`; `integrations` | C29 | C29 |

## 6. Summary for Richard

1. **Caller model** (areas 8, 9, 12, 13 disagree): the runtime carries the caller and policy reads it; actions take a caller parameter only when their logic needs it; schedules and subscribers run as the application; tests set the caller with `as:` (C1–C3).
2. **Routes calling queries** amends your area 1 rule "routes call actions": confirm "an action, or a query for `GET`" (P1).
3. **Event order** reverses your 2026-10-04 ASYNC-001 per-key ordering; confirm, knowing a second reminder can go out while an earlier one is `outcome_unknown` (P4).
4. **Provider call, then writes** (area 11) amends area 3's decided "provider calls run after the commit": confirm (P5).
5. **Root folders and one acceptance mechanism**: add `api/` and `acceptances/` to area 1's fixed root, and use "acceptance" and "approval report" as the only two words (P3, C8).
6. **Colon rule and route entries**: confirm `GET /x { }` inside `routes X { }` is an exemption like match arms (P2).
7. **New `Type.value` option rule**: confirm `OnDelete.restricted` / `.cleared`, verbs allowed for effects and HTTP methods, bare locale names inside `locales { }`, and `asc`/`desc`/`all` as keywords (P15–P17). Table 1c lists 53 places to change.
8. **Seeds**: only migration backfills set ids; seeds call public fixtures rather than private actions (P8, C10–C11).
9. **Folded names**: confirm `CreateTodo` beside `create_todo` is a collision, so inputs are named for the data (`NewTodo`) (P19).
10. **No hand-written owner filters in examples**: rename `todos_for_owner` to `my_todos` with policy doing the scoping, so the reference never teaches what area 9 forbids (C18).
