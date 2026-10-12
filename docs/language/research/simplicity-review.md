# Simplicity review of the whole language

Independent review, 2026-10-12, of areas 1 to 14 as they would be with every
decided row and every open recommendation adopted. Nothing here is decided.

Written before Richard's revision that fixed options are written `Type.value`
(`kind: FailureKind.not_found`, `on_delete: OnDelete.cascade`,
`generated: Generated.on_create`); the cheat sheet below still shows the older
bare form for those settings.

## 1. Concept count: about 290

About 45 of them exist for one narrow case (marked †).

| Group | Count | Contents |
|---|---|---|
| Layout (area 1, 7) | 36 | 4 root files; root entries `domains shared database translations tests build api† acceptances† seeds/† .env.local`; 12 domain folders; 9 placement rules incl. "beside its user" and promotion |
| Keywords | 22 | `type entity function action query library† routes schedule subscriber integration fixture test application principal locales† authentication policy config migration seed† acceptance public` |
| Built-in types | 34 | 6 representation, `Email Url IpAddress`, 7 time, `List Set Map Page PageCursor`, bases `Object Enum Input Output Failure Event TimeFormat†`, `Unit PresentationText PostgresUrl† RedisUrl† Response Principal` |
| Type rules (2) | 20 | no aliases, 8 constraints (`format`†, `version`†), field types, widening, `?`/`none`, `optional`/`default`, `from` with pick/omit/optional, literal typing, constructing validates, range-proof `attempt` exemption†, signature rule, structural `==` |
| Logic (3) | 24 | `var`, `var mut`, `return`, `if ()`, `for`, `match` (6 patterns, 4 arm forms), `attempt`, `reject`, `emit`, `fails`; lambda†, word operators, `[ ]`, `///`; same-type operands, non-zero divisor†, recursion, purity table, unused/shadowing |
| Failures (4) | 12 | 15 kinds; `message code† client internal`; 5 mapping conditions; cause chain; fault vs defect; concealment; implicit `<Entity>NotFound`† |
| Data (5) | 20 | implicit `id`; `store`; 5 field settings; `unique`; `indexes`†; 8 lifecycle keys; `expiry`†; caches and `keep`; `membership`; stores |
| Reads/writes (6) | 22 | 8 query clauses (incl. `where_present`†), result type sets cardinality, `in`; 5 generated operations, `set patch when_supplied† conflict`, counter form†, projection; no reads in loops, one store per action, automatic rerun, caching only for route reads |
| HTTP (7) | 14 | `routes prefix apis† api†`; typed placeholders; `input header† output status deadline run`; GET runs a query; webhook API†; API record; acceptances |
| Identity (8) | 16 | `principal`; 3 built-in fields; `revocation_delay`†; 4 sign-in methods; `current_principal`; `none fresh† X.signature†`; per-API methods†; `sessions.*`†; service-key operations† |
| Policy (9) | 12 | role enums; `Access.*`; owner `role`; `membership`; 4 effects; `scope`†; `tenant`; `tenant: none`†; field `policy`; `server_only`†; `actions`; always on |
| Config/time (10) | 14 | `secret default binding† test†`; `config.x`; `clock.now`; durations; ~20 `temporal` functions; `Dst`†; formats; `locales`; translations†; `PresentationText` |
| Integrations (11) | 16 | `contract`; `base_url` (with `live/sandbox`†); `version_header`†; 3 credential schemes; `timeout retry`; 5 operation keys (incl. `idempotency`†); `webhooks`†; call-before-write; one changing call per action |
| Jobs/events (12) | 9 | `every run Event emit on`; implicit `event` name; `retry`; retry by kind; no fixed delivery order |
| Tests (13) | 14 | `test "…"`; 3 settings; `assert`; `assert … fails`; `request {}`; `Response`; `.run()`†; `clock.advance`†; `Principal.user(u)`†; `.requests`†; test kinds |
| Migrations (5) | 4 | `migration { backfill retire }`; drop; acceptance |

