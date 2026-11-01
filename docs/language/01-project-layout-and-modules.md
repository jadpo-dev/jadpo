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

A project is the folder that holds `app.jadpo`. Its layout is fixed, so you
never decide where a file goes: its path follows from two facts, the
business area it belongs to and what it declares.

The project's root holds a fixed set of files and folders. All application
code lives in business areas, called domains, in folders under `domains/`,
one level deep. Every domain holds the same fixed folders, each named after
the kind of declaration it holds: `entities/`, `actions/`, `queries/` and so
on.

Each file holds one primary declaration, named after the file and written
last in it, below everything it needs. Nothing else goes in the file unless
the primary declaration uses it. [Decided layout](#decided-layout) gives
the whole layout, and [Files and names](#files-and-names) the rules for
each file.

A single file, or a folder with no `app.jadpo` above it, is checked as
loose files: one implicit domain, with the layout rules off.

## Source files
<!-- topic: source-files -->

File order and declaration order never matter. A declaration can refer to
one defined later in the file, or in another file.

Names are unique across the whole project, so a second `Count` in another
file is the error
[`semantic.duplicate_declaration`](../errors/semantic.duplicate_declaration.md).
The one exception is a private function, type or constant: its name needs
to be unique only within its domain (see [Files and names](#files-and-names)).

## Domains instead of modules
<!-- topic: domains-instead-of-modules -->

Jadpo has no modules. A domain owns everything in it, and it is the
boundary of what other code can see: a declaration is private to its
domain unless it is marked `public`.

A call into another domain names that domain, as in
`inventory.reserve_stock(...)`. Each file says where its other names come
from on `use` lines at its top. See
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

Identifiers are ASCII letters, digits and underscores, and can't start with
a digit. The compiler checks the case and suggests the right spelling.

A name must convert to its file name and back unchanged, so a name can't
have two capitals in a row: write `VatRate`, not `VATRate`.

### Reserved words

The words that control flow or open a declaration are reserved. None of
them can name a declaration, field, parameter, variable or variant, and
using one as a name is `syntax.expected_name`. They are `action`, `and`,
`assert`, `attempt`, `constant`, `decline`, `else`, `emit`, `entity`,
`fails`, `false`, `fixture`, `for`, `function`, `if`, `in`, `library`,
`match`, `mut`, `none`, `not`, `or`, `package`, `propagate`, `provide`,
`public`, `query`, `reject`, `resource`, `return`, `routes`, `schedule`,
`seed`, `subscriber`, `test`, `true`, `type`, `use` and `var`.

A domain is named by its folder, so a folder named with a reserved word,
`domains/library/`, is `syntax.expected_name` once, at the folder. Where
the domain is used, `library.borrow(...)` or `library.borrow:` in root
`policy.jadpo`, the word is read as the domain's name and nothing more is
reported. A block whose names are wrong reports its first
`syntax.expected_name`: a later line in it is most often the same mistake,
reported once the first is fixed.

A setting the language defines may still use a reserved word as its key,
such as `package: Stripe.v2024_06_20` in a connection or `test:` in
configuration.

### Words that are names everywhere else

Some words mean something only where the language expects them, and can be
used as names anywhere else:

- `asc` and `desc`, a query's order;
- `end`, in templates;
- `matches`, the pattern operator;
- `input` and `value`;
- the words that open a declaration with no name: `acceptance`,
  `application`, `authentication`, `config`, `flags`, `locales`,
  `migration`, `policy`, `principal` and `suggestions`
  ([Suggestions](14-tooling.md#suggestions)).

`include`, `pick`, `omit` and `optional` build a type's fields
([Values and types](02-values-and-types.md)), so they can name anything but
a field. The standard-library and generated namespaces (`list`, `clock`,
`config` and the rest) can't name a domain or a library.

## Comments
<!-- topic: comments -->

A line comment starts with `//`. It carries no meaning, and tools keep it.

A `///` comment documents the declaration that follows it. Editors, the
generated documentation and the library catalogue show it.

Comments in other languages' styles, `#` and `/* */`, are
`syntax.comment_style`. Its edit rewrites them with `//`, or with `///` for
a `/** */` comment.

## Every error has the same shape
<!-- topic: every-error-has-the-same-shape -->

Each compiler error gives where the problem is, a reason, a suggested next
step and who should act on it, and a stable rule ID with a link to its
page. Tools read the same fields as JSON with `jadpo check --json`.

`(human)` at the end of an error's `next:` line tells your coding agent to
stop and ask you which fix you want, because the choice is yours to make
(who can see data, what a setting holds). `(agent)` means your agent can
make the fix itself.

There is no warning severity. Every finding is an error, and it stops
`check` and the build until it is answered: fixed or, for the few rules
that allow it, declined.

## Declining a judgement call
<!-- topic: declining-a-judgement-call -->

Most rules report plain mistakes, and only a fix answers them. A few rules
are judgement calls, where the code can be right as written: two helpers
that match today but differ on purpose, or two constants that share a value
by coincidence. These rules are *declinable*. You either fix the finding,
or decline it on the declaration itself, with a reason.

Here a library function already caps an amount, and `cap_total` does the
same. The `decline` line says why the second one is deliberate, so
`library.similar_exists` doesn't report it:

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

### Writing a decline

A decline is a line of the form `decline <rule>: "<reason>"`, directly
above the declaration or member it covers, after its `///` comments. So
the order is `///` comments, then `decline` lines, then the declaration.

The rule is written `area.name` with no spaces, and either part may be a
reserved word. A decline with no declaration under it is
`syntax.expected_declaration`, and one inside a body is
`syntax.expected_statement`.

A decline covers its one rule on its one declaration: the innermost
declaration or member around the finding's location. So a decline on a
type covers a finding on the type, not one on its field.

The reason is required. A missing or blank one is `decline.no_reason`, and
the finding stays declined meanwhile, so you see one error, not two.

An agent or a person may decline. A rule the compiler's catalogue marks
as a security rule, such as `policy.call_ungranted`, also needs an
[acceptance](18-records-and-manifest.md#acceptances) to decline, since
an agent can write a reason itself: a new decline of one is
`PolicyChange.<rule name>_declined` on the declaration it sits above
(`lending.borrow: PolicyChange.call_ungranted_declined`), and
`access.jadpo_record` keeps every such decline. `jadpo help --json` gives
each rule's `security`. Every declinable error's `next:` line ends with
", or write `decline <rule>: "<why>"` above it if this is deliberate",
and a security rule's with ", which a person then accepts".

### Which rules can be declined

| Rule | Page | Reports |
|---|---|---|
| [`library.similar_exists`](../errors/library.similar_exists.md) | Project layout and modules | A library function already does the same |
| [`constant.similar_exists`](../errors/constant.similar_exists.md) | [Logic](03-logic.md) | Another constant of the same type holds the same value |
| [`policy.call_ungranted`](../errors/policy.call_ungranted.md) | [Policy](09-policy.md) | A listed action calls one that refuses some of the people its line lets in |
| `translation.field_unused` | [Configuration and time](10-configuration-and-time.md) | No wording in a language uses one of a message's fields |
| `package.standard_candidate` | [Integrations](11-integrations.md) | A provider field looks like a standard type the package doesn't use |
| [`type.batch_order_updatable`](../errors/type.batch_order_updatable.md) | [Jobs and events](12-jobs-and-events.md) | A batch walks a query ordered by a field an update can change |
| [`step.late_failure`](../errors/step.late_failure.md) | [Jobs and events](12-jobs-and-events.md) | An action can fail for good after a changing call took effect |
| `email.field_unused` | [Built-in services](16-built-in-services.md) | An email's field is written by no template and no subject |
| `email.health_problem` | [Built-in services](16-built-in-services.md) | An email's template fails a check of the mail health score |
| `rate_limits.counted_in_memory` | [Built-in services](16-built-in-services.md) | A deployable built for Workers would count its rate limits in each instance's memory |
| [`type.unit_as_number`](../errors/type.unit_as_number.md) | [Values and types](02-values-and-types.md) | A type named for a unit, a currency's minor unit or a percentage is built on a plain number |
| `type.standard_exists` | [Standard library](19-standard-library.md) | A type has a standard type's shape under another name |
| `type.variant_retired` | [Standard library](19-standard-library.md) | Code writes a retired code |
| [`query.nearest_exact`](../errors/query.nearest_exact.md) | [AI](21-ai.md) | Nearest-neighbour order on a vector too long for an index reads every record |

Each of these can be right as written. Two helpers can differ on purpose,
two constants can share a value by coincidence (a VAT rate and a fee), a
listed action can call a helper only some of its callers are meant to
pass, a shape can match a standard type by chance, a name can be a unit
by chance, and code that reads old records names a withdrawn code on
purpose.

Plain mistakes are never declinable. That includes retries of a call to an
outside service that outlast a route's deadline
([Integrations](11-integrations.md)), which can't work as written. Naming
any other rule in a decline, or a rule Jadpo doesn't have, is
[`decline.not_declinable`](../errors/decline.not_declinable.md), which names
the nearest declinable rule.

### Declines that no longer apply

A decline whose rule no longer fires on its declaration is
[`decline.unused`](../errors/decline.unused.md), so declines don't outlive
their reason. A second decline of the same rule on one declaration is
unused too.

`decline.unused` is judged only once every other finding is answered: no
syntax errors, and every other error is itself declinable or about a
decline. That's because another error can stop a rule from firing.

### A declined finding is still an error

In JSON, a declined diagnostic keeps `"severity": "error"` and gains
`"declinable"`. `jadpo help --json` gives each rule's `declinable`, and
`jadpo check --json` and `jadpo test --json` have no `warnings` counts.

### Listing and clearing declines

`jadpo declines list` shows every decline with its rule, place and reason.
`jadpo declines clear` removes them, so each finding comes back to be fixed
or declined again. Both take `--rule <id>`, a path and `--json`
([Commands for records, findings and review](14-tooling.md#commands-for-records-findings-and-review)).

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

When a reason doesn't fit beside its row, `list` puts every reason on the
lines under its row, wrapped.

Use `clear` after a new standard type lands, after a compiler upgrade makes
a rule smarter, and for a periodic sweep. It stops on syntax errors, as
every command that writes source does; `list` lists whatever the state of
the code.

In the editor, "Decline with a reason" on a declinable finding writes the
line with an empty reason for you to fill in, and "Remove decline" on a
decline line removes it. Both run the same code as the commands.

`jadpo approval` lists every decline a change adds or edits, so declining
by reflex shows in review ([Records and manifest](18-records-and-manifest.md)).

### Rules triggered by time passing

A rule triggered by time passing, such as a
[feature flag](16-built-in-services.md#feature-flags) whose date has
expired, is judged at today's date by `check`, so CI fails a new change. It
is judged at the commit's date by `jadpo build`, so rebuilding a merged
revision for a hotfix is never blocked by a date passing. It isn't
declinable: extending the date is the reviewed choice. When
`SOURCE_DATE_EPOCH` is set, as for a reproducible build, both judge at
its date; a build outside git, or before the first commit, judges at
today.

Not built yet: `jadpo approval`.

## Decided layout
<!-- topic: decided-layout -->

Every project has the same shape at every size. The root holds a fixed set
of files and folders. All application code lives in domains under
`domains/`, one level deep, and every domain contains the same fixed
folders, with no subfolders. A file's path follows from two facts: its
domain and what kind of declaration it is.

A small app:

```text
app.jadpo                  application, principal, health, readiness, language version
config.jadpo               configuration: names, types, bindings, defaults
authentication.jadpo       authentication methods and their failures
policy.jadpo               application-wide policy, such as tenancy
jadpo.lock                 installed packages: version, source and digest
packages/                  installed packages, committed and read-only
api/
  main.jadpo_record        the public API record
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

The same rules for a large application, with most of the folders a project
can have:

```text
app.jadpo  config.jadpo  authentication.jadpo  policy.jadpo  flags.jadpo  jadpo.lock
access.jadpo_record  network.jadpo_record   who can do what, and the outside hosts
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

- The root holds only these entries: `app.jadpo`, `config.jadpo`,
  `authentication.jadpo`, `policy.jadpo`, `flags.jadpo`, `jadpo.lock`,
  `access.jadpo_record`, `network.jadpo_record`,
  `ignored_suggestions.jadpo_record`, `taught_suggestions.jadpo_record`,
  `domains/`, `shared/`, `packages/`, `database/`, `api/`,
  `acceptances/`, `translations/`, `tests/` and `build/`, beside the
  files git and its host read, such as `CODEOWNERS`.
- `jadpo suggest ignore` writes the suggestions a team ignores, and
  `jadpo suggest found` the patterns it taught
  ([Suggestions](14-tooling.md#suggestions)).
- The hidden `.jadpo/` folder holds local, untracked state and is never
  read as source.
- `packages/` holds only installed packages, each in
  `packages/<name>/<version>/`. They are committed, read-only, and checked
  against their digest in `jadpo.lock`. Everything the app's authors wrote
  lives in `domains/` and `shared/`, whatever kind it is, so the app's own
  integrations are in `shared/providers/`
  ([Where packages live](11-integrations.md#where-packages-live)). A package
  is laid out as `shared/` is, under the same rules.
- `app.jadpo` names the Jadpo release line the project is written for, as
  `jadpo: "X.Y"` in `application { }`. It is the major and minor numbers as
  text, the same form a package's index uses for its own
  ([The package format](11-integrations.md#the-package-format)). It is a
  line, not a minimum: while Jadpo is at 0.x, a minor release may break
  apps, so the line is what must match, and a release that only fixes
  something stays on its line. The installed `jadpo` runs the newest
  release of that line it keeps or can fetch, and `jadpo.lock` may pin the
  exact release for reproducible builds
  ([Which jadpo runs](14-tooling.md#which-jadpo-runs)). A `jadpo` that
  can't hand the project to that release refuses it with an error alone,
  naming the release to install: `toolchain.version_newer` when it is on
  an earlier line, rather than misread what a later version added, and
  `toolchain.version_older` when it is on a later line, rather than compile
  the project silently. Anything but a major and minor number is
  `toolchain.version_invalid`. If the line is left out, nothing is checked.
  `jadpo new` writes the line of the `jadpo` that ran it
  ([The `jadpo` command](14-tooling.md#the-jadpo-command)).
- The root `translations/` holds only the app's wording for text it didn't
  write, one `<locale>.jadpo` file each, grouped by owner: `jadpo: { }` for
  Jadpo's own text, a library package's name for its text. An
  application's own messages live in the domain that uses them, or in
  `shared/translations/`
  ([Translations as messages](10-configuration-and-time.md#translations-as-messages)).
- A `.jadpo` file anywhere else is an error. Folders with no `.jadpo` files
  (`docs/`, `.github/`) are ignored.
- There is no `env.jadpo`. `config.jadpo` declares every setting. Values
  come from the platform's environment in production and from one
  git-ignored `.env.local` file locally, with secrets entered through
  `jadpo config set`.
- A single file handed to the compiler on its own, such as a test case or
  a snippet in these docs, is checked as one implicit domain with the
  layout rules off.

## What `application { }` holds
<!-- topic: application-sections -->

`application { }` in `app.jadpo` holds the settings for the whole app,
each in a section of its own. These are all the sections it takes, and
the page that decides each:

| Section | What it sets | Decided in |
|---|---|---|
| `jadpo:` | The Jadpo release line the project is written for | [The root](#the-root) |
| `display_name:` | The app's name, as people see it | [The application and revocation](08-identity.md#the-application-and-revocation) |
| `revocation_delay:` | How long a disabled caller or a revoked session may keep working | [The application and revocation](08-identity.md#the-application-and-revocation) |
| `connections:` | Every database, driver and package the app connects to | [Connecting in `app.jadpo`](11-integrations.md#connecting-in-appjadpo) |
| `deployables:` | What is built to run, and where | [Declared deployables](14-tooling.md#declared-deployables) |
| `apis:` | The app's APIs and their versions | [APIs and versions](07-http.md#apis-and-versions) |
| `client_address:` | Where a request's client address is read | [Client addresses](10-configuration-and-time.md#client-addresses) |
| `translations:` | Whether the root translations need a review | [Review](10-configuration-and-time.md#review) |
| `rate_limits:` | Where rate limits are counted | [Rate limits](16-built-in-services.md#rate-limits) |
| `email:` | The connection every email goes through, and its sender | [Email](16-built-in-services.md#email) |
| `events:` | The store events are kept in | [Where the event is kept](12-jobs-and-events.md#where-the-event-is-kept) |
| `schedules:` | The store schedules' state is kept in | [Schedules](12-jobs-and-events.md#schedules) |
| `steps:` | How an action's steps are kept and sealed | [Several changing calls in one action](12-jobs-and-events.md#several-changing-calls-in-one-action) |
| `receiving:` | The queues read into the app's events | [Events from outside](12-jobs-and-events.md#events-from-outside) |
| `ai:` | The model connection, its models and its limits | [The `ai` capability](21-ai.md#the-ai-capability) |

Seven more are decided and not built yet: `bot_check:`
([Bot protection](16-built-in-services.md#bot-protection)), `documents:`
([Documents](16-built-in-services.md#documents)), `grants:`
([OAuth grants](11-integrations.md#acting-for-each-user-oauth-grants)),
`privacy:` ([Data requests](15-personal-data.md#data-requests)),
`publishing:` ([Where events travel](12-jobs-and-events.md#where-events-travel)),
`scanning:` ([Image variants and scanning](16-built-in-services.md#image-variants-and-scanning))
and `telemetry:` ([Telemetry](16-built-in-services.md#telemetry)). Each is
`toolchain.not_built_yet` where it is written, since accepting it would
read as a protection or a service the app doesn't have.

Any other section is `semantic.application_section_unknown`, so a
misspelt setting meant to protect the app is never dropped without a word.
When it is a near miss of a section, such as `revocation_dela:`, the
error's edit respells it.

## Domains
<!-- topic: domains -->

Every project has at least one domain, and all code outside the root files
lives in one. The folder's name is the domain's name. Nothing in the code
declares it, so the two can't disagree. Domains are one level deep and
never nest.

Name a domain after a business area (`ordering`, `planning`), not a plural
entity (`orders`, `todos`). Domain names appear in calls, so they must not
read like variables. Generic names (`misc`, `common`, `utils`, `helpers`,
`core`) are refused for domains and libraries alike.

Split a domain only along a seam the compiler shows has no references
across it ("these 12 entities never reference these 9"). A large domain
whose parts depend on each other stays large: size alone is not a reason
to split. `jadpo domain rename`, `split` and `merge` do the moves and update
every call.

A domain contains only these folders, each named after its declaration
keyword, and only when they have files. There are no subfolders; that is
to be revisited if a folder in a real application passes about 60 files.

| Folder | Holds | Notes |
|---|---|---|
| `entities/` | One entity per file: its fields, unique rules, lifecycle, policy and record caches. The generated operations (`Todo.get`, create, update, delete) belong to it | Queries, actions and failures each have their own folder ([Failures](04-failures.md), [Data](05-data.md)) |
| `types/` | Types used by more than one file in the domain | A type one file uses lives beside its user |
| `failures/` | One failure per file, every failure the domain raises | A failure two or more domains raise goes in `shared/failures/` ([Failures](04-failures.md)) |
| `actions/` | Every action, one per file | Called by name (`complete_todo(todo_id)`); called by triggers or, if `public`, by other domains |
| `queries/` | Every query, one per file, whatever it reads; a query's cache sits in its file | [Data](05-data.md) |
| `libraries/` | Named groups of helper functions and constants, such as `library rounding` | Called like the standard library: `rounding.half_even(x)`, `pricing.vat_rate` ([Constants](03-logic.md#constants)) |
| `routes/` | Named route groups with a prefix the compiler checks | The HTTP trigger: the URL is written in the source |
| `schedules/` | Time triggers: when to run which action | |
| `events/` | Event definitions | |
| `subscribers/` | Event triggers, including long-running coordination | |
| (none) | Outside APIs are provider packages: connections in `app.jadpo`, installed packages in the root `packages/`, pinned in `jadpo.lock`, and the app's own in `shared/providers/` | [Integrations](11-integrations.md); there is no `integrations/` folder and no `integration` keyword |
| `emails/` | Email types and their templates, one email per name (`welcome_email.jadpo`, `welcome_email.html`, `welcome_email.txt`), and the domain's own email partials, laid out the same way | [Email](16-built-in-services.md#email) |
| `translations/` | One `type X = Message` per file; one `translations { }` language file per locale, named after it (`de_de.jadpo`), wording the domain's messages, failures, enums and email subjects; and the generated, committed `sources.jadpo_record` ([Records and manifest](18-records-and-manifest.md)) | [Configuration and time](10-configuration-and-time.md); a message a second domain uses goes in `shared/translations/`. A language file is named after its locale, not after its `translations { }` |
| `prompts/` | One `type X = Prompt` per file, each with its text in `<name>.prompt` beside it, in two sections, `# Instructions` and `# Request` | [Prompts as files](21-ai.md#prompts-as-files); a prompt a second domain uses goes in `shared/prompts/`, as a message does |
| `documents/` | Document types and their templates, one document per name (`invoice_document.jadpo`, `invoice_document.html`), rendered to HTML or PDF by the `documents` capability | [Documents](16-built-in-services.md#documents) |
| `fixtures/` | Test data shared by several test files in the domain | |

A domain created is a boundary other code is shaped around, so it needs
a person's [acceptance](18-records-and-manifest.md#acceptances),
`billing: DomainChange.created`, in the change that adds its folder.

Not built yet: `jadpo domain rename`, `split` and `merge`, so a domain
renamed by hand shows as one created; and the `documents` capability, so
until it is, a file in `documents/` is `layout.unknown_location`.

## Files and names
<!-- topic: files-and-names -->

### One primary declaration per file

Each file holds one primary declaration, named after it:
`ServiceCredential` lives in `service_credential.jadpo`. Route groups and
libraries are named declarations, so the same rule covers them.

Supporting types, failures and helpers that only that file uses live in it
too. A declaration any other file uses, apart from the file's own tests,
has a file of its own named after it. So every name a `use` line brings in
is in `<folder>/<name>.jadpo` (see [Imports](#imports)).

Everything in a file besides its primary declaration is used by the
primary declaration, directly or through one another. A declaration that
only the file's tests use, or that nothing uses, is dead code, reported as
`layout.unused_declaration`, as an unused shared declaration is. So a file
never collects helpers for something else. A declaration another file uses
is reported by `layout.promotion_required`, which moves it, not as unused.

The primary declaration is the one of the folder's kind named after the
file, or the first of that kind when the file is misnamed. In
`database/seeds/` it is the seed. Test files, root files and loose files
have no primary declaration, and a library's functions keep their own
rule: a function nothing calls is unused.

While the primary declaration, or one it uses, writes a name that names
nothing, perhaps a misspelling of the one it meant, nothing beside it is
called unused, as with imports.

### Order within a file

Definitions come before their use. After any opening comment and the `use`
lines, every declaration in a file comes before the first one that uses
it. So the primary declaration comes last, and you read it with the
constants and types it needs already in view.

When two declarations could go in either order:

- They go by kind: constants, then types, then failures, then helpers.
- Within a kind, they go in the order the primary declaration first
  reaches them. Reaching follows the primary's names in the order it first
  writes them, into each declaration before the next.
- Declarations that refer to each other keep their written order among
  themselves, and constants written together stay together.
- A declaration nothing reaches (reported as `layout.unused_declaration`)
  goes after the reached ones of its kind, in written order.
- A declaration still waits for what it uses, whatever its kind, so a
  constant of type `Stock` comes after `Stock`.

`jadpo fmt` writes this order, and `jadpo fmt --check` fails on a file out
of it. A file with no primary declaration (a test file, a root file, a
loose file) keeps its written order.

### Names are unique

A name must convert to its file name and back unchanged, so a name can't
have two capitals in a row: `VatRate`, not `VATRate`.

Names are unique across the whole project and across kinds, with one
exception. A private `function` or `type` needs a name unique only within
its domain, whether it has a file of its own or sits beside the
declaration that uses it. A private `constant` follows the same rule
([Constants](03-logic.md#constants)). A failure or an email
(`type X = Failure` or `EmailMessage`) doesn't get the exception.

Everything else stays unique across the project:

- anything `public`, and anything in `shared/` or the root files;
- every other kind of declaration, whatever its visibility, since most
  have a name outside the code: entities (tables), failures (public error
  codes), events, actions, queries, routes, schedules, subscribers (stored
  deliveries and schedule state name them), services, fixtures and emails;
- a type an API's record names, private or not: a route's input or output,
  or a type reached through their fields, is an OpenAPI schema name;
- library names, which also qualify their functions (`rounding.half_even`).

A name unique across the project is unique against the per-domain ones
too. A private `Address` in `billing` clashes with a public or shared
`Address`, while two private `Address` types in `billing` and `shipping`
don't. Two names folding to one file name (`naming.folded_collision`)
follow the same scope. So a search for a name finds one file, or one file
per domain for a private function or type.

Domain, library and standard-library names are unique among themselves,
and local variables can't reuse them. Standard-library names are fixed per
language version, so a new standard namespace never breaks an existing
library.

A private name can be imported only inside its own domain, where it is
unique, and anything importable across domains is unique across the
project. So `import.missing` can always name the exact `use` line.

A domain is not a namespace: `Customer` means the same thing whichever
folder it appears in.

There is one exception to file names being unique. A type declared in the
same file as an action, query or function may fold to that callable's
name, as an input does: `type CreateTodo = Input` beside
`action create_todo` in `actions/create_todo.jadpo`. Only one file carries
the name. If the type is promoted to its own file, the rule applies again.

### When two names clash

A clash is `semantic.duplicate_declaration`. It says whether the two are a
copy (use the existing one, or move it to `shared/`) or two things with
one name (rename one). It never invents a name, and it lists the names in
use that contain the same word. `jadpo rename` refuses a new name already
in use before editing anything.

An agent working in parallel with others merges `main` before pushing,
runs `jadpo check` (it builds nothing, so it is quick), fixes any clash,
then pushes. Generated CI runs `jadpo check` first, so a clash fails in
seconds ([Tools for agents](14-tooling.md#tools-for-agents)).

### Tests and other files

Tests sit beside their subject as `<name>.test.jadpo`. Scenarios that span
several domains go in the root `tests/`. The kind of test (pure, action,
route, schedule) is known from what it calls, so it isn't a folder;
`jadpo test --kind route` runs one kind.

Files that aren't Jadpo (provider contracts, email templates, search
mappings) sit beside the file that uses them, and that file must refer to
them.

Not built yet: `jadpo rename`.

## Sharing and promotion
<!-- topic: sharing-and-promotion -->

A type, failure or helper used by one file lives in that file. When a
second file needs it, it moves to a file of its own, a step called
promotion:

- When a second file in the same domain uses it, it moves to the domain's
  `types/`, `failures/` or `libraries/`.
- When a second domain uses it, it moves to `shared/`. Only pure types,
  failures, libraries and messages go there; a message moves to
  `shared/translations/`.

Moving to the right folder isn't enough: the file must be named after the
declaration. `DiscountCode` in `types/basket.jadpo`, used by another file,
must move to `types/discount_code.jadpo`.

Tests never count as users. A route counts only when nothing else uses the
type, and a failure counts where it is raised.

A constant ([Constants](03-logic.md#constants)) is placed as a helper
function is. Used by one file, it sits at the top of that file, never as a
primary declaration. Used by a second, it moves into a library and is read
through it, as in `pricing.vat_rate`, so a library holds functions and
constants.

### What `shared/` holds besides

- `shared/emails/` holds only the app's optional email layout,
  `layout.html` and `layout.txt`, and the email partials every domain's
  emails may include, each a `type X = EmailPartial` with its templates
  beside it ([Email](16-built-in-services.md#email)).
- `shared/documents/` holds only the app's optional document layout,
  `layout.html`.
- `shared/providers/` holds the app's own integrations, each in
  `shared/providers/<name>/<version>/`, in exactly the package format
  ([The package format](11-integrations.md#the-package-format)). They
  aren't promoted there: they are written there from the start.

### How a promotion happens

Promotion is required. The compiler reports it as
`layout.promotion_required` with a one-step fix, applied through the
editor or `jadpo fmt --layout`, never silently on save.

Demotion is never forced. A promoted declaration stays where it is until
nothing uses it, when it becomes a dead-code error. So code doesn't move
back and forth, `git blame` keeps working and moves don't cause merge
conflicts.

Promotion is a move, not a rewrite. The library keeps its name, so calls
such as `rounding.half_even(x)` and the function's text stay identical,
and behaviour can't change.

Promoting a private function or type to `shared/` makes its name unique
across the project, so a private one of the same name in another domain
then clashes, as `semantic.duplicate_declaration`.

The move is also recorded in the identity registry,
`database/identities.jadpo_record`: the committed file that gives every
table, column and declaration a lasting identity, so a rename reads as a
rename ([Stable identity](18-records-and-manifest.md#stable-identity)). The
identity key of a function or type declared in a domain includes the
domain, so the move writes one line there, and it reads as a move, not as
a removal and an addition. `layout.promotion_required`'s edits for a type
moving to `shared/types/` add that line, so `jadpo fmt --layout` writes it.
A failure raised by a second domain gets no edit, so moving it by hand
writes no line. A library keeps its name in `shared/`, so its functions'
keys need none.

A move whose new file would carry another declaration's name too gets no
edit either: `ListBooks`, beside query `list_books` in
`queries/list_books.jadpo` and used by another query, would move to
`types/list_books.jadpo`, which is `naming.folded_collision` with the
query. `layout.promotion_required` then says to rename the moving
declaration first, and `jadpo fmt --layout` refuses the move, reporting it,
until the names differ.

### Helpers that depend on a domain

A helper that depends on one domain's types (`order_math.total(order)`)
stays in that domain, and becomes public when another domain calls it. A
helper that depends on two domains' types is an error that needs a design
decision. Making a helper generic, for example extracting
`pricing.sum(amounts)` for `ordering` and `payments` to share, is a
deliberate refactor.

### Finding the helper that already exists

Two constants of the same type holding the same value are the declinable
`constant.similar_exists`, so parallel agents each inventing `vat_rate`
find each other's ([Constants](03-logic.md#constants)).

Duplicate helpers are caught as they are written. When a new helper looks
like an existing one, `jadpo check` says so and names it ("a similar
library exists: `rounding.half_even` in
`shared/libraries/rounding.jadpo`"). The compiler also generates a
catalogue of every library function, with its signature, documentation
comment and callers, and the Jadpo agent skill tells agents to consult it
before writing a helper. Agents look things up in the compiler's project
map, the language server, rather than browsing folders.

Placement alone can't prevent two copies: two domains may each hold a
private helper with the same name.

A function's description is written once, as a `///` comment above it. The
catalogue, the list of callers and the domains that use it are computed
and shown in the editor, never written into source files.

### Seeing a change's reach

Before shared or public code changes, its reach shows in three ways:
find-references in the editor; a `jadpo impact <name>` command listing the
calling domains, the entry points and the tests that cover it; and
`jadpo check` itself, which reports the domains a change affects and runs
their tests.

Not built yet: `jadpo impact`.

## Ownership and calls replace modules
<!-- topic: ownership-and-calls-replace-modules -->

A domain owns everything in it, and it is the boundary of what other code
can see. Declarations are private to their domain unless they are marked
`public`.

### What can be public

Entity identities, types, failures, events, queries, libraries and actions
can be public. So can a fixture, which is still callable only from tests,
fixtures and seeds: another domain's tests call a public fixture through
its domain, as `planning.alice_with_todos()`
([Shared test data: fixtures](13-testing.md#shared-test-data-fixtures)).

`public` on anything else (a function, a route group, a schedule, a
subscriber, a test or a seed) is `visibility.public_not_allowed`.

`public` on a declaration nothing outside its domain uses is
`visibility.public_unused`. A use counts if it comes from another domain's
code or tests, from a root `tests/` scenario, or from the signature of a
public declaration one of those uses. The domain's own tests don't count.

**Everything reachable from a public declaration is public.** A type in a
public action's, query's, library's or event's signature, and a failure it
can raise, must itself be public or shared. This is checked on every
compile. An internal failure is mapped or handled before it leaves the
domain. A package's public surface follows the same rule
([What is public](11-integrations.md#what-is-public)).

### Changing another domain's data

A public action is a domain's way for others to change its data. Checkout
in `ordering` can call `inventory.reserve_stock(...)`, and both changes
commit or roll back together, in one database transaction. Another domain
never touches the entities directly. If a domain ever moves to a database
of its own, the compiler refuses the shared transaction rather than lose
the all-or-nothing guarantee.

Events are for reactions that don't need to be all-or-nothing, such as
sending a confirmation email. They run after the commit, so a failed email
never rolls back an order.

### Calls name their owner

The part before the dot names the owner of what you call:
`temporal.in_zone(x)`, `rounding.half_even(x)`, `Todo.get(id)`,
`inventory.reserve_stock(...)`. Your own domain's actions and queries are
called by name alone: `complete_todo(todo_id)`. An own-domain action or
query called as `planning.snooze_todo()` from inside `planning` is
`semantic.own_domain_qualification`.

Searching for `inventory.` finds the calls into that domain. It doesn't
find generated entity operations such as `Order.get`, which are named by
their entity.

A call names one owner. A library is called by its own name, never through
its domain, so `pantry.sacks.grams_in(1)` or `pantry.nonexistent.thing(x)`
is `semantic.unknown_callee`. Only a connection's operations take three
names ([Calling](11-integrations.md#calling)). A call that reaches nothing
is always reported, never left to fail when it runs.

There are no `module` headers. Each file names where its names come from
on `use` lines: see [Imports](#imports).

## Imports
<!-- topic: imports -->

With imports, the place every name comes from can be read from the file
itself. Without them, a name in one file could be declared in that file,
in its domain's `types/`, `failures/` or `entities/`, in `shared/`, or in
another domain, and you or an agent would have to search for it.

Each `use` line names a folder, then the names it brings in, written
exactly as the code uses them, so finding a name on the page finds its
import. In `domains/sales/actions/price_basket.jadpo`:

```jadpo fragment
use shared/types { Percent, Sterling }
use shared/libraries { pricing }
use sales/types { Basket, DiscountCode, Receipt }
use sales/failures { BasketEmpty, CodeUnknown, DiscountTooLarge }
use sales/actions { price_lines }
use menu
```

The braces hold a list separated by commas, the only one in the language:
every other block holds one `name: value` item per line.

### What is imported

- **Every name not declared in the file is imported**, your own domain's
  included. A name brought in from `sales/types` is in
  `domains/sales/types/<name>.jadpo`, and one from `shared/types` is in
  `shared/types/<name>.jadpo`. The path leaves out `domains/`, so `shared`,
  `app`, `config`, `authentication`, `policy` and `flags` can't name a
  domain.
- **What the project declares is imported, root files included.** A
  connection in `app.jadpo` comes in as `use app { ... }`.
- **Built-ins are never imported.** Built-in types (`Text`, `Int`,
  `List<T>`) and standard-library namespaces (`list.count(...)`) are fixed
  per language version, and a project can't declare them. An entity's
  generated operations (`Todo.get(id)`) come with the entity.
- **The generated namespaces** (`clock`, `config`) are called through
  their names, as the standard library is, and need no import.
- **Calls into another domain keep the domain's name at the call.**
  `use menu` brings in the domain, and its public actions, queries and
  fixtures are called as `menu.stock_item(...)`, so a call that crosses
  domains shows on its own line. Another domain's public types and
  failures come in by folder like any other name:
  `use menu/types { MenuItem }`.
- **A test file sees everything its subject file declares** without
  importing it, so `stock_item.test.jadpo` calls the `loaf_stock` helper
  in `stock_item.jadpo` directly. Anything else it uses is imported.
- **An entity's provided not-found failure**
  ([Reads and writes](06-reads-and-writes.md)) comes in from the entity's
  folder: `use planning/entities { Todo, TodoNotFound }`.

So a name you read has one of three answers: it is on a `use` line, so it
is in `<folder>/<name>.jadpo`; it is a built-in; or it is declared in this
file.

The standard library's namespaces are `list`, `set`, `map`, `range`,
`integer`, `decimal`, `money`, `units`, `text`, `markup`, `hash`,
`encoding`, `crypto`, `random`, `json`, `csv`, `tsv`, `json_lines`,
`temporal`, `locale`, `colour`, `media`, `web`, `semver`, `geo` and `ai`
([Standard library](19-standard-library.md)). Each is reserved by
`semantic.standard_namespace_reserved`, so nothing in a project can take
its name. Fields and named arguments keep every word: `note.text` and
`set:` are fine.

### The formatter keeps imports

`jadpo fmt`, or the editor, adds a missing import and removes an unused
one, one line per folder, sorted. Every name a file can import is unique
where it can be imported: a private function or type within its own
domain, anything importable across domains across the project. So the
import for a name is never in doubt, and a forgotten one is fixed for you
rather than being a compile failure. `jadpo check` still reports an import
that names nothing, and one the file doesn't use.

Go to definition works from any use of a name, in a body, a signature or a
`use` line, and opens its declaration in its `.jadpo` file.

### Where the lines go

`use` lines open the file, below the comments that open it, up to a blank
line. `use` is a reserved word, so nothing is named `use`.

`jadpo fmt` sorts them as the example above is:

- names before domains;
- from the root files, through `shared/`, to each domain's folders by
  domain name;
- a domain's or `shared/`'s folders in the order the layout lists them
  (`entities/`, `types/`, `failures/` and so on);
- then the `use <domain>` lines.

The names in the braces are sorted, and a line too long for 100 columns
puts one name per line.

### What `jadpo check` reports

Each of these comes with the edit that fixes it:

- `import.missing`: a name used without its import.
- `import.unknown`: a line naming a folder, domain or name that isn't
  there. A name imported from the wrong folder gets the edit that moves it
  to the right one.
- `import.unused`: an import the file doesn't use.
- `import.unneeded`: an import of a built-in, of the file's own
  declaration, of its test subject's, or of its own domain.
- `import.duplicate`: a name or folder written twice.

Using another domain's private name is still
`visibility.private_cross_domain` where the file uses it. A call that
reaches nothing is still reported by name resolution, as an unknown name
or callee, not as an import problem: Jadpo never asks you to import
something the file can't use.

`jadpo fmt` fixes imports, then formats, by applying those edits, once
every file in the project parses. `jadpo fmt --check` fails on a file whose
imports it would change.

### Edge cases

- **A misspelling is one error.** While a name the file writes names
  nothing, no import is called unused, since the name may be a misspelt
  import, and `jadpo fmt` removes none either.
- **Before a promotion.** A declaration in a file named after another,
  such as `SnoozeDays` beside `snooze_todo`, comes in from the folder its
  file is in (`use planning/actions { SnoozeDays }`) while
  `layout.promotion_required` asks for the move. The move's edit makes it,
  and `jadpo fmt` then moves each import to the name's new folder.
  `jadpo fmt --layout` does both in one step.
- **Loose files** have no layout, so there are no folders to import from.
  Every name in them is seen without a `use` line, and a `use` line there
  is `import.unknown`.
- **A migration's `changes.jadpo`**, which `jadpo migrate new` writes and
  which names what changed (`Todo.rank: { ... }`), imports nothing either.

## Database
<!-- topic: database -->

- `database/identities.jadpo_record` is the identity registry. It gives
  every table, column and index a stable identity, so renaming a field
  renames its column instead of dropping it. It records renames, of data
  and code alike ([Stable identity](18-records-and-manifest.md#stable-identity)).
  The compiler maintains it, and it is committed and reviewed. It holds
  one sorted entry per line, so two branches that change the schema merge
  readably.
- **Committed generated files share one extension, `.jadpo_record`**, and
  stay next to what they describe: `api/<api>.jadpo_record`,
  `database/identities.jadpo_record`, each `translations/sources.jadpo_record`
  and each installed package's `surface.jadpo_record`. Each is in Jadpo's
  readable format and starts with a line saying what wrote it and which
  command updates it. `jadpo check` regenerates it, and a hand edit is an
  error naming that command. `jadpo.lock` keeps its name. `jadpo records`
  lists them all and flags any that are out of date
  ([What is committed, and why](18-records-and-manifest.md#what-is-committed-and-why)).
  Files people write, such as acceptances (a person's recorded acceptance
  of a breaking change, see
  [Acceptances](18-records-and-manifest.md#acceptances)), aren't records.
- Each schema change is a folder under `database/migrations/`, named by
  date, time and purpose. It holds your decisions on destructive changes,
  the generated SQL for each database, and any backfill of data written in
  Jadpo. The compiler keeps a record of the latest migration, so history
  stays linear.
- `database/seeds/` holds demonstration and development data for your
  machine.

## What it looks like in code
<!-- topic: what-it-looks-like-in-code -->

Sketches of how the pieces sit in their files. Each piece's own page gives
its full syntax.

A library with its documentation comment, in
`shared/libraries/rounding.jadpo`:

```jadpo fragment
/// Rounds to the nearest whole penny, ties to the even penny.
library rounding {
    function half_even(amount: Price) -> Price { ... }
}
```

A call to it from another domain, in
`domains/ordering/actions/checkout.jadpo`
([Constants](03-logic.md#constants) covers libraries' constants too):

```jadpo fragment
var total: Price = rounding.half_even(subtotal)
```

A public action, in `domains/inventory/actions/reserve_for_cart.jadpo`:

```jadpo fragment
public action reserve_for_cart(cart_id: Cart.id) fails OutOfStock -> Reservation { ... }
```

Another domain calls it inside its own transaction, in
`domains/ordering/actions/checkout.jadpo`
([Reads and writes](06-reads-and-writes.md)):

```jadpo fragment
action checkout(cart_id: Cart.id) fails OutOfStock, CartEmpty -> Order {
    var reservation: Reservation = attempt inventory.reserve_for_cart(cart_id)
    var order: Order = attempt place_order(cart_id, reservation)
    return order
}
```

A named route group with a checked prefix, in
`domains/planning/routes/todos.jadpo` ([HTTP](07-http.md)):

```jadpo fragment
routes Todos {
    prefix: /todos

    GET / { ... }
    GET /{todo_id} { ... }
    PATCH /{todo_id} { ... }
}
```

A schedule, in `domains/planning/schedules/overdue_reminders.jadpo`
([Jobs and events](12-jobs-and-events.md)):

```jadpo fragment
schedule overdue_reminders {
    every: 15m
    run: send_due_reminders()
}
```

A subscriber in another domain, in
`domains/notifications/subscribers/send_order_confirmation.jadpo`:

```jadpo fragment
subscriber send_order_confirmation {
    on: OrderPlaced
    run: send_order_confirmation_email(event)
}
```

A connection to a provider package, pinned to the provider's API version,
in `app.jadpo` ([Integrations](11-integrations.md)):

```jadpo fragment
connections: {
    stripe: {
        package: Stripe.v2024_06_20
        credential: config.stripe_secret_key
        domains: [payments]
    }
}
```

## Folders other pages describe
<!-- topic: decided-in-later-areas -->

The layout only reserves these folders; each piece has its own page. Route
groups are on [HTTP](07-http.md), schedules on
[Jobs and events](12-jobs-and-events.md), integrations and provider API
versions on [Integrations](11-integrations.md), translation files on
[Configuration and time](10-configuration-and-time.md), and the migration
workflow on [Data](05-data.md).

## What is built
<!-- topic: project-layout-and-modules-built -->

The compiler builds the decided layout, with each rule's real output under
[Errors](#errors).

| Feature | Status |
|---|---|
| Root, domains and their folders | Built: a file where the layout has no place for it is `layout.unknown_location`, and a declaration in the wrong folder `layout.wrong_folder`; a folder or file with no `app.jadpo` above it is checked as loose files. A domain created needs an [acceptance](18-records-and-manifest.md#acceptances). Not built yet: `jadpo domain rename`, `split` and `merge`; the `documents` capability, so a file in `documents/` is `layout.unknown_location` until it is. |
| Files and names | Built: one primary declaration per file, named after it; names unique project-wide; the round-trip and folded-collision rules, with the exception for a type folding to its callable's name. |
| Names per domain | Partly built: private functions and types unique only within their domain, each domain's code reading its own; their identity keys carrying the domain, with promotion to `shared/` writing a registry line; `semantic.duplicate_declaration` saying copy or clash and listing the names in use with the same words. `jadpo rename` is not built yet ([Commands for looking into and changing code](14-tooling.md#commands-for-looking-into-and-changing-code)). |
| Order independence | Built: declaration and file order never matter. |
| Domains, `public` and calls | Built: owner-qualified calls and per-declaration `public`, with `visibility.private_cross_domain`, `visibility.public_not_allowed` and `visibility.public_unused`. |
| Reserved words | Built: a reserved word can't name anything. |
| Comments | Built: `//` and `///` documentation comments. |
| Similar helpers | Built: `library.similar_exists`, a declinable error. Not built yet: `jadpo impact`. |
| Declinable errors | Partly built: no warning severity; the `decline <rule>: "<reason>"` line with `decline.unused`, `decline.not_declinable` and `decline.no_reason`; the catalogue's security rules, whose decline needs an [acceptance](18-records-and-manifest.md#acceptances); `jadpo declines list` and `clear`; the editor's "Decline with a reason" and "Remove decline"; and the date rule for an expired [feature flag](16-built-in-services.md#feature-flags), `flag.expired`, judged at today by `check` and at the commit's date by `build`. Not built yet: `jadpo approval` ([Records and manifest](18-records-and-manifest.md)). |
| Constants | Built: `constant` as a reserved word and a declaration placed and shared as helper functions are, a private one unique within its domain, `constant.similar_exists`, and a top-level `var` pointed at `constant` ([Constants](03-logic.md#constants)). |
| Imports | Built: `use <folder> { Names }` and `use <domain>`, with `import.missing`, `import.unknown`, `import.unused`, `import.unneeded` and `import.duplicate`; `jadpo fmt` adds, removes and sorts imports, `jadpo fmt --layout` makes promotion moves, and go to definition works from every use of a name, a `use` line's included. |
| A file holds only what its primary declaration uses | Built: `layout.unused_declaration` for a supporting declaration the primary declaration doesn't use, directly or through one another. |
| Definition before use | Built: `jadpo fmt` writes each file's declarations before the first one that uses them, and `jadpo fmt --check` fails on a file out of that order. |
| Referenced assets | Not built yet: `layout.asset_unreferenced` waits for the ways [Integrations](11-integrations.md) and [Built-in services](16-built-in-services.md) refer to files that aren't Jadpo. |
| Jadpo version | Built: `application { jadpo: "X.Y" }`, with `toolchain.version_newer` refusing a project on a later line, `toolchain.version_older` one on an earlier line, each naming the release to install, and `toolchain.version_invalid`; the installed `jadpo` runs the release the line or `jadpo.lock`'s pin names ([Which jadpo runs](14-tooling.md#which-jadpo-runs)). `jadpo new` writes the line of the `jadpo` that ran it. |
| `application { }`'s sections | Built: the one list of sections ([What `application { }` holds](#what-application---holds)), `semantic.application_section_unknown` for any other, with the edit to the section a near miss meant, and `toolchain.not_built_yet` for the seven decided and not built. |
| Packages | Built: the root `packages/` and `jadpo.lock`, `shared/providers/`, packages checked as `shared/` is, `use packages/<name>`, and the reachable-from-public rule (`visibility.public_reaches_private`) in domains and packages; [Integrations](11-integrations.md) records the choices. |

## Errors

Every error this page's rules can produce, with an example that triggers it
and the compiler's real output from `jadpo check`. An example without
`app.jadpo` is checked as loose files, one implicit domain with the layout
rules off; the layout examples are small projects.

- [`toolchain.empty_project`](../errors/toolchain.empty_project.md)
- [`toolchain.version_newer`](../errors/toolchain.version_newer.md)
- [`toolchain.version_older`](../errors/toolchain.version_older.md)
- [`toolchain.version_invalid`](../errors/toolchain.version_invalid.md)
- [`semantic.application_section_unknown`](../errors/semantic.application_section_unknown.md)
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

People and agents write constructs from other languages out of habit. Each
has an error of its own that names the habit and the Jadpo form, with an
edit. Parsing carries on as if the Jadpo form had been written, so a line
gives one error. When the Jadpo form only renames or drops words, the rest
of the project is checked too. The catalogue is `jadpo_syntax::habits`;
[Habits from other languages](03-logic.md#habits-from-other-languages) on
the Logic page explains how a line shows one error at a time.

- [`syntax.comment_style`](../errors/syntax.comment_style.md)
- [`syntax.import_statement`](../errors/syntax.import_statement.md)
- [`syntax.export_keyword`](../errors/syntax.export_keyword.md)
