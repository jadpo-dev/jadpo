---
title: Project layout and modules
area: 1
status: decided
checked-with: this checkout's jadpo on every gate run
---

# Project layout and modules
<!-- topic: project-layout-and-modules -->

How a Jadpo project is laid out on disk, how one file sees declarations from
another, and the naming rules every name follows.

## Project layout
<!-- topic: project-layout -->

A project is the folder holding `app.jadpo`. The root holds a fixed set of
files and folders, and all application code lives in business areas
(domains) under `domains/`, one level deep. Every domain holds the same
fixed folders, named after the declarations they hold (`entities/`,
`actions/`, `queries/` and so on), with one primary declaration per file,
named after it, first in the file, and nothing else in the file that it
doesn't use. A file's path follows from its domain and what it declares.
[Decided layout](#decided-layout) gives the full rules.

A single file, or a folder with no `app.jadpo` above it, is checked as loose
files: one implicit domain with the layout rules off.

## Source files
<!-- topic: source-files -->

File order and declaration order never matter: a declaration can refer to
one defined later or in another file. Names are unique across the whole
project, so a second `Count` in another file is an error (see
`semantic.duplicate_declaration` below). The one exception: a private
function, type or constant needs a name unique only within its domain (see
[Files and names](#files-and-names)).

## Domains instead of modules
<!-- topic: domains-instead-of-modules -->

There are no modules. A domain owns everything in it and is the visibility
boundary: a declaration is private to its domain unless marked `public`,
and a call into another domain names it (`inventory.reserve_stock(...)`).
Each file names where its other names come from on `use` lines. See
[Ownership and calls replace modules](#ownership-and-calls-replace-modules)
and [Imports](#imports).

## Naming
<!-- topic: naming -->

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
`constant`, `decline`, `else`, `emit`, `entity`, `fails`, `false`,
`fixture`, `for`, `function`, `if`, `in`, `library`, `match`, `mut`,
`none`, `not`, `or`, `package`, `propagate`, `provide`, `public`, `query`,
`reject`, `resource`, `return`, `routes`, `schedule`, `seed`,
`subscriber`, `test`, `true`, `type`, `use` and `var`. A setting the
language defines may still use one as its key
(`package: Stripe.v2024_06_20` in a connection, `test:` in configuration).

These words mean something only where the language expects them, and stay
usable as names everywhere else: `asc` and `desc` (a query's order), `end`
(templates), `matches` (the pattern operator), `input` and `value`, and the
words that open a declaration with no name, `acceptance`, `application`,
`authentication`, `config`, `flags`, `locales`, `migration`, `policy`,
`principal` and `suggestions` (area 14). `include`, `pick`, `omit` and `optional` build a type's fields,
so they name anything but a field (area 2), and the standard-library and
generated namespaces (`list`, `clock`, `config` ...) name no domain
or library.

## Comments
<!-- topic: comments -->

Line comments start with `//`. They carry no meaning and are kept for tooling.
`#` and `/* */` comments from other languages are `syntax.comment_style`,
whose edit writes them with `//` (or `///` for `/** */`).
A `///` comment attaches to the next declaration and documents it: editors,
generated documentation and the library catalogue show it.

## Every error has the same shape
<!-- topic: every-error-has-the-same-shape -->

Each compiler error gives the location, a reason, a suggested next step and who
should act on it, plus a stable rule ID with a help link. Tools read the same
fields as JSON with `jadpo check --json`. There is no warning severity:
every finding is an error, and stops `check` and the build until it is
answered.

## Declining a judgement call
<!-- topic: declining-a-judgement-call -->

Most rules report plain mistakes, which only a fix answers. A few are
judgement calls, where the code can be right as written: two helpers that
match today but differ on purpose, two constants that share a value by
coincidence. These rules are *declinable* (question 11): the author fixes the finding, or
declines it on the declaration itself with a reason, in a line directly
above the declaration or member, after its `///` comments. A decline is
the first child of the declaration or member below it: `///` comments,
then `decline` lines, then the declaration. The rule is written
`area.name` with no spaces, and either part may be a reserved word. A
decline with nothing under it is `syntax.expected_declaration`; one
inside a body is `syntax.expected_statement`.

```jadpo check project=e-declined
type Stock = Int {
    min: 0
}

library rounding {
    /// The amount, capped at a thousand.
    function capped(amount: Stock) -> Stock {
        if (amount > Stock(1000)) {
            return Stock(1000)
        }
        return amount
    }
}

/// The basket total, capped for the till.
decline library.similar_exists: "Caps the basket for the till, which changes on its own; rounding.capped caps one line"
function cap_total(total: Stock) -> Stock {
    if (total > Stock(1000)) {
        return Stock(1000)
    }
    return total
}
```

A decline covers its one rule on its one declaration: the innermost
declaration or member around the finding's location, so a decline on a
type covers a finding on the type, not one on its field. Its reason is
required: a missing or blank one is `decline.no_reason`, and the finding
stays declined meanwhile, so one error shows, not two. An agent or a
person may decline; every declinable error's `next:` ends with ``, or
write `decline <rule>: "<why>"` above it if this is deliberate``.
The declinable rules:

| Rule | Area | Reports |
|---|---|---|
| [`library.similar_exists`](#library-similar-exists) | 1 | A library function already does the same |
| [`constant.similar_exists`](03-logic.md#constant-similar-exists) | 3 | Another constant of the same type holds the same value |
| `translation.field_unused` | 10 | No wording in a language uses one of a message's fields |
| `package.standard_candidate` | 11 | A provider field looks like a standard type the package doesn't use |
| [`type.batch_order_updatable`](12-jobs-and-events.md#type-batch-order-updatable) | 12 | A batch walks a query ordered by a field an update can change |
| [`step.late_failure`](12-jobs-and-events.md#step-late-failure) | 12 | An action can fail for good after a changing call took effect |
| `email.field_unused` | 16 | An email's field is written by no template and no subject |
| `email.health_problem` | 16 | An email's template fails a check of the mail health score |
| `rate_limits.counted_in_memory` | 16 | A deployable built for Workers would count its rate limits in each instance's memory |
| `type.standard_exists` | 19 | A type has a standard type's shape under another name |
| `type.variant_retired` | 19 | Code writes a retired code |
| [`query.nearest_exact`](21-ai.md#query-nearest-exact) | 21 | Nearest-neighbour order on a vector too long for an index reads every record |

Each can be right as written: two helpers can differ on purpose, two
constants can share a value by coincidence (a VAT rate and a fee), a shape
can match a standard type by chance, and code that reads old records names
a withdrawn code on purpose. Plain mistakes are never declinable, including
area 11's integration retries that outlast a route's deadline, which can't
work as written. Naming any other rule, or one Jadpo doesn't have, is
[`decline.not_declinable`](#decline-not-declinable), which names the
nearest declinable rule.

A decline whose rule no longer fires there is
[`decline.unused`](#decline-unused), so declines don't outlive their
reason. It is judged only once every other finding is answered (no
syntax errors, and every other error is itself declinable or about a
decline), since another error can stop a rule from firing. A second
decline of the same rule on one declaration is unused.

A declined finding is still an error. In JSON a diagnostic keeps
`"severity": "error"` and gains `"declinable"`, `jadpo help --json`
gives each rule's `declinable`, and `jadpo check --json` and
`jadpo test --json` have no `warnings` counts.

`jadpo declines list` shows every decline with its rule, place and
reason, and `jadpo declines clear` removes them, so each finding comes
back to be fixed or declined again; both take `--rule <id>`, a path and
`--json` (area 14). `clear` is for after a new standard type lands, after a
compiler upgrade makes a rule smarter, and for a periodic sweep. It stops on syntax errors, as every command
that writes source does; `list` lists regardless. When a reason doesn't
fit beside its row, every reason goes on the lines under its row,
wrapped. In the editor, "Decline with a reason" on a declinable finding
writes the line with an empty reason for the author to fill in, and
"Remove decline" on a decline line removes it; both run the same code as
the commands. `jadpo approval` lists every decline a change adds or edits,
so declining by reflex shows in review (area 18); it is not built yet. A
rule triggered by time passing (area 16's expired flag, not built yet) is
judged at today's date by `check`, so CI fails a new change, and at the
commit's date by `jadpo build`, so rebuilding a merged revision for a
hotfix is never blocked by a date passing. It isn't declinable: extending
the date is the reviewed choice.

```text
$ jadpo declines list
Declines
  constant.similar_exists  domains/sales/libraries/fees.jadpo:4
    A courier's charge, not the fee
  library.similar_exists   domains/sales/libraries/rounding.jadpo:12
    Caps the basket for the till, which changes on its own

✓ 2 declines  2 rules · 2 files

$ jadpo declines clear --rule library.similar_exists
✓ Removed 1 decline from 1 file

→ jadpo check  shows each finding again, to fix or decline
```

## Decided layout
<!-- topic: decided-layout -->

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
jadpo.lock                 installed packages: version, source and digest
packages/                  installed packages, committed and read-only
api/
  main.jadpo_record        the public API record (area 7)
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
  identities.jadpo_record  stable identity of every renamed table, column and declaration
  migrations/
    20261012T0930_initial/
build/                     generated output, never committed
```

The same rules for a large application:

```text
app.jadpo  config.jadpo  authentication.jadpo  policy.jadpo  flags.jadpo  jadpo.lock
shared/
  types/          stock.jadpo  country_code.jadpo  address.jadpo
  libraries/      rounding.jadpo
  translations/   order_reference.jadpo  en_gb.jadpo  de_de.jadpo  sources.jadpo_record
  emails/         layout.html  layout.txt  button.jadpo  button.html  button.txt
  providers/      acme_couriers/v1/          the app's own integration, in the package format
packages/         stripe/v2024_06_20/  postcodes/v2/   installed: in jadpo.lock, read-only
api/              main.jadpo_record  admin.jadpo_record
acceptances/      20261020T1400_remove_todo_priority.jadpo
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
    translations/ order_shipped.jadpo  en_gb.jadpo  de_de.jadpo  sources.jadpo_record
    prompts/      suggest_gift_note.jadpo  suggest_gift_note.prompt
    documents/    invoice_document.jadpo  invoice_document.html
  inventory/
    actions/      reserve_stock.jadpo        public: called by checkout in one transaction
  payments/
    subscribers/  payment_succeeded.jadpo    on Stripe.events.PaymentIntentSucceeded
  notifications/
    subscribers/  send_order_confirmation.jadpo
  catalogue/  fulfilment/  identity/
database/
  identities.jadpo_record
  migrations/     20260901T1000_initial/  20261012T0930_add_order_channel/
  seeds/          local_demo.jadpo
translations/     fr_fr.jadpo        the app's wording for Jadpo's and packages' text
tests/            checkout_to_delivery.test.jadpo
build/
```

## The root
<!-- topic: the-root -->

- Only these entries: `app.jadpo`, `config.jadpo`, `authentication.jadpo`,
  `policy.jadpo`, `flags.jadpo`, `jadpo.lock`,
  `ignored_suggestions.jadpo_record`, `taught_suggestions.jadpo_record`,
  `domains/`, `shared/`, `packages/`, `database/`, `api/`,
  `acceptances/`, `translations/`, `tests/` and `build/`.
  `jadpo suggest ignore` writes the suggestions a team ignores, and
  `jadpo suggest found` the patterns it taught (area 14, "Suggestions").
  The hidden `.jadpo/`
  folder holds local, untracked state and is never read as source.
- `packages/` holds only installed packages, each in
  `packages/<name>/<version>/`, committed, read-only and checked against its
  digest in `jadpo.lock`; everything the app's authors wrote lives in
  `domains/` and `shared/`, whatever kind it is, so the app's own
  integrations are in `shared/providers/` (area 11). A package
  is laid out as `shared/` is, under the same rules.
- `app.jadpo` also records the project's Jadpo language version.
- The root `translations/` holds only the app's wording for text it did
  not write, one `<locale>.jadpo` file each, grouped by owner (`jadpo: { }`
  for Jadpo's own, a library package's name for its); an application's
  messages live in the domain that uses them, or in `shared/translations/`
  (area 10).
- A `.jadpo` file anywhere else is an error. Folders with no `.jadpo` files
  (`docs/`, `.github/`) are ignored.
- There is no `env.jadpo`. `config.jadpo` declares every setting; values come
  from the platform environment in production and from one git-ignored
  `.env.local` file locally, with secrets entered through `jadpo config set`.
- A single file handed to the compiler on its own (a test case, a snippet in
  these docs) is checked as one implicit domain with the layout rules off.

## Domains
<!-- topic: domains -->

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
  every call. Not built yet.
- A domain contains only these folders, each named after its declaration
  keyword, and only when they have files. No subfolders; revisit if any
  folder in a real application passes about 60 files.

| Folder | Holds | Notes |
|---|---|---|
| `entities/` | One entity per file: its fields, unique rules, lifecycle, policy and record caches; the generated operations (`Todo.get`, create, update, delete) belong to it | Queries, actions and failures each have their own folder (areas 4 and 5) |
| `types/` | Types used by more than one file in the domain | Single-use ones live beside their user |
| `failures/` | One failure per file, every failure the domain raises | Failures raised by two or more domains go in `shared/failures/` (area 4) |
| `actions/` | Every action, one per file | Called by name (`complete_todo(todo_id)`); called by triggers or, if `public`, by other domains |
| `queries/` | Every query, one per file, whatever it reads; a query's cache sits in its file | Area 5 |
| `libraries/` | Named groups of helper functions and constants, e.g. `library rounding` | Called like the standard library: `rounding.half_even(x)`, `pricing.vat_rate` (constants: area 3) |
| `routes/` | Named route groups with a prefix the compiler checks | HTTP trigger: the URL is written in the source |
| `schedules/` | Time triggers: when to run which action | |
| `events/` | Event definitions | |
| `subscribers/` | Event triggers, including long-running coordination | |
| (none) | Third-party APIs are provider packages: connections in `app.jadpo`; installed packages in root `packages/`, pinned in `jadpo.lock`; the app's own in `shared/providers/` | Area 11; there is no `integrations/` folder and no `integration` keyword |
| `emails/` | Email types and their templates, one email per name (`welcome_email.jadpo`, `welcome_email.html`, `welcome_email.txt`), and the domain's own email partials, laid out the same way | Area 16 |
| `translations/` | One `type X = Message` per file, one `translations { }` language file per locale named after it (`de_de.jadpo`), wording the domain's messages, failures, enums and email subjects, and the generated, committed `sources.jadpo_record` (area 18) | Area 10; a message a second domain uses goes in `shared/translations/`. A language file is named after its locale, not after its `translations { }` |
| `prompts/` | One `type X = Prompt` per file, each with its text in `<name>.prompt` beside it, two sections, `# Instructions` and `# Request` (area 21) | Area 21; a prompt a second domain uses goes in `shared/prompts/`, as a message does |
| `documents/` | Document types and their templates, one document per name (`invoice_document.jadpo`, `invoice_document.html`), rendered to HTML or PDF by the `documents` capability | Area 16. Not built yet: until the `documents` capability is, a file in `documents/` is `layout.unknown_location` |
| `fixtures/` | Test data shared by several test files in the domain | |

## Files and names
<!-- topic: files-and-names -->

- One primary declaration per file, named after it: `ServiceCredential` lives
  in `service_credential.jadpo`. Route groups and libraries are named
  declarations, so the same rule covers them. Supporting types, failures and
  helpers used only by that file live in it too. A declaration any other
  file uses (its own test file aside) has a file of its own, named after it,
  so every name a `use` line brings in is `<folder>/<name>.jadpo`
  (see [Imports](#imports)).
- Everything in a file besides its primary declaration is used by the
  primary declaration, directly or through one another. One that only the
  file's tests use, or nothing uses, is dead code, reported as
  `layout.unused_declaration` like an unused shared declaration, so a file
  never collects helpers for something else. The primary declaration is
  the one of the folder's kind named after the file (the first of that kind
  when the file is misnamed), or the seed in `database/seeds/`. One another
  file uses is moved by `layout.promotion_required`, not reported unused.
  While the primary, or a declaration it uses, writes a name that names
  nothing, perhaps a misspelling of the one it meant, nothing beside it is
  called unused, as with imports. Test files, root files and loose files
  have no primary declaration, and a library's functions keep their own
  rule: one nothing calls is unused.
- Definitions come before their use: after any opening comment and the
  `use` lines, every declaration in a file comes before the first one that
  uses it, so the primary declaration comes last, read with the context of
  the constants and types it needs. Ties are broken by
  kind (constants, then types, then failures, then helpers) and within a
  kind by the order the primary declaration first reaches them;
  declarations that refer to each other keep their written order among
  themselves, and constants written together stay together. `jadpo fmt`
  writes this order and `jadpo fmt --check` fails on a file out of it. A
  file with no primary
  declaration (a test file, a root file, a loose file) keeps its written
  order. Reaching follows the primary's names in the order it first writes
  them, into each declaration before the next. A
  declaration nothing reaches (reported as `layout.unused_declaration`)
  goes after the reached ones of its kind, in written order, and a
  declaration still waits for what it uses whatever its kind, so a constant
  of type `Stock` comes after `Stock`.
- A name must convert to its file name and back unchanged, so consecutive
  capitals are not allowed: `VatRate`, not `VATRate` (Rust's guideline too).
- Names are unique across the whole project and across kinds, with one
  exception: a private `function` or `type` (other than a failure or an
  email, `type X = Failure` or `EmailMessage`) needs a name unique only
  within its domain, whether it has a file of its own or sits beside the
  declaration that uses it. A private `constant` follows the same rule
  (area 3). Everything else stays unique across the project:
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
- A domain is not a namespace. Tables, error codes, OpenAPI schema names,
  event names and identity keys all need one unique name anyway, so a
  namespace per domain would only move the prefix into generated names, and
  `Customer` would mean different things by folder in a diff.
- A clash is `semantic.duplicate_declaration`, which says whether the two
  are a copy (use the existing one, or move it to `shared/`) or two things
  with one name (rename one), never invents a name, and lists the names in
  use containing the same word. `jadpo rename`, not built yet, refuses a
  new name already in use before editing anything (area 14). An agent
  working in parallel merges `main` before pushing, runs `jadpo check`
  (it builds nothing, so it is quick), fixes any
  clash, then pushes; generated CI runs `jadpo check` first so a clash fails
  in seconds (area 14, [Tools for agents](14-tooling.md#tools-for-agents)).
- One exception to file-name uniqueness: a type declared in the same file
  as an action, query or function may fold to that callable's name, as an
  input does (`type CreateTodo = Input` beside `action create_todo` in
  `actions/create_todo.jadpo`), because only one file carries the name. If
  the type is promoted to its own file, the rule applies again.
- Tests sit beside their subject as `<name>.test.jadpo`. Scenarios that span
  several domains go in the root `tests/`. The kind of test (pure,
  action, route, schedule) is known from what it calls, so it is not a
  folder; `jadpo test --kind route` runs one kind.
- Non-Jadpo files (provider contracts, email templates, search mappings) sit
  beside the file that uses them and must be referenced from it.

## Sharing and promotion
<!-- topic: sharing-and-promotion -->

- A type, failure or helper used by one file lives in that file. When a
  second file in the domain uses it, it must move to a file of its own in the
  domain's `types/`, `failures/` or `libraries/`; when a second domain does,
  to `shared/` (pure types, failures, libraries and messages only; a
  message moves to `shared/translations/`). A constant (area 3) is placed
  as a helper function is: used by one file it sits at the top of that
  file, never as a primary declaration, and used by a second it moves into
  a library and is read through it, `pricing.vat_rate`, so a library holds
  functions and constants. `shared/emails/`
  holds only the app's optional email layout, `layout.html` and
  `layout.txt`, and the email partials every domain's emails may include,
  each a `type X = EmailPartial` with its templates beside it (area 16),
  and `shared/documents/` only its
  optional document layout, `layout.html`. `shared/providers/` holds the
  app's own integrations, each in `shared/providers/<name>/<version>/` in
  exactly the package format (area 11); they are not promoted
  there, but written there from the start. Moving to the
  right folder is not enough: `DiscountCode` in `types/basket.jadpo` used by
  another file must move to `types/discount_code.jadpo`. Tests never count as users, a route only counts when
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
  then clashes, as `semantic.duplicate_declaration`. The identity key of a
  function or type declared in a domain carries the domain, so the move
  writes one line in the identity registry, and it reads as a move and not
  as a removal and an addition (area 18, "Stable identity").
  `layout.promotion_required`'s edits for a type moving to `shared/types/`
  add the line to `database/identities.jadpo_record`, so
  `jadpo fmt --layout` writes it. A failure raised by a second domain gets
  no edit, so moving it by hand writes no line, and a library keeps its
  name in `shared/`, so its functions' keys need none.
- A helper that depends on one domain's types (`order_math.total(order)`)
  stays in that domain and becomes public when another domain calls it.
  Depending on two domains' types is an error that needs a design decision.
  Making a helper generic, for example extracting `pricing.sum(amounts)` for
  `ordering` and `payments` to share, is a deliberate refactor.
- Two constants of the same type holding the same value are the
  declinable `constant.similar_exists`, so parallel agents each inventing
  `vat_rate` find each other's (area 3).
- Duplicates are caught at the point of writing: when a new helper looks like
  an existing one, `jadpo check` says so and names it ("a similar library
  exists: `rounding.half_even` in `shared/libraries/rounding.jadpo`"). The
  compiler also generates a catalogue of every library function with its
  signature, documentation comment and callers, and the Jadpo agent skill
  tells agents to consult it before writing a helper. Agents look things
  up in the compiler's project map (the language server) rather than
  browsing folders. Placement alone can't prevent two copies: two domains
  may each hold a private helper with the same name.
- A function's description is written once, as a `///` comment above it.
  The catalogue, the caller list and the domains that use it are computed
  and shown in the editor, never written into source files.
- Before shared or public code changes, its reach is visible in three ways:
  find-references in the editor, a `jadpo impact <name>`
  command (not built yet) listing calling domains, entry points and
  covering tests, and
  `jadpo check` itself reporting which domains a change affects and running
  their tests.

## Ownership and calls replace modules
<!-- topic: ownership-and-calls-replace-modules -->

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
- **Everything reachable from a public declaration is public** (area 11).
  A type in a public action's,
  query's, library's or event's signature, and a failure it can raise,
  must itself be public (or shared), checked on every compile; an internal
  failure is mapped or handled before it leaves. A package's public surface
  follows the same rule.
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
  nothing is always reported, never left to fail when it runs.
- There are no `module` headers; each file names where its names come
  from on `use` lines: see [Imports](#imports).

## Imports
<!-- topic: imports -->

Imports make the place every name comes from readable from the file
itself, without searching. Without them, a name read in one file could be
declared in that file, in its domain's `types/`, `failures/` or
`entities/`, in `shared/`, or in another domain, and an agent would have to
search for it.

Each `use` line names a folder, then the names it brings in, written exactly
as the code uses them, so finding a name in the page finds its import:

```jadpo fragment
// domains/sales/actions/price_basket.jadpo
use shared/types { Percent, Sterling }
use shared/libraries { pricing }
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
  `List<T>`) and standard-library namespaces (`list.count(...)`) are
  fixed per language version and can't be declared by a project. An
  entity's generated operations (`Todo.get(id)`) come with the entity.

  The standard library's namespaces are `list`, `set`, `map`, `range`,
  `integer`, `decimal`, `money`, `units`, `text`, `markup`, `hash`,
  `encoding`, `crypto`, `random`, `json`, `csv`, `tsv`, `json_lines`,
  `temporal`, `locale`, `colour`, `media`, `web`, `semver`, `geo` and `ai`
  (area 19, "One namespace per type" and "Reserved namespaces"). Each is reserved by
  `semantic.standard_namespace_reserved`; fields and named arguments keep
  every word (`note.text`, `set:`).
- **Calls into another domain keep the domain's name at the call.** `use menu`
  brings in the domain, and its public actions, queries and fixtures are
  called as `menu.stock_item(...)`, so a call that crosses domains is
  visible on its own line. Another domain's public types and failures come
  in by folder like any other name: `use menu/types { MenuItem }`.
- **A test file sees everything its subject file declares** without
  importing it, so `stock_item.test.jadpo` calls the `loaf_stock` helper
  in `stock_item.jadpo` directly. Anything else it uses is imported.
- **Reading a name therefore has three answers:** it is on a `use` line, so
  it is in `<folder>/<name>.jadpo`; it is a built-in; or it is declared in
  this file.
- **The formatter keeps imports.** One line per folder, sorted, and
  `jadpo fmt` or the editor adds a missing import and removes an unused one.
  Every name a file can import is unique where it can import it (a private
  function or type within its own domain, anything importable across
  domains across the project), so the import for a name is
  never in doubt and a forgotten one is fixed, not a compile failure for an
  agent.
  `jadpo check` still reports an import that names nothing, and one the
  file doesn't use.
- **Every name clicks through in the editor.** Go to definition from any
  use of a name, in a body, a signature or a `use` line, opens its
  declaration in its `.jadpo` file.

This is the one comma-separated `{ }` list in the language, where every
other block holds one `name: value` item per line, accepted because the
names have to appear exactly as the code writes them.

How the lines are written and checked:

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

## Database
<!-- topic: database -->

- `database/identities.jadpo_record` is the registry that gives every table, column
  and index a stable identity, so renaming a field renames its column instead
  of dropping it (area 18 narrows it to renames, data and code alike).
  Compiler-maintained, committed and reviewed. One sorted
  entry per line, so two branches that change the schema produce a readable
  merge.
- **Committed generated files share one extension, `.jadpo_record`**, and
  stay next to what they describe: `api/<api>.jadpo_record`,
  `database/identities.jadpo_record`, each `translations/sources.jadpo_record`
  and each installed package's `surface.jadpo_record`. Each is in Jadpo's readable format and
  starts with a line saying what wrote it and which command updates it;
  `jadpo check` regenerates it and a hand edit is an error naming that
  command. `jadpo.lock` keeps its name. `jadpo records` lists them all and
  flags any out of date (area 18). People-written files (acceptances) are
  not records.
- Each schema change is a folder under `database/migrations/`, named by date,
  time and purpose, holding your decisions on destructive changes, the
  generated SQL for each database and any data backfill written in Jadpo. The
  compiler keeps a record of the latest migration so history stays linear.
- `database/seeds/` holds local demonstration and development data.

## What it looks like in code
<!-- topic: what-it-looks-like-in-code -->

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
connections: {
    stripe: {
        package: Stripe.v2024_06_20
        credential: config.stripe_secret_key
        domains: [payments]
    }
}
```

## Decided in later areas
<!-- topic: decided-in-later-areas -->

Route group syntax (area 7), the `schedule` keyword (area 12), integrations
and provider API versions (area 11), translation file format (area 10) and the
migration workflow (area 5) are settled in their areas; the layout only
reserves their folders.

## What is built
<!-- topic: project-layout-and-modules-built -->

The compiler builds the decided layout, with each rule's real output under
[Errors](#errors).

| Feature | Status |
|---|---|
| Root, domains and their folders | Built: a file where the layout has no place for it is `layout.unknown_location`, and a declaration in the wrong folder `layout.wrong_folder`; a folder or file with no `app.jadpo` above it is checked as loose files. |
| Files and names | Built: one primary declaration per file, named after it; names unique project-wide; the round-trip and folded-collision rules, with the exception for a type folding to its callable's name. |
| Names per domain | Partly built: private functions and types unique only within their domain, each domain's code reading its own; their identity keys carrying the domain, with promotion to `shared/` writing a registry line; `semantic.duplicate_declaration` saying copy or clash and listing the names in use with the same words. `jadpo rename` is not built yet (area 14). |
| Order independence | Built: declaration and file order never matter. |
| Domains, `public` and calls | Built: owner-qualified calls and per-declaration `public`, with `visibility.private_cross_domain`, `visibility.public_not_allowed` and `visibility.public_unused`. |
| Reserved words | Built: a reserved word can't name anything. |
| Comments | Built: `//` and `///` documentation comments. |
| Similar helpers | Built: `library.similar_exists`, a declinable error. |
| Declinable errors | Partly built: no warning severity; the `decline <rule>: "<reason>"` line with `decline.unused`, `decline.not_declinable` and `decline.no_reason`; `jadpo declines list` and `clear`; the editor's "Decline with a reason" and "Remove decline". `jadpo approval` (area 18) and the date rule for expired flags (area 16) are not built yet. |
| Constants | Built: `constant` as a reserved word and a declaration placed and shared as helper functions are, a private one unique within its domain, `constant.similar_exists`, and a top-level `var` pointed at `constant` (area 3). |
| Imports | Built: `use <folder> { Names }` and `use <domain>`, with `import.missing`, `import.unknown`, `import.unused`, `import.unneeded` and `import.duplicate`; `jadpo fmt` adds, removes and sorts imports, `jadpo fmt --layout` makes promotion moves, and go to definition works from every use of a name, a `use` line's included. |
| A file holds only what its primary declaration uses | Built: `layout.unused_declaration` for a supporting declaration the primary declaration doesn't use, directly or through one another. |
| Definition before use | Built: `jadpo fmt` writes each file's declarations before the first one that uses them, and `jadpo fmt --check` fails on a file out of that order. |
| Referenced assets | Not built yet: `layout.asset_unreferenced` waits for the reference forms of areas 11 and 16. |
| Packages | Built: the root `packages/` and `jadpo.lock`, `shared/providers/`, packages checked as `shared/` is, `use packages/<name>`, and the reachable-from-public rule (`visibility.public_reaches_private`) in domains and packages; area 11 records the choices. |

## Errors

Every error this area can produce, with an example that triggers it and the
compiler's real output from `jadpo check`. An example without `app.jadpo`
is checked as loose files, one implicit domain with the layout rules off; the
layout examples are small projects. `layout.asset_unreferenced` is not
built yet: it waits for the reference forms of areas 11 and 16.

- [`toolchain.empty_project`](../errors/toolchain.empty_project.md)
- [`semantic.duplicate_declaration`](../errors/semantic.duplicate_declaration.md)
- [`semantic.name_case`](../errors/semantic.name_case.md)
- [`naming.round_trip`](../errors/naming.round_trip.md)
- [`naming.folded_collision`](../errors/naming.folded_collision.md)
- [`naming.generic_name`](../errors/naming.generic_name.md)
- [`semantic.standard_namespace_reserved`](../errors/semantic.standard_namespace_reserved.md)
- [`semantic.unknown_name`](../errors/semantic.unknown_name.md)
- [`semantic.unknown_callee`](../errors/semantic.unknown_callee.md)
- [`semantic.standard_operation_qualification`](../errors/semantic.standard_operation_qualification.md)
- [`semantic.not_callable`](../errors/semantic.not_callable.md)
- [`semantic.wrong_name_kind`](../errors/semantic.wrong_name_kind.md)
- [`visibility.private_cross_domain`](../errors/visibility.private_cross_domain.md)
- [`visibility.public_not_allowed`](../errors/visibility.public_not_allowed.md)
- [`visibility.public_unused`](../errors/visibility.public_unused.md)
- [`visibility.public_reaches_private`](../errors/visibility.public_reaches_private.md)
- [`semantic.own_domain_qualification`](../errors/semantic.own_domain_qualification.md)
- [`import.missing`](../errors/import.missing.md)
- [`import.unknown`](../errors/import.unknown.md)
- [`import.unused`](../errors/import.unused.md)
- [`import.unneeded`](../errors/import.unneeded.md)
- [`import.duplicate`](../errors/import.duplicate.md)
- [`layout.unknown_location`](../errors/layout.unknown_location.md)
- [`layout.wrong_folder`](../errors/layout.wrong_folder.md)
- [`layout.file_name_mismatch`](../errors/layout.file_name_mismatch.md)
- [`layout.promotion_required`](../errors/layout.promotion_required.md)
- [`layout.unused_declaration`](../errors/layout.unused_declaration.md)
- [`layout.test_without_subject`](../errors/layout.test_without_subject.md)
- [`layout.route_prefix_mismatch`](../errors/layout.route_prefix_mismatch.md)
- [`layout.call_direction`](../errors/layout.call_direction.md)
- [`library.similar_exists`](../errors/library.similar_exists.md)
- [`decline.unused`](../errors/decline.unused.md)
- [`decline.not_declinable`](../errors/decline.not_declinable.md)
- [`decline.no_reason`](../errors/decline.no_reason.md)
- [`syntax.expected_declaration`](../errors/syntax.expected_declaration.md)
- [`syntax.expected_name`](../errors/syntax.expected_name.md)
- [`syntax.unexpected_token`](../errors/syntax.unexpected_token.md)
- [`syntax.unexpected_character`](../errors/syntax.unexpected_character.md)
- [`syntax.unterminated_string`](../errors/syntax.unterminated_string.md)
- [`syntax.invalid_escape`](../errors/syntax.invalid_escape.md)
### Habits from other languages

People and agents write constructs from other languages out of habit. Each has an error of its own that names
the habit and the Jadpo form, with an edit, and parsing carries on as if the
Jadpo form had been written, so a line gives one error. When the Jadpo
form only renames or drops words, the rest of the project is checked too.
The catalogue is `jadpo_syntax::habits`; [area 3](03-logic.md#habits-from-other-languages)
explains how a line shows one error at a time.

- [`syntax.comment_style`](../errors/syntax.comment_style.md)
- [`syntax.import_statement`](../errors/syntax.import_statement.md)
- [`syntax.export_keyword`](../errors/syntax.export_keyword.md)