## 2. Merge or cut

1. **Placement by kind, no promotion** (A1 sharing and promotion). Every
   declaration goes in its kind's folder from the start; cross-domain sharing
   is `public` only; generic code is an ordinary domain (`domains/money/`).
   Lost: single-use types beside their user; more files. Gain: placement
   depends only on the declaration's kind.
2. **One `application { }`** (A1 root, A8, A9 tenancy, A10 locales). Five
   singletons in three files become `application { stores: … principal: …
   sign_in: … tenant: Company locales: [en_gb] }`. Lost: a separate
   `policy.jadpo` (policy is already protected by approval, not by file).
3. **Libraries are files** (A3 calling): `libraries/rounding.jadpo` holds
   plain `function`s with no `library { }` wrapper.
4. **Seeds are fixtures** (A5 seeds, A13): `jadpo seed alice_with_todos`.
5. **One caller channel** (A8 who is calling, A9, A12): policy checks a caller
   parameter; an action without one runs as the application:
   `action complete_todo(id: Todo.id, caller: Principal.user)`.
6. **`Output` merges into `Object`** (A2 one form); non-`Input` derived types
   may only `pick`.
7. **One `unique` form** (A5): only `unique: { email: [email] }`.
8. **One not-found rule** (A6): `missing:` optional, defaulting to
   `<Entity>NotFound`, as `Todo.get` already does.
9. **`attempt` decided by the target type** (A4 Q6): any computed construction
   into a type with rules is fallible, rather than a range proof deciding.
10. **Name trigger data in brackets** (A12, A3 Q9): `on: TodoCompleted(done)`,
    no magic `event` name.
11. **Cut `format: email`** (A2): write `type ContactEmail = Email {}`.
12. **Built-in time formats only** (A10 Q7): drop the `TimeFormat` base.
13. **Rename `deadline:` to `timeout:`** (A7, A11): one word for a time limit.

## 3. Inconsistencies in form

- **One header, six bodies.** `type X = Base {}` hides six block grammars;
  failures and events get their own folders yet use `type`, against A5's row
  "a declaration with its own folder keeps its keyword".
- **Triggers.** Routes bind `input`, placeholders and `current_principal`
  implicitly, subscribers bind `event`, schedules bind nothing; `routes Todos`
  is plural UpperCamel, `schedule overdue_reminders` is neither.
- **Contradictions between areas.** Provider calls: A3 Q15 runs them after
  commit, A11 Q7 calls first, A6 justifies reruns by "no outside call before
  commit". Schedule identity: no caller (A8), a service principal (A9), the
  application (A12 Q8). `clock.now` in a query (A6 filters) where A10 forbids
  it. A8 says policy-only actions take no caller, but A13 passes one.
- **One word, several shapes.** `include:`, `unique`, `policy`,
  `authentication`, `from`; queries qualify fields (`Todo.deleted_at`),
  lifecycles do not (A5).
- **`Type(x)`** both validates (A2) and projects (`TodoView(changed)`, A6).
- **`request { method: … }`** (A13) is an anonymous object, banned by A2 Q19.
- **Root list** in A1 is missing `api/` and `acceptances/` (A7).
- **Stale fragments agents will copy:** inline `attempt query required` (A4,
  A9) contradicts A6; `OrderEvent.placed(order) -> attempt postmark.send(…)`
  (A1) contradicts A12; `stripe.signature` (A7, A8) vs `StripeV2024.signature`
  (A11); a hand-written `/health/live` (A8) contradicts A7 Q6; `auth: none`
  (A9).

## 4. One-page test

Today the syntax needs about three dense pages. After the cuts it fits on one
(illustrative, compressed onto fewer lines than the formatter would use):

```jadpo fragment
// domains/<area>/<kind>/<name>.jadpo (+ .test.jadpo); root: app.jadpo config.jadpo database/ tests/
type Price     = Decimal { scale: 2  min: 0.00 }
type Money     = Object { amount: Price  currency: Currency }
type NewTodo   = Input { title: Todo.title  due_at: DueAt { optional: true } }
type TodoView  = Object from Todo { pick: [id, title, due_at] }
type TodoNotFound = Failure { kind: not_found  client: { todo_id: Todo.id } }
public type TodoCompleted = Event { todo_id: Todo.id }

entity Todo {
    owner_id: User.id { on_delete: cascade  role: TodoRole.owner }
    title: TodoTitle
    deleted_at: Instant?
    created_at: Instant { generated: create }
    unique: { title_per_owner: [owner_id, title] }
    lifecycle: { visible: Todo.deleted_at == none
                 transitions: { delete: { set: { deleted_at: clock.now } } } }
    policy: { TodoRole.owner: [create, read, update, delete] }
}

function half_even(amount: Price) -> Price { ... }     // libraries/rounding.jadpo
action rename(id: Todo.id, t: TodoTitle, caller: Principal.user) fails TodoNotFound, TitleTaken, TooMany, Declined -> TodoView {
    var todo: Todo = attempt Todo.get(id)
    var mut n: Count = 0
    for line in todo.lines { if (line.done) { n = n + 1 } }
    var q: Quantity = attempt Quantity(n * 2) invalid: TooMany
    var paid: Receipt = match payments.charge(q) { success(r) -> r  CardDeclined -> reject Declined }
    var done: Todo = attempt Todo.update(todo) { set: { title: t }  conflict: { Todo.title_per_owner: TitleTaken } }
    emit TodoRenamed { todo_id: done.id  receipt: paid.id }
    return TodoView(done)
}

query open_todos(owner: User.id, f: ListTodos) -> Page<TodoView> {
    from: Todo
    where: Todo.owner_id == owner
    where_present: Todo.status == f.status
    order: [Todo.created_at desc]
    page: { size: f.page_size  after: f.after }
}

routes Todos {
    prefix: /todos
    GET /{id: Todo.id} { output: TodoView  run: todo_view(id) }
    POST / { input: NewTodo  output: TodoView  status: created  run: create_todo(input, current_principal.user) }
}
schedule overdue_reminders { every: 15m  run: queue_reminders() }
subscriber email_on_done { on: TodoCompleted(e)  run: send_done_email(e.todo_id) }

integration ReminderMail {
    contract: "reminder_mail.contract.json"
    base_url: "https://mail.example.com"
    credential: { scheme: bearer  token: config.mail_key }
    operations: { send: { endpoint: POST /v1/messages  input: Mail  output: Receipt
        failures: { invalid_recipient: RecipientRejected  timed_out: MailDelayed } } }
}

application {
    stores: { main: { engine: postgres  url: config.database_url } }
    principal: { user: { subject: User.auth_subject  active: User.status == UserStatus.active } }
    sign_in: { browser_session: { cookie: "s"  signing_key: config.session_key  origin: config.origin } }
    tenant: Company
    locales: [en_gb, fr_fr]
}
config { database_url: PostgresUrl  session_key: SigningKey { secret: true } }

fixture alice() -> Principal.user { ... }
test "a past due date is rejected" {
    clock: "2026-01-15T12:00:00Z"
    fakes: { ReminderMail.send: [rate_limited] }
    assert create_todo(NewTodo { title: "x"  due_at: "2026-01-14T00:00:00Z" }, alice()) fails DueDateInPast
}
```

## 5. Top 5 recommendations (none decided)

1. Fix the contradictions and stale fragments in section 3 first: doc edits
   only, and agents learn the language from these pages.
2. One caller channel (#5): settles A8 Q5, A9's schedule identity and A12 Q8
   together.
3. Merges #2 to #4: six keywords and three root entries go, at low cost.
4. Placement by kind (#1): the biggest structural cut, reversing part of a
   decision already made in area 1.
5. One syntax pass for #6 to #13.
