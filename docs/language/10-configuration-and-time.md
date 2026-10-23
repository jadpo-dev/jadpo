---
title: Configuration and time
area: 10
status: decided
checked-with: jadpo 3ce094d0 (2026-10-12)
rules:
  - configuration.binding_empty
  - configuration.binding_invalid
  - configuration.client_address_duplicate
  - configuration.client_address_hops_invalid
  - configuration.client_address_hops_unused
  - configuration.client_address_invalid
  - configuration.client_address_source_unknown
  - configuration.default_invalid
  - configuration.default_literal_required
  - configuration.default_type
  - configuration.duplicate_binding
  - configuration.duplicate_option
  - configuration.expected_field
  - configuration.field_unknown
  - configuration.multiple_declarations
  - configuration.secret_default
  - configuration.secret_flow
  - configuration.secret_invalid
  - configuration.test_value_missing
  - configuration.type_unsupported
  - configuration.unknown_option
  - configuration.url_type
  - type.clock_context
  - type.temporal_clock_invalid
  - type.temporal_option_unused
  - type.temporal_zone_required
  - type.temporal_format_component_duplicate
  - type.temporal_format_component_invalid
  - type.temporal_format_components_empty
  - type.temporal_format_invalid
  - type.temporal_format_requires_time
  - type.temporal_locales_required
  - type.locale_default_unsupported
  - type.locale_duplicate
  - type.locale_invalid
  - type.locale_unsupported
  - syntax.locales_default_required
  - syntax.locales_duplicate_option
  - syntax.locales_supported_required
  - syntax.locales_unknown_option
  - semantic.multiple_locale_declarations
  - type.presentation_text_persistence
  - translation.feature_off
  - translation.field_type
  - translation.field_unused
  - translation.file_missing
  - translation.file_name_invalid
  - translation.jadpo_missing
  - translation.locale_unsupported
  - translation.locales_required
  - translation.member_duplicate
  - translation.member_unknown
  - translation.message_unknown
  - translation.option_unknown
  - translation.group_unknown
  - translation.placeholder_style
  - translation.placeholder_unknown
  - translation.review_invalid
  - translation.sources_invalid
  - translation.stale
  - translation.too_many_selectors
  - translation.variant_missing
  - translation.variant_unknown
  - translation.wording_invalid
  - translation.wording_missing
  - translation.wording_shape
  - type.locale_in_progress_default
  - type.locale_in_progress_unsupported
---

# Configuration and time

How an application declares its settings and secrets, where their values come
from, and how code reads the current time, works with dates and zones, and
shows times to people in their own language.

> Decided on 2026-10-12. Code on this page applies the decisions from areas 1 to 10. The new compiler has built the time literals and the built-in `Zone`, `Weekday` and `Locale` enums (chunk 1.9) and checks where `clock.now` may be read. Chunk 5.1 built settings: `config.jadpo`, secrets, signing keys, the connections' addresses, `jadpo config`, the check before the app starts and a test's own values; chunk 5.1c built the client's address behind trusted proxies (`client_address:`). Chunk 5.2 built time: `ZonedTime`, durations, arithmetic and comparison, the `temporal` library over a bundled time zone database, and a test's clock; chunk 5.2b built the formats and `PresentationText`, and checks `temporal.format` and `format_relative`. The examples marked `check` in those sections and their errors run with this checkout's `jadpo`. Chunk 5.3 built locales (`locales { }`, checked wherever a locale is written) and the text `temporal.format` and `format_relative` give, in each locale's own words, order and digits; chunk 5.3c built choosing a locale from a request, `locale.from_accept_language(header)`. Chunks 5.4c to 5.4f built translations as messages (`Message` types, `translations { }` language files, one typed call per message, the checks between locales, Jadpo's own messages and the exchange with translation services).

Two rules run through the page. **Code never reads the environment or the
system clock directly**: settings arrive as typed `config.<field>` values,
checked before the app starts, and the time arrives as `clock.now`, fixed for
the whole operation. **Machine values and text for people never mix**: an
`Instant` is stored and compared; a formatted date is only shown.

## Settings: `config.jadpo`

Every setting the application needs is a field in the one `config { }`
declaration in the root file `config.jadpo`. A field has a type, and
optionally `secret: true`, a `default:`, a `binding:` and a `test:` value.
Code reads it as `config.<field>` anywhere, functions included, because a
setting cannot change while the app runs.

```jadpo check project=settings
config {
    browser_origin: Url
    database_url: PostgresUrl
    mail_sender: Email
    session_signing_key: SigningKey
    soft_delete_retention: Duration {
        default: 30d
    }
}
```

- The declaration has no name (decided in area 1: unnamed singletons).
- Each value comes from an environment variable named after the field in
  capitals: `mail_sender` reads `MAIL_SENDER` (question 1).
- A field is required unless it has a `default:`. There are no optional
  settings: give a default instead, or leave the feature's setting required.
- There is no `env.jadpo` and no second binding file (decided in area 1).
- A setting is typed, and so is what code reads: `config.page_size` is the
  `ListLimit` the field declares, checked when the app starts, so code never
  builds it again. Declare the named type the code uses
  (`page_size: ListLimit { default: 25 }`) and read `config.page_size` where a
  `ListLimit` goes. A setting may be declared with a built-in type such as
  `Duration` or `Int`, but a signature takes named types (area 2), and
  building one from a plainly typed setting is an ordinary construction,
  which can fail and so needs `attempt`.

```jadpo check project=settings-in-code
type ListLimit = Int {
    min: 1
    max: 100
}

type FromAddress = Email {}

config {
    mail_sender: FromAddress {
        test: "reminders@example.test"
    }
    page_size: ListLimit {
        default: 25
    }
}

function first_page() -> ListLimit {
    return config.page_size
}

function from_address() -> FromAddress {
    return config.mail_sender
}
```

### Environment variable names

The variable name is the field name in capitals. Write `binding:` only when it
must differ, for example `binding: "HEROKU_POSTGRESQL_URL"` to keep a name a
hosting platform already uses. A binding is capital letters, digits and `_`,
starting with a capital letter, and no two settings read the same variable.
A `SigningKey` setting also reads its variable with `_PREVIOUS` after it
([Signing keys](#signing-keys)), so no other setting may bind that name.

```jadpo check project=binding
config {
    database_url: PostgresUrl {
        binding: "HEROKU_POSTGRESQL_URL"
    }
}
```

The cost of deriving it: renaming a field renames the variable, which breaks
a deployment that still sets the old name. A changed name is therefore shown
in the approval review like any other contract change (question 1; the review
comes with area 18).

### What a setting can hold

A setting is read from one environment variable, so its type must be decodable
from one piece of text: text-based types, `Int`, `Decimal`, `Bool`, `Uuid`,
`Email`, `Url`, `IpAddress`, `Duration`, `Instant` and `CalendarDate`, a named
type built on one of them, an enum whose variants carry no data, and the
built-in enums `Zone` and `Locale` (question 2). A `ModelId` is read from its
text, `openai/gpt-4.1-mini`, since every model `ai:` names is one (area 21;
chunk 5.5v3), and a `Money` from its canonical text, `"GBP 20.00"`, since
`ai: { limits: { per_day: { cost: } } }` names one (area 21; chunk 5.5v5). A
`Certificate` is read from its PEM, since a database connection's
`authority:` names one (area 5; chunk 8.1n).
Lists, objects, maps, `T?` and data-carrying enums are rejected
(`configuration.type_unsupported`): split them into fields.

```jadpo check project=settings-enums
type Verbosity = Enum {
    verbose
    normal
    terse
}

config {
    business_zone: Zone {
        default: Zone.europe_london
    }
    log_level: Verbosity {
        default: Verbosity.normal
    }
}
```

`LOG_LEVEL=verbose` and `BUSINESS_ZONE=Europe/London` are valid values: an enum
is read as the variant's name, a `Zone` as its IANA name and a `Locale` as its
BCP 47 tag (`en-GB`). A `Duration` is written as it is in source (`30s`, `5m`,
`2h`, `30d`) or as its ISO 8601 text (`PT5M`, built in chunk 5.5w2), a `Bool`
as `true` or `false`, and an `Instant` (ending in `Z`, never with an offset)
and a `CalendarDate` in ISO 8601. A variable set to nothing counts as not set.

### Defaults and test values

A default is a literal of the field's type, checked while compiling
(`default: 25`, `default: 5s`, `default: Verbosity.normal`), against every rule of
the type and the types it is built on. The same is true of `test:`, the value
every test reads (below); a field that has none of its own default and is read
by a test needs one. Neither names a constant (area 3,
[Constants](03-logic.md#constants)), though it is fixed while compiling: a
value is a constant or a setting, never both (below; revised 2026-10-16,
Richard).

A secret cannot have either, so a forgotten production secret stops the app
instead of falling back to a value written in source, and secrets never appear
in test source.

### A constant or a setting

> Decided by Richard on 2026-10-16. Constants are built (chunk 5.3e, area
> 3), and so is the refusal of one as a `default:`; the move below, `jadpo
> config move`, was built in chunk 5.3f, and its examples are checked.

A value that could differ between deployments is a setting; one that never
does is a `constant` (area 3). A value is one or the other, never both, so
it has one source and one way to read it: a constant is read as
`security.max_login_attempts`, a setting as `config.max_login_attempts`.
That is why a `default:` never names a constant. If one could, code reading
the constant directly would quietly ignore the environment: with
`security.standard_attempts` (5) as `max_login_attempts`'s default,
someone sets `MAX_LOGIN_ATTEMPTS=10`, and code reading
`security.standard_attempts` still allows 5. That is the familiar `DEFAULT_PORT` bug of TypeScript and
pydantic code, where a constant seeds a setting and half the code reads the
constant. Jadpo has no need for the pattern: `default:` on the field
already names the default, in the one place a reader looks.

The three tiers, from fixed to live:

| Tier | Where | Changes | Read as |
|---|---|---|---|
| `constant` | Beside its use, or in a library (area 3) | With a code change | `security.max_login_attempts` |
| Setting | `config { }` in `config.jadpo` | Per deployment, when the app starts | `config.max_login_attempts` |
| Flag | `flags { }` in `flags.jadpo` (area 16) | While the app runs | `flags.new_checkout` |

#### Making a constant a setting

Turning a fixed value into a per-deployment one is a move a tool makes:
`jadpo config move <constant>` on the command line, or "Make this a
setting" in the editor, on the constant's declaration or any read of it,
which runs the same code (area 14, as the editor's rename runs `jadpo
rename`). It is offered, never required, so `jadpo fmt --layout`, which
makes only the moves an error asks for, doesn't make it. In one step it:

- adds a field to the one `config { }`, before the first field whose name
  comes after it (in a new `config.jadpo` when there is none), with the
  constant's name and type, whose `default:` is the constant's value as
  written when that is a literal (`5`, `30d`, `0.2000`,
  `Verbosity.normal`) and otherwise the value the compiler worked out, and
  whose `///` lines are the constant's;
- deletes the constant, and a library whose only member it was, and a file
  left holding nothing (with its folder, if that is left empty);
- rewrites every read, `security.max_login_attempts` (or
  `max_login_attempts` beside its declaration) to
  `config.max_login_attempts`, and fixes the imports of every file it
  touches, removing a `use` line nothing needs any more.

`<constant>` names the constant as code reads it, `security.max_login_attempts` or
`free_delivery_from`, or after its domain
(`accounts.security.max_login_attempts`) when private constants of two
domains share the name; a name that fits two is refused, listing each.

Before, a library in the `accounts` domain holds the constant and an
action reads it:

```jadpo check project=move-before file=app.jadpo hidden
application {}
```

```jadpo check project=move-before file=domains/accounts/types/login_attempts.jadpo hidden
type LoginAttempts = Int {
    min: 1
    max: 100
}
```

```jadpo check project=move-before file=domains/accounts/types/account_locked.jadpo hidden
type AccountLocked = Bool {}
```

```jadpo check project=move-before file=domains/accounts/libraries/security.jadpo
use accounts/types { LoginAttempts }

library security {
    /// Failed sign-ins before an account is locked.
    constant max_login_attempts: LoginAttempts = 5
}
```

```jadpo check project=move-before file=domains/accounts/actions/sign_in.jadpo
use accounts/types { AccountLocked, LoginAttempts }
use accounts/libraries { security }

action sign_in(failed_attempts: LoginAttempts) -> AccountLocked {
    return AccountLocked(failed_attempts >= security.max_login_attempts)
}
```

The move lists what it changed, file by file (the setting added or filled,
the constant and anything removed with it, and each read by line; imports
are left to the diff), then the setting and the variable a deployment can
now set:

```text
$ jadpo config move security.max_login_attempts
  config.jadpo
    + max_login_attempts  setting, default 5

  domains/accounts/libraries/security.jadpo
    - max_login_attempts  constant
    - security            library, nothing left in it
    - security.jadpo      file, nothing left in it

  domains/accounts/actions/sign_in.jadpo
    ~ line 5              reads config.max_login_attempts

Made security.max_login_attempts a setting
    setting   config.max_login_attempts
    variable  MAX_LOGIN_ATTEMPTS
```

After, the library held nothing else, so it and its file are gone, and
the imports follow:

```jadpo check project=move-after file=app.jadpo hidden
application {}
```

```jadpo check project=move-after file=domains/accounts/types/login_attempts.jadpo hidden
type LoginAttempts = Int {
    min: 1
    max: 100
}
```

```jadpo check project=move-after file=domains/accounts/types/account_locked.jadpo hidden
type AccountLocked = Bool {}
```

```jadpo check project=move-after file=config.jadpo
use accounts/types { LoginAttempts }

config {
    /// Failed sign-ins before an account is locked.
    max_login_attempts: LoginAttempts {
        default: 5
    }
}
```

```jadpo check project=move-after file=domains/accounts/actions/sign_in.jadpo
use accounts/types { AccountLocked, LoginAttempts }

action sign_in(failed_attempts: LoginAttempts) -> AccountLocked {
    return AccountLocked(failed_attempts >= config.max_login_attempts)
}
```

Every read changes, so the reviewer sees each place whose behaviour can now
differ per deployment, and the tool makes that mechanical. The new
variable, `MAX_LOGIN_ATTEMPTS`, is derived from the field's name as any
setting's is. It refuses, editing nothing, and says why:

- a constant whose type a setting can't hold ([What a setting can
  hold](#what-a-setting-can-hold): a list, an object, `T?`, an enum with
  data);
- a constant whose name a setting already has, naming that setting;
- a constant read where a setting can't be: in a query, an update by `id`,
  a backfill or another constant's value, listing each such read.

Each reason is an error of its own (`configuration.move_type_unsupported`,
`configuration.move_name_taken`, `configuration.move_read_unsupported`,
area 14's command errors), all given at once, ending `config move refused:
nothing changed`. A project with other errors is not moved: its errors are
shown as `jadpo check` shows them, and the editor offers the action as
unavailable. A result that wouldn't check (another constant left unused,
say) is undone and refused with the errors it would have; nothing is
written.

When exactly one setting's `default:` names the constant
(`configuration.default_literal_required`, below), that error alone
doesn't stop the move: that setting is the one the constant becomes, its
`default:` becomes the constant's value, and every read becomes
`config.<that setting>`.

Moving a setting back to a constant has no command: it is rare, and
removing a setting is a change to every deployment's environment, made by
hand.

Rejected (Richard, 2026-10-16):

- **A constant an environment variable can override** (`setting: true` on
  the constant). A read of it looks fixed at the call site while its value
  differs per deployment; agents would add it everywhere "just in case";
  there is no single list of the variables a deployment must set; and
  moving the file would rename the variable.
- **A `config { }` block in each domain.** Settings belong to the
  deployment, not to the code that reads them, and an operator wants one
  list.
- **Settings declared beside their use and gathered by the compiler.** The
  same missing list, and a setting's name would depend on where its first
  reader sits.
- **Values per environment in source** (`production:`, `staging:`). Against
  this page's rule that values come from the deployment's environment with
  no layering ([Values on your machine and in
  production](#values-on-your-machine-and-in-production)).

### Fields in name order

`jadpo fmt` sorts the fields of `config { }` by name, each with the comments
written above it and after it on its line, so two workspaces that each add
a setting rarely touch the same lines and merge cleanly. Nothing gives a
setting's position a meaning: a setting is read, bound and reported by
name; `jadpo config check` and the check before the app starts list
settings in the order they are written, which becomes name order.
`jadpo fmt --check` fails on a `config { }` out of order. Decided by
Richard on 2026-10-16 and built in chunk 5.3f; the examples on the
language pages are in name order.

## Secrets

> Amended 2026-10-17 by area 19 (question 9): the `key:` of a `crypto`
> operation joins the places the compiler owns, written `key:
> config.<name>` with an `EncryptionKey` (new) or `SigningKey` setting;
> any other read stays `configuration.secret_flow`. Area 19 also adds a
> generated `random` namespace, read only in actions as `clock.now` is
> (question 19). `random` built in chunk 5.5q (`type.random_context`
> beside `type.clock_context`); the `key:` place built in chunk 5.5q2,
> with `PrivateKey` as a third key setting, for `crypto.sign`.

`secret: true` marks a value that must never leave the server. The compiler
accepts it only at places the compiler itself owns: a connection's `url:`, an
integration's `credential:` (and its webhook `secret:`, area 11) and an
authentication `signing_key:` (area 8), all written in `app.jadpo` or
`authentication.jadpo`, and a `crypto` call's `key:` (area 19), written `key:
config.<name>` with an `EncryptionKey`, `SigningKey` or `PrivateKey` setting.
Nothing else in a function, action, query or test is such a place, so
reading a secret there at all (returning it, storing it, logging
it, putting it in a failure, building another value from it, passing it on
or even comparing it to something) is `configuration.secret_flow`. A value
that is always secret needs no flag: `PostgresUrl`, `RedisUrl` and
`SigningKey`.

```jadpo check project=secret-sink
config {
    database_url: PostgresUrl
}

application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

Comparisons are included: `key == config.api_key` is rejected, because a
yes/no answer leaks the secret one guess at a time. Because the secret is
never read by code, nothing printed, stored, returned or logged by the
application can hold it; `jadpo config check`, `jadpo dev` and the start-up
check say which setting and which variable, never the value.

## Database and other connection URLs

Area 5 decided that stores take their address from configuration, and area 11
put them in one `connections:` block:

```jadpo check project=connection-urls file=app.jadpo
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
        cache: {
            driver: Driver.redis
            url: config.redis_url
        }
    }
}
```

```jadpo check project=connection-urls file=config.jadpo
config {
    database_url: PostgresUrl
    redis_url: RedisUrl
}
```

`Url` accepts only `http://` and `https://`, so it cannot hold a
`postgres://user:password@host/db` address. The built-in connection types
(question 3), one per driver, can: `PostgresUrl` (`postgres://` or
`postgresql://`) and `RedisUrl` (`redis://` or `rediss://`). They are always
secret, because they normally carry a password, and a connection's `url:` must
read a setting of the type its `driver:` needs (`configuration.url_type`); an
address written in source is refused the same way. The runtime opens the
connection only through that setting.

Since chunk 9.1d2a (Richard, 2026-10-23), connection types check URI
structure as well as the scheme, using the same pure validator for literals,
startup and `config check`. Empty targets after user information, malformed
percent escapes, invalid ports and broken IPv6 brackets are invalid; checking
never resolves a name or opens a connection. PostgreSQL multi-host URIs,
encoded credentials, bracketed IPv6 and explicit socket addresses
(`postgres://%2Ftmp/app`, `postgresql:///app?host=/tmp`) pass the structural
check. The type also retains its earlier acceptance of `postgres:///app`;
that alone supplies no socket directory to a host driver and establishes no
implicit driver default. Driver-specific options and connection availability
are checked separately by the driver. A Redis address needs a host and, when supplied, a nonnegative numeric database path.
All invalid declarations are reported together; credentials are never shown.

A project that declares no database has one called `main` on PostgreSQL, and
it opens at `config.database_url` when the project declares one (and it is a
`PostgresUrl`).

A `PostgresUrl` may not set TLS itself (`sslmode` and libpq's other `ssl…`
settings), since its connection in `app.jadpo` decides (area 5, "PostgreSQL
over TLS"; chunk 8.1n): such a value is `configuration.value_invalid`,
saying so. A setting may be a `Certificate` (area 19), read from its PEM:
what a database connection's `authority:` reads, the authority its
server's certificate is signed by.

A connection to a provider package (area 11, "Connecting in `app.jadpo`")
reads its settings the same way, never a value written in source:
`credential:` a `secret: true` setting, `environment:` a
`ProviderEnvironment` setting (the built-in enum `.sandbox` or `.live`,
choosing between the package's hosts), and `host:`, for a package with no
fixed hosts, a `Url` setting. Built in chunk 7.1c; a wrong one is area 11's
`connection.value_invalid`.

```jadpo fragment
// config.jadpo
config {
    couriers_api_key: Text {
        secret: true
    }
    couriers_environment: ProviderEnvironment
}
```

## Values on your machine and in production

Production values come from the platform's environment (or its secret
manager, which sets the same variables). Locally there is one git-ignored
file, `.env.local`; Jadpo defines no `.env`, `.env.development` or layering.
The process's environment wins over the file for a variable both set, and
nothing else is read. Tests use each field's `test:` value, then its default,
and `jadpo test` makes a random throwaway value for every secret; a single
test can override a value with its `config:` setting (area 13).

A secret is never typed into a chat with an agent. The agent asks you to run:

```text
$ jadpo config set session_signing_key
? Enter SESSION_SIGNING_KEY hidden
session_signing_key configured locally
    variable  SESSION_SIGNING_KEY
    file      .env.local
```

The command asks at a prompt (never a command-line argument), hiding what is
typed when the setting is secret, checks the value against the field's type
before anything is written, and writes `.env.local` safely: only an ordinary
file, through a temporary file only its owner can read, only if the file has
not changed since it was read, and with `.env.local` added to `.gitignore`.
A value that breaks the type is refused by the rule it breaks, never by
printing it. `jadpo config check` lists each field as `set`, `default`,
`missing` or `invalid`, with the variable it reads, and never the value:

```text
$ jadpo config check
Values
  database_url         set      DATABASE_URL
  session_signing_key  missing  SESSION_SIGNING_KEY
  log_level            default  LOG_LEVEL

error: `SESSION_SIGNING_KEY` isn't set
  reason: `session_signing_key` is required: it has no `default:`, so the app can't start without a value for it.
  next: Run `jadpo config set session_signing_key` to store it in `.env.local`, or set `SESSION_SIGNING_KEY` in the environment (human)
  rule: configuration.value_missing
  help: docs/language/14-tooling.html#command-errors

config incomplete: 1 problem, 3 settings

next: jadpo config set session_signing_key  stores it in .env.local
```

Both are tooling (area 14). `config set` needs a terminal and refuses
`--json`; `config check --json` lists the same settings as data.

## Signing keys

A `SigningKey` is a built-in secret type for the key sessions and tokens are
signed with: at least 32 characters with no spaces. A setting of that type
reads its variable and, when that is set, `NAME_PREVIOUS` as well:
`session_signing_key` reads `SESSION_SIGNING_KEY` and `SESSION_SIGNING_KEY_PREVIOUS`
(question 10). New tokens are signed with the current key and both are
accepted, so a deployment rotates a key by setting the new one and moving the
old one to `_PREVIOUS`, then removing it once the longest token lifetime has
passed. Both are checked when the app starts, and neither is ever printed.
Authentication reads the key as `signing_key: config.session_signing_key`,
and with a revocation delay signs sessions and tokens with it (area 8, "The
application and revocation").

## Client addresses

Rate limits count a request by its address while no one is signed in (areas
7 and 16), and `jadpo dev`'s request lines and the request timeline (area 17)
show it. Behind a load balancer or a CDN the connection's own address is the
proxy's, so every client would share one allowance until the real address is
read from the header the proxy adds; but a header anyone can send lets a
caller choose its own address. `client_address:` in `app.jadpo` says, once,
where the real address is and how many proxies of the deployment's own to
believe (question 12):

```jadpo check project=client-address file=config.jadpo
use edge/types { ProxyHops }

config {
    proxy_hops: ProxyHops {
        default: 1
    }
}
```

```jadpo check project=client-address file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.forwarded_for
        hops: config.proxy_hops
    }
}
```

```jadpo check project=client-address file=domains/edge/types/proxy_hops.jadpo hidden
type ProxyHops = Int {
    min: 1
    max: 8
}
```

`ClientAddressSource` is a closed set of four:

| Source | The client's address is | `hops:` |
|---|---|---|
| `connection` | the connection's own address; no header is read (the native default) | not allowed |
| `forwarded_for` | in `X-Forwarded-For` | required |
| `forwarded` | in RFC 7239's `Forwarded: for=` | required |
| `cloudflare` | in `CF-Connecting-IP` (the default on Workers, the only source accepted there) | not allowed |

`hops` is how many proxies of the deployment's own sit in front of the app. It
is a count and not a list of trusted ranges, so it cannot drift when a
provider's addresses change, and it is never guessed: `forwarded_for` and
`forwarded` need it, and `connection` and `cloudflare`, which count no header
from the right, refuse it (`configuration.client_address_hops_unused`). Each
trusted proxy adds the address it saw to the
right of the list, and whatever a caller sends sits to the left of that, so
with `hops: N` the client is the N-th address from the right of the header's
addresses, every line of the header joined in order: with one proxy,
`X-Forwarded-For: 203.0.113.9` and with a caller's own claim in front,
`X-Forwarded-For: 10.9.9.9, 203.0.113.9`, are both `203.0.113.9`. If the header
has fewer than N addresses, or the one chosen is not an address, the request
counts as the connection's own address, never as a guess. `hops` is a whole
number from 1, written out (`hops: 1`) or read from an `Int` setting (or a
type built on one) that declares `min: 1` or more, so a count below 1 cannot
be started with (`configuration.client_address_hops_invalid` otherwise).

How a header is read:

- **Lines and entries.** Every header line of the name counts, in the order
  received, and each is split at commas. An empty entry still takes its
  place in the count, so a gap never moves the client left, but it is not an
  address. In `Forwarded`, each element (split at commas outside quotes)
  gives one entry, from its `for` parameter (any case; a token or a quoted
  string); an element without one is an entry that is not an address. A
  `Forwarded` header that does not parse (an unterminated quote, two `for`
  in one element) is invalid as a whole.
- **An address.** Anything `IpAddress` accepts, as IPv4 or IPv6, and in a
  header also with a port (`203.0.113.9:4711`) or in brackets
  (`[2001:db8::17]:4711`, `[2001:db8::17]`). It is kept in one spelling, so
  a client is one count whichever way a proxy writes it: IPv6 in lower case
  with the zeros shortened, and an IPv4 address written as IPv6
  (`::ffff:203.0.113.9`) as IPv4. The connection's own address is spelled
  the same way. A name, `unknown`, an obfuscated identifier (`_hidden`) or
  a zone (`fe80::1%eth0`) is not an address.
- **`cloudflare`.** Exactly one `CF-Connecting-IP` line holding one address;
  anything else falls back, as a header that is short or invalid does.
- **Falling back is whole.** The connection's address replaces the choice;
  part of a header is never mixed with it.

The address chosen is the one the default rate limits count (an IPv6
address by its /64, area 16; chunk 8.4d), `jadpo dev`'s
request lines show (after the time, muted, and as `address` with `--json`) and
the request timeline records. Natively the connection's address comes from the
host and the rest is the runtime's, so a deployable chooses an address the
way `jadpo dev` does (`jadpo_runtime::http::ClientAddress`).

On Workers there is no connection address to fall back to, and Cloudflare sets
`CF-Connecting-IP` on every request from its edge and overwrites a caller's, so
`cloudflare` is the only source accepted there and what a deployable for
Workers uses without a `client_address:`. An address Cloudflare did not give
is unknown there, and requests with no known address share one allowance. A
source other than `cloudflare` is refused when the project is built for
Workers: `jadpo check` refuses it for each deployable with `target:
Target.wasm` (`deployable.target_unsupported`, area 14; chunk 8.2m), and
since chunk 8.2d a Workers deployable that reads one doesn't start, logging
`client_address_refused` and answering every request 503
(`ClientAddress::read` refuses it for that target).

## Checking values before the app starts

The generated app reads only the declared variables, checks every value
against its type, and refuses to start if any is missing or invalid. It never
opens its port with bad configuration, and it reports the field and variable
name, never the value. `jadpo dev` runs the same check against the environment
over `.env.local`, watches `.env.local` as it does the source, and keeps the
last good version running when a change is invalid. A database that is
configured correctly but down is not a configuration error: the app starts,
reports not ready and recovers (health checks are area 11).

The check is one function in the runtime, `jadpo_runtime::config::load`, given
the plan's settings and a source of variables (a host capability:
`Host::variable` natively, the process's environment; a binding on
Workers), so the interpreter and compiled code on every platform start the
same way, and `jadpo config check` and the app cannot disagree about what is
valid. `jadpo dev` is the one place that makes up a value: a database
connection whose address no variable gives gets a database of its own on a
disposable PostgreSQL for the session, so a fresh clone runs.

```text
$ jadpo dev
jadpo dev, started 12:52:17
error: `GREETING` isn't set
  reason: `greeting` is required: it has no `default:`, so the app can't start without a value for it.
  next: Run `jadpo config set greeting` to store it in `.env.local`, or set `GREETING` in the environment (human)
  rule: configuration.value_missing
  help: docs/language/14-tooling.html#command-errors

not running this version: 1 error, waiting for a save that fixes them
```

## Time types

| Type | What it is | Example | On the wire |
|---|---|---|---|
| `Instant` | One point on the timeline, in UTC, to the millisecond | when a todo was created | `"2026-01-15T12:00:00Z"`, `"2026-01-15T12:00:00.250Z"` |
| `CalendarDate` | A date with no time or zone | a birthday, an invoice date | `"2026-10-25"` |
| `ZonedTime` | An `Instant` seen in a `Zone`, for showing or reasoning about local time | "09:30 in London" | `{ "instant": "2026-10-25T08:30:00Z", "zone": "Europe/London" }` |
| `Duration` | Elapsed time | a timeout, a retention period | `"PT5M"` |
| `Zone` | An IANA time zone, a built-in enum | `Zone.europe_london` | `"Europe/London"` |
| `Weekday` | A built-in enum | `Weekday.monday` | `"monday"` |

- Store `Instant`. Use `CalendarDate` only for facts that really are dates.
  There is no time of day on its own and no date-time without a zone
  among these: `ZonedTime` (renamed from `Time`, question 5) holds an
  instant and the zone it is seen in. Area 19's `LocalTime` and
  `LocalDateTime` are for what has no zone, such as a shop's opening
  time, and are never a moment until a zone is chosen.
- `Instant` keeps milliseconds: the precision JavaScript clients, SQLite and
  PostgreSQL all share. A literal with microseconds is rejected, and
  writes to PostgreSQL's microsecond column are always whole milliseconds
  (carried from area 5).
- Types never convert to each other or to `Text` silently; signatures use
  named types built on them (decided in area 2), such as
  `type DueAt = Instant {}`.
- Built in chunk 1.9 (decided in the Phase 1 review): an `Instant` literal is
  ISO 8601 with a `Z` or an offset, to the millisecond
  (`"2026-10-25T08:30:00Z"`), a `CalendarDate` literal a real date
  (`"2026-10-25"`), both checked while compiling; a `Duration` is written only
  with a unit (`15m`), and `min:` and `max:` on a `Duration` type are
  enforced; a `ZonedTime` has no literal. `Zone`, `Weekday` and `Locale` are
  built-in enums: `Zone` has a variant for each zone in the IANA database's
  `zone.tab` (2026d) plus `utc`, named in lower_snake_case
  (`Zone.america_argentina_buenos_aires`); `Locale` has one for each
  locale Unicode CLDR 48.2.3 gives modern coverage, plus the language with
  its likely region for each without one (`Locale.fr` and `Locale.fr_fr`,
  `Locale.en_gb`; 533 in all, and `Locale.en_xa`, the pseudo-locale), named in lower_snake_case from the BCP 47
  tag. Both tables live in `jadpo-std` (`src/zones.rs`, which says how each
  was made) and change only in a change of their own.
- An `Instant` holds the years 0000 to 9999, up to
  `9999-12-30T22:00:00.999Z`, the last instant whose local date is in year
  9999 in every zone, and a `CalendarDate` the same years; a literal past
  them is refused while compiling, and arithmetic that goes past them is the
  fault `time_out_of_range` (chosen in chunk 5.2).
- Two `ZonedTime`s are equal when both their instants and their zones are.
  They have no order of their own, since two in different zones have none;
  their instants do, through `temporal.instant(...)` (chosen in chunk 5.2).
- A `ZonedTime` field of an entity is stored in two columns: its instant as
  `timestamptz(3)` and its zone's IANA name as `text`.
- `clock.now` in a function, a library function or an inline function is
  `type.clock_context`; `clock.advance(...)` outside a test is
  `type.test_clock_context` (area 13); any other member of `clock`,
  `translations`, `sessions` or `oauth` is `toolchain.not_built_yet` until
  its area is built. `config.<name>` is a setting (chunk 5.1).

```jadpo check project=time-types
/// When an invoice falls due.
type DueAt = Instant {}

/// The day an invoice is dated.
type InvoiceDate = CalendarDate {}

test "time literals are checked while compiling and compare as time" {
    assert DueAt("2026-10-25T13:00:00+01:00") == DueAt("2026-10-25T12:00:00Z")
    assert InvoiceDate("2026-10-25") < InvoiceDate("2026-10-26")
}
```

## Durations

Decided (question 4): one spelling everywhere in source, `500ms`, `30s`,
`5m`, `2h`, `30d`, with ISO 8601 (`"PT5M"`) only on the wire.

```jadpo check project=durations
/// How long a request may take.
type RequestTimeout = Duration {
    max: 30s
}

/// How long a reminder comes before its due time.
type ReminderNotice = Duration {
    min: 1m
    max: 30d
}

test "durations are written with a unit, and add up" {
    var allowed: RequestTimeout = 5s
    assert allowed + 500ms == 5500ms
    var notice: ReminderNotice = 2h
    assert notice - 30m == 90m
}
```

`30d` means exactly 30 × 24 hours, which is what a retention period or a
token lifetime means. Moving a date by calendar days is a separate operation
on `CalendarDate` (below).

On the wire a duration is ISO 8601 in hours, minutes and seconds, as the
runtime writes it: `"PT5M"`, `"PT1.5S"`, `"PT720H"` for `30d`, `"-PT5M"`
for a negative one. A request's JSON, query string, path and headers give
it the same way, with days (`"P1DT2H"`) accepted as 24 hours each; `5m` is
the source's spelling and is refused there (chosen in chunk 5.2). A setting
is the exception, read as source writes it, `5m` (chunk 5.1), and also as
the contract's canonical text, `PT5M`, as `text.parse` reads it, so a
value copied from a response or a log is a setting too: every boundary
reads a standard type through its contract (area 19, "Every boundary
through the contract"; decided by the lead session, 2026-10-18, under
Richard's instruction of 2026-10-17; built in chunk 5.5w2).

## The current time: `clock.now`

`clock.now` is the time an operation started. It is fixed for the whole
operation, nested actions included, so every timestamp one request writes
agrees. A later request, schedule run or retry gets a new value. There is no
way to read a fresher time, and code never reads the system clock.

Only actions read the clock. Functions and queries must give the same answer
for the same arguments, so they take the time as a parameter
(`type.clock_context`); a query's condition may compare with `clock.now`,
which is the calling action's (area 6):

```jadpo check project=clock-now
type ReminderCutoff = Instant {}

/// The latest due time a reminder goes out for now: an hour ahead.
action reminder_cutoff() -> ReminderCutoff {
    return ReminderCutoff(clock.now + 1h)
}

test "an action reads the test's clock" {
    clock: "2026-01-15T12:00:00Z"

    assert reminder_cutoff() == ReminderCutoff("2026-01-15T13:00:00Z")
    clock.advance(30m)
    assert reminder_cutoff() == ReminderCutoff("2026-01-15T13:30:00Z")
}
```

Timeouts are measured on a separate clock that wall-clock changes cannot
move; code never sees it. Tests fix the clock with `clock:` and move it with
`clock.advance(...)` (area 13). The generated SQL always passes the time as
a value; the database's own `now()` is never used.

## Arithmetic and comparison

`Instant + Duration`, `Duration + Instant` and `Instant - Duration` give an
`Instant` of the instant's own named type, so no rebuild is needed
(question 6). `Instant - Instant` gives the `Duration` between them, which is
negative when the second is later; `Duration ± Duration` gives a
`Duration`. As with numbers, both sides are the same type after widening, or
one is a literal, and the result of two of the same is a plain `Duration`.
Instants, dates and durations order with `<`, `<=`, `>` and `>=` and compare
with `==` on the timeline, so `12:00Z` equals `13:00+01:00`.

A `CalendarDate` has no clock time, so a `Duration` doesn't move it; it moves
by calendar days with `temporal.add_days`. A `ZonedTime` moves on its
calendar the same way, or through its instant. `*`, `/` and `%` don't take
time values (chosen in chunk 5.2).

```jadpo check project=arithmetic
type DueAt = Instant {}

type ReminderNotice = Duration {
    min: 1m
    max: 7d
}

type Lateness = Duration {}

/// When to send a reminder: the notice before the due time. Taking a
/// duration from a `DueAt` gives a `DueAt`.
function reminder_time(due_at: DueAt, notice: ReminderNotice) -> DueAt {
    return due_at - notice
}

/// How late something done at `done_at` was: negative when it was early.
function lateness(due_at: DueAt, done_at: DueAt) -> Lateness {
    return Lateness(done_at - due_at)
}

test "a duration moves an instant and keeps its type" {
    assert reminder_time("2026-10-25T09:00:00Z", 30m) == "2026-10-25T08:30:00Z"
    assert lateness("2026-10-25T09:00:00Z", "2026-10-25T11:15:00Z") == 135m
    assert lateness("2026-10-25T09:00:00Z", "2026-10-25T08:00:00Z") < 0s
}

test "instants compare on the timeline" {
    assert DueAt("2026-10-25T12:00:00Z") == DueAt("2026-10-25T13:00:00+01:00")
    assert DueAt("2026-10-25T12:00:00Z") < DueAt("2026-10-25T12:00:00.001Z")
}
```

## Zones and local times

A zone turns an instant into local time; this is never ambiguous:
`temporal.in_zone(clock.now, Zone.europe_london)` is a `ZonedTime`.

Going the other way, from a date and a clock time to an instant, can be: at
01:30 on the day clocks go back, London has two 01:30s, and on the day they
go forward 01:30 does not exist. One option, `dst:`, says which instant to
take (question 7): `Dst.earlier` or `Dst.later` takes that instant of an
overlap, and in a gap moves the clock time back or forward by the gap's
length; `Dst.compatible` is earlier in an overlap and later in a gap, as
JavaScript's Temporal does. Left out, or `Dst.reject`, an unclear local time
fails, and the call's `attempt` maps it by the option's name, `dst:`, the
way a construction's broken rule is mapped (area 2): the failure is
`invalid_value`, and `invalid:` covers every condition without its own arm.

```jadpo check project=local-times
type ShopDay = CalendarDate {}

type Opening = ZonedTime {}

type OpeningUnclear = Failure {
    kind: FailureKind.invalid_value
}

/// When the night shift opens on `day`: 01:30, which on the day the clocks
/// go back happens twice, and on the day they go forward not at all.
function night_opening(day: ShopDay) fails OpeningUnclear -> Opening {
    var opening: ZonedTime = attempt temporal.resolve(day, at: "01:30", zone: Zone.europe_london) {
        dst: OpeningUnclear
    }
    return Opening(opening)
}

/// The same, taking the earlier 01:30 of the two and, when there is none,
/// the time an hour on.
function night_opening_anyway(day: ShopDay) -> Opening {
    return Opening(
        temporal.resolve(day, at: "01:30", zone: Zone.europe_london, dst: Dst.compatible)
    )
}

test "an unclear local time fails unless `dst:` decides" {
    assert night_opening("2026-10-25") fails OpeningUnclear
    assert night_opening("2026-03-29") fails OpeningUnclear
    assert temporal.instant(night_opening_anyway("2026-10-25")) == Instant("2026-10-25T00:30:00Z")
    assert temporal.instant(night_opening_anyway("2026-03-29")) == Instant("2026-03-29T01:30:00Z")
}

test "a day on keeps the clock time, 25 hours later across the change" {
    var saturday: ZonedTime = temporal.resolve(
        "2026-10-24",
        at: "09:00",
        zone: Zone.europe_london,
        dst: Dst.compatible
    )
    var sunday: ZonedTime = temporal.add_days(saturday, 1, dst: Dst.compatible)
    assert temporal.instant(sunday) - temporal.instant(saturday) == 25h
    assert temporal.hour(sunday) == 9
    assert temporal.weekday(sunday) == Weekday.sunday
}

test "a month on from the 31st needs a day the month has" {
    var end_of_january: CalendarDate = "2026-01-31"
    assert temporal.add_months(
        end_of_january,
        1,
        invalid_day: InvalidDay.last_valid_day
    ) == CalendarDate("2026-02-28")
}
```

`at:` is a clock time written out, `"09:30"` or `"09:30:15"`, checked while
compiling (`type.temporal_clock_invalid`), or a `LocalTime` (area 19), and
in place of a date and `at:` the call takes a `LocalDateTime`, which
carries its own clock time: `temporal.resolve(shift.start, zone:
shift_zone, dst: Dst.compatible)`. Only a `dst:` written out settles the condition: a `Dst`
passed as a value might be `reject`, so the call can still fail. Moving a
date by months or years can land on a day the month doesn't have (31
January plus a month): `invalid_day: InvalidDay.last_valid_day` takes the
month's last day, and left out, or `InvalidDay.reject`, the call fails with
`invalid_day:` to map. `dst:` means something only for a `ZonedTime`; on a
`CalendarDate` it is `type.temporal_option_unused` (chosen in chunk 5.2).

A query for "deliveries on 25 October in London" uses the day's bounds, from
its first instant up to the next day's, which are correct whether the day has
23, 24 or 25 hours; a query compares a field with an `InstantRange`
parameter's `start` and `end` as with any parameter's fields (area 6):

```jadpo check project=deliveries file=app.jadpo hidden
application {}
```

```jadpo check project=deliveries file=domains/shop/types/delivery_note.jadpo hidden
type DeliveryNote = Text {
    max_length: 200
}
```

```jadpo check project=deliveries file=domains/shop/types/shop_day.jadpo hidden
type ShopDay = CalendarDate {}
```

```jadpo check project=deliveries file=domains/shop/entities/delivery.jadpo hidden
use shop/types { DeliveryNote }

entity Delivery {
    id: Uuid
    note: DeliveryNote
    arrived_at: Instant {
        generated: Generated.on_create
    }
}
```

```jadpo check project=deliveries file=domains/shop/types/delivery_view.jadpo hidden
use shop/entities { Delivery }

type DeliveryView = Output {
    pick: [Delivery.note, Delivery.arrived_at]
}
```

```jadpo check project=deliveries file=domains/shop/actions/record_delivery.jadpo hidden
use shop/entities { Delivery }

/// Records a delivery as it arrives: `arrived_at` is the action's
/// `clock.now`.
action record_delivery(note: Delivery.note) -> Delivery {
    return Delivery.create {
        set: {
            note: note
        }
    }
}
```

```jadpo check project=deliveries file=domains/shop/queries/deliveries_in.jadpo
use shop/entities { Delivery }
use shop/types { DeliveryView }

/// The deliveries that arrived from `arrivals.start` up to `arrivals.end`.
query deliveries_in(arrivals: InstantRange) -> List<DeliveryView> {
    from: Delivery
    where: Delivery.arrived_at >= arrivals.start
    where: Delivery.arrived_at < arrivals.end
    order: [Delivery.arrived_at asc]
    limit: 100
}
```

```jadpo check project=deliveries file=domains/shop/actions/deliveries_on.jadpo migrate=initial
use shop/types { DeliveryView, ShopDay }
use shop/queries { deliveries_in }

/// The deliveries on `day` in London, however many hours it has.
action deliveries_on(day: ShopDay) -> List<DeliveryView> {
    return deliveries_in(temporal.day_bounds(day, Zone.europe_london))
}
```

The test runs against PostgreSQL. London's clocks go back at 02:00 on 25
October 2026, so the day runs from 23:00 UTC on the 24th to midnight UTC on
the 26th:

```jadpo check project=deliveries file=domains/shop/actions/deliveries_on.test.jadpo
use shop/actions { record_delivery }

test "a London day has 25 hours when the clocks go back" {
    clock: "2026-10-24T22:59:00Z"

    assert record_delivery("before midnight").note == "before midnight"
    clock.advance(2m)
    assert record_delivery("just after midnight").note == "just after midnight"
    clock.advance(24h)
    assert record_delivery("late on the 25th").note == "late on the 25th"
    clock.advance(1h)
    assert record_delivery("the 26th").note == "the 26th"
    assert list.map(deliveries_on("2026-10-25"), delivery -> delivery.note) == [
        "just after midnight",
        "late on the 25th"
    ]
}
```

> Amended 2026-10-18 by area 19 (chunk 5.5k4): `resolve` takes a
> `LocalDateTime`, which has its own clock time, or a date with `at:` a
> clock time written out or a `LocalTime`, each under the same `dst:`; the
> choices are in area 19's "Time and calendar additions".

The `temporal` library (question 7, Option B in the
[time and formatting proposals](research/10-configuration-and-time-proposals.md)),
always called through its namespace. "A date or local time" is a value built
on `CalendarDate` or `ZonedTime`, and a result of "its type" keeps the
argument's named type:

| Operation | Gives | Fails on, unless an option settles it |
|---|---|---|
| `in_zone(instant, zone)` | `ZonedTime` | |
| `resolve(date, at:, zone:, dst:)`, or a `LocalDateTime` with no `at:` | `ZonedTime` | `dst` |
| `between(start, end)` | `Duration`, `end - start` | |
| `add_days(value, days, dst:)` | its type, a whole number of calendar days on, the clock time kept | `dst`, for a `ZonedTime` |
| `add_months(value, months, invalid_day:, dst:)` | its type, months on, the day and clock time kept | `invalid_day`, and `dst` for a `ZonedTime` |
| `add_years(value, years, invalid_day:, dst:)` | its type, twelve months a year | the same |
| `day_bounds(date, zone)` | `InstantRange`: `start`, and `end` not included | |
| `week_bounds(date, zone, starts_on:)` | the week holding `date`, from `starts_on` (a `Weekday`) | |
| `month_bounds(date, zone)`, `year_bounds(date, zone)` | the month or year holding `date` | |
| `year(value)`, `month(value)`, `day(value)` | `Int`, of a date or a local time's date; `month` and `day` from 1 | |
| `weekday(value)` | `Weekday` | |
| `date(zoned)`, `hour(zoned)`, `minute(zoned)` | a local time's `CalendarDate`, and `Int`s from 0 | |
| `zone(zoned)`, `instant(zoned)` | the `Zone` it is seen in, and the `Instant` it is | |
| `format(value, zone:, locale:, format:)`, `format_relative(value, now:, zone:, locale:)` | `PresentationText`, below | |

A day's bounds start at its first instant: midnight, or, in a zone whose
clocks skip midnight, the first time they show that day. `instant(...)` is
the one operation the proposal didn't list: a `ZonedTime` has no order of
its own, and its instant does (chosen in chunk 5.2).

Zones come from the IANA time zone database bundled into every build
(jiff's `jiff-tzdb`, release 2026e), never the platform's own files, so the
interpreter, a native deployable and WebAssembly on Workers answer alike,
and every `Zone` variant is in it, which a test holds. A newer release is a
dependency change of its own (chosen in chunk 5.2).

## Showing dates and times to people

> Built in chunk 5.2b: the formats, `PresentationText` and the checks on
> both calls. Built in chunk 5.3: the text they give, in the words, order,
> punctuation and digits of each locale, and the locales an application
> supports.

> Amended 2026-10-17 by area 19 (question 2): every standard type is
> formatted for people by one call, `text.format(value, locale:)`, so
> `temporal.format` became the time types' `text.format`, with the same
> `zone:` and `format:` options and a habit error,
> `semantic.standard_operation_moved`, whose edit writes `text.format`;
> `temporal.format_relative` stays. Built in chunk 5.5a, and the examples
> here use `text.format`. Area 19 also adds
> three `temporal` operations (`occurrences`, `open_at`,
> `period_between`, chunks 5.5k and 5.5k2) and `{price, number, currency}` to the
> placeholder styles, built in chunk 5.5h2 ("Messages", below), and
> `{rate, number, percent}`, built in chunk 5.5j2.

Formatting needs three things the server must never guess: the zone, the
language and the format. Decided (question 7): one call that takes all
three, with the format either built in or declared once as a named type.
The language is one the application supports, declared in `app.jadpo`
("Locales", below), so the example is a project:

```jadpo check project=format-call file=app.jadpo
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=format-call file=domains/shop/types/ordered_at.jadpo
/// When an order was placed.
type OrderedAt = Instant {}
```

```jadpo check project=format-call file=domains/shop/types/short_date.jadpo
/// A date as "25 Oct 2026" in `Locale.en_gb`.
type ShortDate = TimeFormat {
    day: ComponentStyle.numeric
    month: ComponentStyle.short
    year: ComponentStyle.numeric
}
```

```jadpo check project=format-call file=domains/shop/types/order_placed.jadpo
/// When an order was placed, shown to its customer.
type OrderPlaced = Output {
    placed: PresentationText
}
```

```jadpo check project=format-call file=domains/shop/actions/show_placed.jadpo
use shop/types { OrderPlaced, OrderedAt, ShortDate }

/// The order's placing, in the customer's zone and language.
action show_placed(ordered_at: OrderedAt, zone: Zone, reader_locale: Locale) -> OrderPlaced {
    return OrderPlaced {
        placed: text.format(ordered_at, zone: zone, locale: reader_locale, format: ShortDate)
    }
}
```

The zone decides the date, and the locale the words and their order: at
23:30 UTC on 25 October 2026 it is already the 26th in Paris.

```jadpo check project=format-call file=domains/shop/actions/show_placed.test.jadpo
test "the placing is shown in the customer's zone and language" {
    assert show_placed(
        "2026-10-25T09:30:00Z",
        Zone.europe_london,
        Locale.en_gb
    ).placed == "25 Oct 2026"
    assert show_placed(
        "2026-10-25T09:30:00Z",
        Zone.europe_paris,
        Locale.fr_fr
    ).placed == "25 oct. 2026"
    assert show_placed(
        "2026-10-25T23:30:00Z",
        Zone.europe_paris,
        Locale.fr_fr
    ).placed == "26 oct. 2026"
}
```

The words come from Unicode CLDR 48 through ICU4X (the `icu_datetime`,
`icu_calendar`, `icu_time`, `icu_locale` and `icu_experimental` crates,
pinned together), compiled into the binary, so the interpreter, a native
deployable and WebAssembly on Workers write the same text and nothing is
read from the platform; `Locale` was generated from the same release, so
every variant has data. The calendar is always the Gregorian one, as
`calendar: "gregory"` gives in `Intl.DateTimeFormat` (a Thai or Arabic
reader of a `CalendarDate` sees the same year the database holds), and the
digits are always the ones the locale writes (`ar_eg`, `fa`, `bn`, `mr` and
`ne` write their own). CLDR's narrow no-break space before `PM` is written
as a plain space, as browsers and Node write it, so a test is written with
the space it shows.

A built-in format is written `format: TimeFormat.date_time_long`. The
built-in formats are `date_`, `time_` and `date_time_` each with `full`,
`long`, `medium` or `short`, CLDR's four lengths: the locale's own pattern
for a date, a time, or both with the words it joins them with. In `en_gb`,
for 14:30:05 on Sunday 27 September 2026 in London, where the clocks are on
British Summer Time:

| Format | `en_gb` |
|---|---|
| `date_full` | Sunday, 27 September 2026 |
| `date_long` | 27 September 2026 |
| `date_medium` | 27 Sept 2026 |
| `date_short` | 27/09/2026 |
| `time_full` | 14:30:05 British Summer Time |
| `time_long` | 14:30:05 BST |
| `time_medium` | 14:30:05 |
| `time_short` | 14:30 |
| `date_time_full` | Sunday, 27 September 2026 at 14:30:05 British Summer Time |
| `date_time_long` | 27 September 2026 at 14:30:05 BST |
| `date_time_medium` | 27 Sept 2026, 14:30:05 |
| `date_time_short` | 27/09/2026, 14:30 |

`date_full` is the long date with the weekday, and `date_time_long` has the
seconds `timeStyle: "long"` has. For a handful of locales (Japanese and
Korean `time_` formats, the punctuation of a few `date_` formats) the
pattern is ICU4X's, which is not always the one `Intl.DateTimeFormat`
writes. Measured against Node 24 (ICU 78, CLDR 48) on all 533 locales, two
zones and four instants, 87% of 145,000 formatted texts, built-in and
declared, are identical; by built-in format the same text is 92% for
`date_long` and 94% for `time_medium`, down to 68% for `date_time_full`,
the rest differing in punctuation, joining words, number widths or the
24-hour padding. A zone's name comes from the zone and the instant: "BST"
or "British Summer Time" in summer, "GMT" or "Greenwich Mean Time" in
winter, and where CLDR has no name for the locale, its offset ("GMT+1",
"UTC+1").

This page checks the table; the project that does is hidden.

```jadpo check project=format-builtins file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

```jadpo check project=format-builtins file=tests/built_in_formats.test.jadpo hidden
test "the twelve built-in formats in en_gb" {
    var seen: ZonedTime = temporal.in_zone("2026-09-27T13:30:05Z", Zone.europe_london)
    assert text.format(
        seen,
        locale: Locale.en_gb,
        format: TimeFormat.date_full
    ) == "Sunday, 27 September 2026"
    assert text.format(
        seen,
        locale: Locale.en_gb,
        format: TimeFormat.date_long
    ) == "27 September 2026"
    assert text.format(seen, locale: Locale.en_gb, format: TimeFormat.date_medium) == "27 Sept 2026"
    assert text.format(seen, locale: Locale.en_gb, format: TimeFormat.date_short) == "27/09/2026"
    assert text.format(
        seen,
        locale: Locale.en_gb,
        format: TimeFormat.time_full
    ) == "14:30:05 British Summer Time"
    assert text.format(seen, locale: Locale.en_gb, format: TimeFormat.time_long) == "14:30:05 BST"
    assert text.format(seen, locale: Locale.en_gb, format: TimeFormat.time_medium) == "14:30:05"
    assert text.format(seen, locale: Locale.en_gb, format: TimeFormat.time_short) == "14:30"
    assert text.format(
        seen,
        locale: Locale.en_gb,
        format: TimeFormat.date_time_full
    ) == "Sunday, 27 September 2026 at 14:30:05 British Summer Time"
    assert text.format(
        seen,
        locale: Locale.en_gb,
        format: TimeFormat.date_time_long
    ) == "27 September 2026 at 14:30:05 BST"
    assert text.format(
        seen,
        locale: Locale.en_gb,
        format: TimeFormat.date_time_medium
    ) == "27 Sept 2026, 14:30:05"
    assert text.format(
        seen,
        locale: Locale.en_gb,
        format: TimeFormat.date_time_short
    ) == "27/09/2026, 14:30"
}
```

A declared format lists the parts it shows, each once, with a
`ComponentStyle`, and the locale decides their order and punctuation and
whether the clock has 12 or 24 hours, as CLDR's skeletons and JavaScript's
`Intl.DateTimeFormat` do:

| Part | Takes |
|---|---|
| `weekday` | `long`, `short`, `narrow` |
| `day`, `year` | `numeric`, `two_digit` |
| `month` | `numeric`, `two_digit`, `long`, `short`, `narrow` |
| `hour`, `minute`, `second` | `numeric`, `two_digit` |
| `zone_name` | `long` ("British Summer Time"), `short` ("BST") |

A format with no parts is `type.temporal_format_components_empty`, a part
twice `type.temporal_format_component_duplicate`, a style the part doesn't
take `type.temporal_format_component_invalid`, and a name that is no part
`type.unknown_setting`. A format holds no value: `format:` names it
written out, and it is never the type of a field, a parameter or a
variable, nor a value passed in (`type.temporal_format_invalid`). The
operation is given it as text, `day:numeric month:short year:numeric` or
`date_time_long`.

A declared format takes the locale's pattern for its parts, then sets each
part to its style. A month written as a name is taken from the long date
pattern (the medium one where that writes the month as a name too), a
number from the short one, and `month: short` is that pattern with the
abbreviated name; `numeric` leaves a number as wide as the locale writes it
(`dd/MM/y` stays) and `two_digit` makes it at least two; a date and a time
are joined with the words the locale joins them with for that month ("at"
for `long`, a comma for `short`). So the output is the locale's own and
matches `Intl.DateTimeFormat` for most locales and most options, but not
all of them: where `Intl` shortens `01:59` to `1:59` for `hour: numeric
minute: two_digit`, this writes the locale's `01:59`, and a Japanese or
Korean weekday beside a long date has no brackets. Parts no pattern holds
together (a year with a day and no month, a weekday and a year, a minute
without an hour, a month with a time) are written one after another with a
space: `27 2026`, `30:05`, `September, 14:30`. Only `zone_name` alone is
the name alone ("British Summer Time"), where `Intl` adds the date.

`zone:` is given exactly when the value is an `Instant`
(`type.temporal_zone_required`): a `ZonedTime` is shown in its own zone and
a `CalendarDate` has none, so `zone:` with either is
`type.temporal_option_unused`. A `CalendarDate` has no clock time, so a
`time_` or `date_time_` format, or one showing `hour`, `minute`, `second`
or `zone_name`, is `type.temporal_format_requires_time`. `locale:` is a
`Locale` the application supports (`type.temporal_locales_required`,
`type.locale_unsupported`; "Locales", below).

"3 minutes ago" style text uses
`temporal.format_relative(value, now: clock.now, zone:, locale:)`, for an
`Instant` or a `ZonedTime`, with `zone:` as for `format`. Its rules are
fixed, so two readers of the same value see the same words:

| `value` against `now` | Text, in `en_gb` |
|---|---|
| under a minute, either way | "just now" |
| under an hour | "4 minutes ago", "in 10 minutes" |
| the same date | "Today at 13:00" |
| the date before, or after | "Yesterday at 14:30", "Tomorrow at 14:30" |
| two to six days away | "Friday at 07:30" |
| anything else | "20 September 2026" |

```jadpo check project=format-relative file=app.jadpo
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=format-relative file=domains/chat/types/sent_at.jadpo
/// When a message was sent.
type SentAt = Instant {}
```

```jadpo check project=format-relative file=domains/chat/types/sent_text.jadpo
/// When a message was sent, as its reader reads it.
type SentText = Output {
    seen: PresentationText
}
```

```jadpo check project=format-relative file=domains/chat/actions/show_sent.jadpo
use chat/types { SentAt, SentText }

/// When a message was sent, against now in London.
action show_sent(sent_at: SentAt, reader_locale: Locale) -> SentText {
    return SentText {
        seen: temporal.format_relative(
            sent_at,
            now: clock.now,
            zone: Zone.europe_london,
            locale: reader_locale
        )
    }
}
```

```jadpo check project=format-relative file=domains/chat/actions/show_sent.test.jadpo
test "a time is shown against now" {
    clock: "2026-09-27T13:30:00Z"

    assert show_sent("2026-09-27T13:29:40Z", Locale.en_gb).seen == "just now"
    assert show_sent("2026-09-27T13:26:00Z", Locale.en_gb).seen == "4 minutes ago"
    assert show_sent("2026-09-27T12:00:00Z", Locale.en_gb).seen == "Today at 13:00"
    assert show_sent("2026-09-26T13:30:00Z", Locale.en_gb).seen == "Yesterday at 14:30"
    assert show_sent("2026-09-20T13:30:00Z", Locale.en_gb).seen == "20 September 2026"
    assert show_sent("2026-09-27T13:26:00Z", Locale.fr_fr).seen == "il y a 4 minutes"
    assert show_sent("2026-09-26T13:30:00Z", Locale.fr_fr).seen == "Hier à 14:30"
}
```

The words are the locale's own for "now", "4 minutes ago", "yesterday" and
"tomorrow"; "just now" is English's, and every other locale says what CLDR
says for "now". Under an hour counts whole minutes, a past value down and a
future one up, so a value is never said to be further away than it is.
Dates are the dates on the clock in the zone the value is shown in, not
blocks of 24 hours, so a day with 25 hours is still one day. The first
letter of "Today at", "Yesterday at", "Tomorrow at" and a weekday is a
capital where the script has them. A locale with no word for yesterday or
tomorrow shows the long date. "Two to six days away" is a weekday and the
time, in the past or the future alike; a week or more is the long date,
with no time.

The result is `PresentationText`, a built-in type: it can go into a
response, an email or a page, a signature may use it directly, and text
written in quotes may stand for one, which is how a test compares formatted
text with what it expects. It cannot be stored: an entity field holding it,
directly or through a named type, a list, an object or a variant's data, is
`type.presentation_text_persistence`, and it is not a setting. It cannot be
turned back into a time, ordered, added to or rebuilt into `Text`; two
compare with `==` and `!=`. On the wire it is a JSON string. Clients that
format for themselves receive the `Instant`.

## Locales

`Locale` is a built-in enum of the standard locales (`Locale.en_gb`,
`Locale.fr_fr`): every locale Unicode CLDR 48 covers fully, and for each
language without a region the region it most likely means. An application
lists the ones it supports, once, in `app.jadpo`; formatting and
translations accept only those, and using any other is a compile error. The
server's own locale is never used.

```jadpo check project=locales file=app.jadpo
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.en_us, Locale.fr_fr]
}
```

`locales { }` is a declaration with no name, like `config { }`, with
exactly two options, each once: `default:` and `supported:`, a list. Each
entry is written `Locale.<tag>`, the tag in lower case with `_` for `-`
(`Locale.zh_hans` is zh-Hans). A locale written out where text is formatted,
or as the `default:` or `test:` of a `Locale` setting, that the list doesn't
have is `type.locale_unsupported`; a project that formats text and declares
none is `type.temporal_locales_required`.

There is no `unsupported:` setting. A request field typed `Locale` is
decoded like any enum, so an unsupported tag is an `invalid_value` failure
and the OpenAPI document lists only the supported tags; a setting typed
`Locale` is read the same way when the app starts. Choosing a language from
an `Accept-Language` header is a library call that falls back to `default`
("Choosing a locale from a request", below).

Decided on 2026-10-16 (Richard) and built in chunk 5.4c, with translations
as messages: `in_progress:` is the third option, optional and once, a list
of the supported locales whose translations are not finished yet. A locale
listed there but not in `supported:` is `type.locale_in_progress_unsupported`,
and the default can't be listed (`type.locale_in_progress_default`). Every
other supported locale must be complete; only one listed here falls back
where wording is missing, with no diagnostic, because listing it is the
explicit, declared opt-out (there are no warnings, area 1, question 11); see
Translations as messages.

```jadpo check project=locales-in-progress file=app.jadpo
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.en_us, Locale.fr_fr, Locale.pl_pl]
    in_progress: [Locale.pl_pl]
}
```

Text for people is only ever written in a supported locale. A `Locale`
the list doesn't have can still reach `text.format`,
`temporal.format_relative` or a message as the app runs: one saved with a
record before `supported:` changed, or written out anywhere but `locale:`.
It is written in the supported locale a browser asking for it would get
("Choosing a locale from a request", below): a supported locale of its
language, the one CLDR's likely subtags give for the language first, else
`default:` (chosen 2026-10-21 in chunk 8.2c). A list `list.sort_by`
orders in such a locale is ordered by that same supported locale's
alphabet (Richard, 2026-10-22; built in chunk 8.4i), so `jadpo test` and
a deployable, which carries only its supported locales' collation, give
the same order. Sorting by a locale needs `locales { }`, as formatting
does (`type.temporal_locales_required`; Richard, 2026-10-22), so a
deployable never carries collation for a locale the app doesn't support.
Case mapping is not text for people in a language, and takes any
`Locale` (area 19, "Text").

```jadpo check project=locales-unsupported file=app.jadpo
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=locales-unsupported file=domains/shop/types/ordered_at.jadpo
/// When an order was placed.
type OrderedAt = Instant {}
```

```jadpo check project=locales-unsupported file=domains/shop/types/placed_text.jadpo
/// When an order was placed, shown to its customer.
type PlacedText = Output {
    placed: PresentationText
}
```

```jadpo check project=locales-unsupported file=domains/shop/actions/show_placed.jadpo
use shop/types { OrderedAt, PlacedText }

/// The placing, in the locale saved with the customer's order.
action show_placed(ordered_at: OrderedAt, saved_locale: Locale) -> PlacedText {
    return PlacedText {
        placed: text.format(
            ordered_at,
            zone: Zone.europe_london,
            locale: saved_locale,
            format: TimeFormat.date_long
        )
    }
}
```

```jadpo check project=locales-unsupported file=domains/shop/actions/show_placed.test.jadpo
test "a locale the application doesn't support is written in one it does" {
    assert show_placed("2026-09-27T13:30:05Z", Locale.fr_fr).placed == "27 septembre 2026"
    assert show_placed("2026-09-27T13:30:05Z", Locale.fr_ca).placed == "27 septembre 2026"
    assert show_placed("2026-09-27T13:30:05Z", Locale.de_de).placed == "27 September 2026"
}
```

So a deployable carries the locale data of its supported locales only
(chunk 8.2c): `jadpo build` takes, from the CLDR data compiled into
Jadpo, the entries the supported locales reach as ICU4X looks data up
(the locale, then each locale its lookups fall back to, down to the root),
collation among them, and the
data that is the same in every locale,
and compiles the deployable with that; `jadpo test` and `jadpo dev` keep
every locale, and write the same text. A WebAssembly module that runs
every standard-library operation is 18.5 MB with every locale and 8.5 MB
with `en_gb` and `fr_fr` (6.1 MB and 2.7 MB with gzip), 0.9 MB of which
was every locale's collation until chunk 8.4i kept the supported
locales' only.

Every locale knows its text direction (decided 2026-10-16, built in chunk
5.4d): `locale.direction(value)` gives a value of the built-in enum
`TextDirection`, `.left_to_right` or `.right_to_left`, as CLDR's likely
script for the locale says (`Locale.ar_eg` and `Locale.he_il` read right to
left, `Locale.en_gb` left to right). Jadpo ships its wording as the HTML
values, `ltr` and `rtl`, so the email layout's `<html dir="{{
locale.direction }}">` writes one of them (built in chunk 7.3).

```jadpo check project=locales-direction file=app.jadpo
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.ar_eg]
}
```

```jadpo check project=locales-direction file=tests/direction.test.jadpo
test "a locale knows which way its text runs" {
    assert locale.direction(Locale.ar_eg) == TextDirection.right_to_left
    assert locale.direction(Locale.en_gb) == TextDirection.left_to_right
}
```

### Choosing a locale from a request

> Built in chunk 5.3c, named by Richard on 2026-10-16. The examples marked
> `check` run with this checkout's `jadpo`.

`locale.from_accept_language(header)` gives the `Locale` a browser asks for
in its `Accept-Language` header, out of the locales the application
supports, and `default:` when the header names none of them. `header` is
`Text?`: a field read with `header: "Accept-Language"` and `optional: true`
(area 7) is passed straight in, whether the request sent the header or not.
The call never fails, and it is the only way a request chooses a locale
that is not written in its input.

```jadpo fragment
type PlaceRequest = Input {
    ordered_at: OrderedAt
    languages_header: BrowserLanguages {
        header: "Accept-Language"
        optional: true
    }
}
```

```jadpo check project=accept-language file=app.jadpo
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.en_us, Locale.fr_fr]
}
```

```jadpo check project=accept-language file=domains/shop/types/browser_languages.jadpo
/// What a browser says its reader's languages are.
type BrowserLanguages = Text {
    max_length: 2000
}
```

```jadpo check project=accept-language file=domains/shop/types/ordered_at.jadpo
/// When an order was placed.
type OrderedAt = Instant {}
```

```jadpo check project=accept-language file=domains/shop/types/placed_text.jadpo
/// An order's placing, shown to its customer.
type PlacedText = Output {
    reply_locale: Locale
    placed: PresentationText
}
```

```jadpo check project=accept-language file=domains/shop/actions/show_placed.jadpo
use shop/types { BrowserLanguages, OrderedAt, PlacedText }

/// The placing, in the language the customer's browser asks for.
action show_placed(ordered_at: OrderedAt, languages_header: BrowserLanguages?) -> PlacedText {
    var reader_pick: Locale = locale.from_accept_language(languages_header)
    return PlacedText {
        reply_locale: reader_pick
        placed: text.format(
            ordered_at,
            zone: Zone.europe_london,
            locale: reader_pick,
            format: TimeFormat.date_long
        )
    }
}
```

```jadpo check project=accept-language file=domains/shop/actions/show_placed.test.jadpo
test "the browser's languages choose the locale" {
    assert show_placed("2026-09-27T13:30:05Z", "fr-FR").reply_locale == Locale.fr_fr
    assert show_placed("2026-09-27T13:30:05Z", "fr-CA").reply_locale == Locale.fr_fr
    assert show_placed("2026-09-27T13:30:05Z", "en").reply_locale == Locale.en_us
    assert show_placed("2026-09-27T13:30:05Z", "en-GB;q=0.3, fr;q=0.9").reply_locale == Locale.fr_fr
    assert show_placed(
        "2026-09-27T13:30:05Z",
        "fr-FR;q=0, en-US;q=0.1"
    ).reply_locale == Locale.en_us
    assert show_placed("2026-09-27T13:30:05Z", "de-DE, ja;q=0.8").reply_locale == Locale.en_gb
    assert show_placed("2026-09-27T13:30:05Z", "fr-FR;q=nonsense").reply_locale == Locale.en_gb
    assert show_placed("2026-09-27T13:30:05Z", none).reply_locale == Locale.en_gb
    assert show_placed("2026-09-27T13:30:05Z", "fr-FR").placed == "27 septembre 2026"
}
```

The rules are fixed, so every target gives the same locale for the same
header and the same `locales { }`:

- **Reading the header.** As RFC 9110 reads `Accept-Language`: a list split
  at commas, each entry a language range (`fr-CA`, `en`) with an optional
  weight, `;q=` and a number from 0 to 1 with up to three decimals. Spaces
  and tabs around entries and the `;` are ignored, an empty entry is
  nothing, and the range is read in any case, `_` as `-` (a client that
  writes `en_GB` means it). An entry that does not parse (a weight like
  `1.5` or `abc`, a range with an empty part or a part over eight
  characters, anything after the weight) is skipped and the others are
  still read. A weight of `0` means "not acceptable", so that tag is
  skipped too, and `*`, which names no language, is ignored.
- **Taking the tags.** By weight, the heaviest first; tags of equal weight
  keep the order the header gives them (an entry with no weight has `1`).
  The first tag that matches a supported locale decides, so a lighter tag
  that matches is chosen over a heavier one that does not.
- **Matching a tag.** Against `supported:` only, never against every
  `Locale`. A supported locale with exactly the tag wins, whatever the case
  (`fr-FR` is `Locale.fr_fr`). Otherwise a supported locale of the same
  language answers, so `fr-CA` gets `fr_fr` when French is supported only
  as `fr_fr`. Where several are of that language, the one CLDR's likely
  subtags give for the language alone is chosen (`en` means `en-US`, `pt`
  means `pt-BR`), and if none of them is that one, the first in the order
  `supported:` lists them. Only the language is compared, never the
  script, so list a locale a header should reach exactly (`zh-TW` is
  `Locale.zh_tw`); otherwise it is answered by another locale of its
  language.
- **Falling back.** With no tag matching, an empty header or `none`, the
  result is `default:`. It is never the server's own locale, and never a
  locale that is not supported.
- **A pure function.** The same header and `locales { }` give the same
  locale in the interpreter, a native deployable and WebAssembly. It reads
  no clock, store or request, so it may be called in any function, action
  or test. A query computes nothing, so it cannot call it
  (`query.value_unsupported`): choose the locale in the action and pass it
  in.
- **The call is checked as a standard-library call is.** `header` is
  always given, by position or as `header:`, and takes a `Text`, a type
  built on `Text`, `none`, or any of these that may be `none`
  (`type.argument_count`, `type.named_argument_unknown`, `type.mismatch`).
  A project with no `locales { }` has nothing to choose among, which is
  `type.temporal_locales_required`, as it is for formatting.
- **The name `locale` is reserved.** It is a standard-library namespace
  now, so a parameter or variable cannot be called `locale`
  (`semantic.standard_namespace_reserved`); the examples on this page
  call theirs `reader_locale`. A field or a named argument called `locale`
  (`locale:`) is a different thing and stays.

## Translations as messages

> Decided on 2026-10-16 (Richard), replacing question 9's JSON files and ICU
> MessageFormat. Built in chunk 5.4c: messages, the language files, their checks and errors, the typed
> calls, enum wording, email subjects and `in_progress:` locales. Built in
> chunk 5.4d: failures' client text and its export, stale translations and
> review, `TextDirection`, the `jadpo translations` commands that read and
> record the project's own wording (`missing`, `unused`, `update`,
> `accept`), go to definition and the pseudo-locale. The examples marked
> `check` run with this checkout's `jadpo`. Built in chunk 5.4e: Jadpo's own
> messages and the root file's owner groups, and `jadpo translations
> builtin`. Built in chunk 5.4f: `jadpo translations export` and `import`,
> the exchange with translation services. Built in chunk 7.1b3: library
> packages' translations, called through the package, and the root file's
> package groups. Built in chunk 7.1b4: packages' text in `jadpo
> translations export`, `import` and `unused`. Built in chunk 7.3: email
> templates that read translations, and email subjects required (area 16).

Text the server writes for people (emails, generated pages, a failure's text
for client apps) is translated in Jadpo itself. Each message is a type, each
language is a file of wordings, and both live in the domain that uses them.

### Where translations live

In the domain that uses them, in `domains/<domain>/translations/`, as its
emails live in `emails/`. A message a second domain uses moves to
`shared/translations/` under area 1's promotion rule (moved by hand until
`layout.promotion_required` learns messages: another domain's call of a
message is `visibility.private_cross_domain` meanwhile). The root
`translations/` holds only the app's wording for text it did not write:
Jadpo's own (below, built in chunk 5.4e) and its library packages' (built
in chunk 7.1b3).
A `.json` file there is ignored, like any other file that is not source.

```text
domains/shipping/translations/
    order_shipped.jadpo       type OrderShipped = Message { ... }
    parcels.jadpo             type Parcels = Message { ... }
    en_gb.jadpo               translations { ... }: the English wording
    de_de.jadpo               translations { ... }: the German wording
    sources.jadpo_record      generated: what each translation was made from
shared/translations/          messages several domains use
translations/                 the app's wording for Jadpo's and packages' text only
    fr_fr.jadpo               translations { jadpo: { ... } }: grouped by whose text it is
    sources.jadpo_record      generated: what each of those was made from
```

### Messages

A message is a type built on `Message`, a type base beside `Failure`,
`Event` and `EmailMessage`, one per file in the domain's `translations/`
folder and named after it (area 1). Its fields are the wording's
placeholders, and its `///` description reaches translators as context,
in the export (see Tooling). `Message` is a type base, so its name can't
name a type, and message names are unique across the project, as a
failure's or an email's are.

```jadpo check project=translation-messages file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.de_de]
}
```

```jadpo check project=translation-messages file=domains/shipping/types/formality.jadpo hidden
type Formality = Enum {
    formal
    casual
}
```

```jadpo check project=translation-messages file=domains/shipping/translations/order_shipped.jadpo
use shipping/types { Formality }

/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    formality: Formality
    name: Text
}
```

```jadpo check project=translation-messages file=domains/shipping/translations/parcels.jadpo
/// How many parcels an order has, on the tracking page.
type Parcels = Message {
    count: PluralCount
}
```

```jadpo check project=translation-messages file=domains/shipping/translations/delivery_window.jadpo
/// When a delivery comes, on the tracking page.
type DeliveryWindow = Message {
    day: PresentationText
    hours: PresentationText
}
```

`PluralCount` is a built-in whole number (area 2) that picks a plural form:
a value built on `Int`, used directly as a `Message` field, and written as
`{count}` in each form's wording. It has no use outside a message.

### Languages

Each language is one unnamed `translations { }` declaration in Jadpo
syntax, in a file named after its locale (`de_de.jadpo`), found by its name
as `config { }` is; code never refers to it by name, and `translations` is
a reserved word beside `config` and `locales`. A file words the messages,
the emails and the enums of its folder's own scope, a domain's or
`shared/`'s; an enum is worded in the scope that declares it. A member is a
message's, an enum's or an email's name in lower_snake_case, found by
area 1's name conversion (`order_shipped` is `OrderShipped`), as the
message's file name and its call are; a name two kinds share in one scope
keeps the first (naming rules report the clash). Every wording
is a plain string whose only syntax is `{field}`; a literal brace is
doubled, `{{` writing `{` and `}}` writing `}`. There is no grouping inside a
file (the domain is the group, and nesting means variants and plural forms
only, except the root file's groups by owner, below), and no message inside
another: a reused value such as a product name is a placeholder, because
languages that inflect cannot reuse a fragment.

```jadpo check project=translation-messages file=domains/shipping/translations/de_de.jadpo
translations {
    order_shipped: {
        formal: "Guten Tag {name}, Ihre Bestellung wurde versandt."
        casual: "Hallo {name}, deine Bestellung ist unterwegs!"
    }
    parcels: {
        one: "{count} Paket"
        other: "{count} Pakete"
    }
    delivery_window: "Lieferung am {day}, zwischen {hours}."
}
```

```jadpo check project=translation-messages file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: {
        formal: "Good day {name}, your order has shipped."
        casual: "Hi {name}, your order is on its way!"
    }
    parcels: {
        one: "{count} parcel"
        other: "{count} parcels"
    }
    delivery_window: "Delivery on {day}, between {hours}."
}
```

How a message's type maps to its wording in each language's file:

| In the type | In each language's file |
|---|---|
| The type's name, `OrderShipped` | The member `order_shipped`, in the language files of the type's own `translations/` folder |
| No fields | A plain string |
| A `Text`, `PresentationText`, `Int` or `Decimal` field, or a named type built on `Text`, `Int` or `Decimal` | A `{field}` placeholder; numbers are written the locale's way |
| A field built on `Money` | A `{field, number, currency}` placeholder, written as `text.format` shows the amount in the locale (area 19); `{field}` alone is `translation.placeholder_style` |
| A field built on `Percentage` | A `{field, number, percent}` placeholder, written as `text.format` shows the percentage in the locale (area 19, chunk 5.5j2); `{field}` alone is `translation.placeholder_style` |
| One enum field | One member per value of the enum; the field is not a placeholder |
| One `PluralCount` field | One member per plural form the language needs, from Unicode CLDR (English `one` and `other`; Polish `one`, `few`, `many` and `other`); `{count}` is available in each |
| Any other field type (`Instant`, an object, `T?`) | `translation.field_type`: format it in code and pass the text |
| More than one enum or `PluralCount` field | `translation.too_many_selectors`: split the message, so a translator never fills a grid of combinations |

The plural forms are CLDR's cardinal forms of the locale and no others
(`jadpo-std` holds the table, checked against ICU4X's data on every build).
There is no exact `=0` form: a language that says "no items" differently
words it as its own `zero` form where CLDR has one.

Dates and times are formatted in code (`text.format`, area 19) and
passed in as `PresentationText`. An amount of money is a placeholder of
its own, through the same formatter as `text.format`, so the translator
sees a price and each language writes it its own way (area 19, question
16, built in chunk 5.5h2):

```jadpo check project=translation-money file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.es_mx]
}
```

```jadpo check project=translation-money file=domains/orders/types/order_amount.jadpo hidden
/// What an order costs, in the customer's currency.
type OrderAmount = Money {}
```

```jadpo check project=translation-money file=domains/orders/translations/order_total.jadpo
use orders/types { OrderAmount }

/// What an order comes to, on its confirmation page.
type OrderTotal = Message {
    total: OrderAmount
}
```

```jadpo check project=translation-money file=domains/orders/translations/en_gb.jadpo
translations {
    order_total: "Your order comes to {total, number, currency}."
}
```

```jadpo check project=translation-money file=domains/orders/translations/es_mx.jadpo
translations {
    order_total: "Tu pedido suma {total, number, currency}."
}
```

```jadpo check project=translation-money file=domains/orders/actions/confirm_total.jadpo hidden
use orders/types { OrderAmount }

/// The order's total, in the customer's language.
action confirm_total(total: OrderAmount, reader_locale: Locale) -> PresentationText {
    return translations.orders.order_total(total: total, locale: reader_locale)
}
```

```jadpo check project=translation-money file=domains/orders/actions/confirm_total.test.jadpo
use orders/types { OrderAmount }

test "a price is written each language's way" {
    var total: OrderAmount = OrderAmount(money.from_minor(123450, Currency.mxn))
    assert confirm_total(total, Locale.en_gb) == "Your order comes to MX$1,234.50."
    assert confirm_total(total, Locale.es_mx) == "Tu pedido suma $1,234.50."
}
```

A percentage is a placeholder of its own in the same way, written
`{rate, number, percent}` (area 19, built in chunk 5.5j2):

```jadpo check project=translation-percent file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.es_mx]
}
```

```jadpo check project=translation-percent file=domains/orders/types/discount_rate.jadpo hidden
/// A discount off an order.
type DiscountRate = Percentage {}
```

```jadpo check project=translation-percent file=domains/orders/translations/order_discount.jadpo
use orders/types { DiscountRate }

/// The discount on an order, on its confirmation page.
type OrderDiscount = Message {
    reduction: DiscountRate
}
```

```jadpo check project=translation-percent file=domains/orders/translations/en_gb.jadpo
translations {
    order_discount: "You saved {reduction, number, percent}."
}
```

```jadpo check project=translation-percent file=domains/orders/translations/es_mx.jadpo
translations {
    order_discount: "Ahorraste {reduction, number, percent}."
}
```

```jadpo check project=translation-percent file=domains/orders/actions/confirm_discount.jadpo hidden
use orders/types { DiscountRate }

/// The order's discount, in the customer's language.
action confirm_discount(reduction: DiscountRate, reader_locale: Locale) -> PresentationText {
    return translations.orders.order_discount(reduction: reduction, locale: reader_locale)
}
```

```jadpo check project=translation-percent file=domains/orders/actions/confirm_discount.test.jadpo
test "a discount is written each language's way" {
    assert confirm_discount(12.5%, Locale.en_gb) == "You saved 12.5%."
    assert confirm_discount(12.5%, Locale.es_mx) == "Ahorraste 12.5%."
}
```

### Using a message

The compiler generates one call per message,
`translations.<domain>.<message>(...)`, taking every field by name plus
`locale:`, one of the app's supported locales. A value given without its
name is a field's, in the order the type declares them; the locale is
always named. A field that picks the wording takes its enum, a
`PluralCount` any value built on `Int`, a text field a value built on
`Text` or `PresentationText`, and a number field a value built on `Int` or
`Decimal`. It returns `PresentationText`, so the result can be shown but
never stored. A message is the domain's own: another domain may call it
only when it is `public`, so a message two domains use lives in
`shared/translations/` and is called `translations.shared.<message>(...)`.
In an email template the same call takes the email's locale and is written
without `locale:`: `{{ translations.shipping.parcels(count: parcel_count) }}`
(area 16).

```jadpo check project=translation-messages file=domains/shipping/types/customer_name.jadpo hidden
/// A customer's first name.
type CustomerName = Text {
    max_length: 80
}
```

```jadpo check project=translation-messages file=domains/shipping/types/parcel_count.jadpo hidden
/// How many parcels.
type ParcelCount = Int {
    min: 0
}
```

```jadpo check project=translation-messages file=domains/shipping/types/shipping_text.jadpo hidden
/// What a customer is told.
type ShippingText = Output {
    subject: PresentationText
    parcels: PresentationText
}
```

```jadpo check project=translation-messages file=domains/shipping/actions/describe_order.jadpo
use shipping/types { CustomerName, Formality, ParcelCount, ShippingText }

/// What a customer is told about their order, in their language.
action describe_order(
    formality: Formality,
    name: CustomerName,
    count: ParcelCount,
    reader_locale: Locale
) -> ShippingText {
    return ShippingText {
        subject: translations.shipping.order_shipped(
            formality: formality,
            name: name,
            locale: reader_locale
        )
        parcels: translations.shipping.parcels(count: count, locale: reader_locale)
    }
}
```

The text is the language's own: the plural form comes from CLDR's rules for
the locale and numbers are written with its digits and separators, from the
same ICU4X 2.3 and CLDR 48 data as `text.format`, in `jadpo-value`, so the
interpreter, a native deployable and WebAssembly write the same text. (The
wording is read while compiling into `jadpo-std`'s `Template`, carried to
the runtime as plan data, `TranslationPlan`, and written by
`present::translate`.)

```jadpo check project=translation-messages file=domains/shipping/actions/describe_order.test.jadpo
use shipping/types { Formality }

test "a message is worded by its enum value and its plural count" {
    assert translations.shipping.order_shipped(
        formality: Formality.formal,
        name: "Ana",
        locale: Locale.de_de
    ) == "Guten Tag Ana, Ihre Bestellung wurde versandt."
    assert translations.shipping.order_shipped(
        formality: Formality.casual,
        name: "Ana",
        locale: Locale.en_gb
    ) == "Hi Ana, your order is on its way!"
    assert translations.shipping.parcels(count: 1, locale: Locale.en_gb) == "1 parcel"
    assert translations.shipping.parcels(count: 1234, locale: Locale.en_gb) == "1,234 parcels"
    assert translations.shipping.parcels(count: 1234, locale: Locale.de_de) == "1.234 Pakete"
}

test "a field given without its name is the next field" {
    assert translations.shipping.delivery_window(
        "Monday",
        "9 to 12",
        locale: Locale.de_de
    ) == "Lieferung am Monday, zwischen 9 to 12."
}
```

### Email subjects

An `EmailMessage`'s member in its domain's language files is its subject
line, with the email's fields as placeholders; its body is its `.html` and
`.txt` templates (area 16). A subject is worded where its emails are
written, checked when a language file words it, and it is not a call.
Since chunk 7.3, which sends emails, it is required in every supported
locale that isn't in progress, as a message's wording is.

```jadpo check project=translation-email file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=translation-email file=domains/accounts/emails/welcome_email.jadpo hidden
/// Sent when someone signs up.
type WelcomeEmail = EmailMessage {
    display_name: Text
}
```

```jadpo check project=translation-email file=domains/accounts/emails/welcome_email.txt hidden
Welcome to Acme, {{ display_name }}.
```

```jadpo check project=translation-email file=domains/accounts/translations/en_gb.jadpo
translations {
    welcome_email: "Welcome, {display_name}"
}
```

```jadpo check project=translation-email file=domains/accounts/translations/fr_fr.jadpo hidden
translations {
    welcome_email: "Bienvenue, {display_name}"
}
```

### Enums with wording

An enum can be written out once its domain's language files give each of
its values a wording, checked like a message with one enum field. A value's
own fields are placeholders in that value's wording, both in the enum's
wording and in a message that has the enum as its field; another value's
fields are not available. In a template `{{ order.status }}` then
writes the wording (built in chunk 7.3), and code reads it with
`translations.<domain>.<enum>(value, locale: ...)`. Writing an enum out
without wording is an error that says to add it, in a template. Area 2's
"enums have no display labels" stands: the wording lives in translations.
An enum is worded in every supported locale as soon as any language file
words it; one nobody words needs none.

```jadpo check project=translation-enums file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=translation-enums file=domains/shipping/types/hold_reason.jadpo hidden
/// Why a parcel is on hold.
type HoldReason = Text {
    max_length: 80
}
```

```jadpo check project=translation-enums file=domains/shipping/types/order_status.jadpo
use shipping/types { HoldReason }

type OrderStatus = Enum {
    pending
    on_hold: {
        reason: HoldReason
    }
    shipped
}
```

```jadpo check project=translation-enums file=domains/shipping/types/status_text.jadpo hidden
/// What a customer is told about the status.
type StatusText = Output {
    status: PresentationText
}
```

```jadpo check project=translation-enums file=domains/shipping/actions/describe_status.jadpo
use shipping/types { OrderStatus, StatusText }

/// An order's status, in the customer's language.
action describe_status(status: OrderStatus, reader_locale: Locale) -> StatusText {
    return StatusText {
        status: translations.shipping.order_status(status, locale: reader_locale)
    }
}
```

```jadpo check project=translation-enums file=domains/shipping/translations/en_gb.jadpo
translations {
    order_status: {
        pending: "Waiting to be packed"
        on_hold: "Held: {reason}"
        shipped: "On its way"
    }
}
```

```jadpo check project=translation-enums file=domains/shipping/translations/fr_fr.jadpo hidden
translations {
    order_status: {
        pending: "En attente d'emballage"
        on_hold: "Suspendue : {reason}"
        shipped: "En route"
    }
}
```

```jadpo check project=translation-enums file=domains/shipping/actions/describe_status.test.jadpo
use shipping/types { OrderStatus }

test "an enum is worded by its value, with the value's fields" {
    assert translations.shipping.order_status(
        OrderStatus.pending,
        locale: Locale.fr_fr
    ) == "En attente d'emballage"
    assert translations.shipping.order_status(
        OrderStatus.on_hold {
            reason: "no stock"
        },
        locale: Locale.en_gb
    ) == "Held: no stock"
}
```

### Failures for client apps

A `Failure`'s text for client apps is keyed by its code (`code:` when
given) in its domain's
language files, mapped like a message: its `client: { }` fields are the
placeholders, and `kind:` and `message:` are not. The wording is never
written on the failure type: one rule holds for all text, and Jadpo's and
packages' failures could not carry the app's wording in place. Names are
unique across the project, so a failure and a message never share a key.
Every failure that can reach a response, which is one some route's `fails`
lists anywhere in the project, must have wording in every supported locale;
any other may be worded and needn't be. A client field no wording writes is
not an error: the client is sent it in `details` anyway. The not-found failure
the compiler provides for an entity (area 6) is worded in the entity's
domain, as one the project declares is. A project that declares no
`locales { }` words no failures.

A client field that is a date, a time or a number is sent as a value (an
`Instant` stays an instant on the wire, area 7), and the client formats it
for its own language before filling the placeholder, so unlike a
message's, a failure's date or time field can be a placeholder. The
field's type decides how the client formats it:

| Client field type | Format |
|---|---|
| `CalendarDate` | `date` |
| `Instant` | `date_time`, shown in the reader's own zone by the client |
| `ZonedTime` | `date_time`, in the zone it carries |
| `Duration` | `duration` |
| `Int` | `number` |
| `Decimal` | `decimal` |
| `Text`, `PresentationText`, `Uuid`, `Email`, `Url`, `IpAddress` | none: written as it is |
| A type the client cannot format (an object, a list, an enum, `T?`) | `translation.field_type`, as for a message's placeholder |

A type built on one of these is formatted as the one it is built on.

```jadpo check project=translation-failures file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=translation-failures file=api/main.jadpo_record hidden
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

POST /bookings
    authentication none
    input BookRequest (body)
    output Booked
    status 200
    fails slot_taken

GET /openapi.json
    generated openapi
    authentication none
    status 200

input BookRequest
    seats: Int { min: 0 }

output Booked
    seats: Int { min: 0 }

failure slot_taken
    status 409
    next_free: Instant
    seats_left: Int { min: 0 }
```

```jadpo check project=translation-failures file=policy.jadpo hidden
use booking

policy {
    actions: {
        booking.book: [Access.public]
    }
}
```

```jadpo check project=translation-failures file=domains/booking/types/seat_count.jadpo hidden
/// How many seats.
type SeatCount = Int {
    min: 0
}
```

```jadpo check project=translation-failures file=domains/booking/types/slot_time.jadpo hidden
/// When a table is free.
type SlotTime = Instant {}
```

```jadpo check project=translation-failures file=domains/booking/types/book_request.jadpo hidden
use booking/types { SeatCount }

/// A booking to make.
type BookRequest = Input {
    seats: SeatCount
}
```

```jadpo check project=translation-failures file=domains/booking/types/booked.jadpo hidden
use booking/types { SeatCount }

/// A booking made.
type Booked = Output {
    seats: SeatCount
}
```

```jadpo check project=translation-failures file=domains/booking/failures/slot_taken.jadpo
use booking/types { SeatCount, SlotTime }

/// The slot was taken while the booking was made.
type SlotTaken = Failure {
    kind: FailureKind.conflict
    client: {
        next_free: SlotTime
        seats_left: SeatCount
    }
}
```

```jadpo check project=translation-failures file=domains/booking/actions/book.jadpo hidden
use booking/types { Booked, SeatCount }
use booking/failures { SlotTaken }

/// Books seats, or says when the next slot is.
action book(seats: SeatCount) fails SlotTaken -> Booked {
    if (seats > 4) {
        reject SlotTaken {
            next_free: "2026-10-20T18:30:00Z"
            seats_left: 2
        }
    }
    return Booked {
        seats: seats
    }
}
```

```jadpo check project=translation-failures file=domains/booking/routes/bookings.jadpo hidden
use booking/types { BookRequest, Booked }
use booking/actions { book }

routes Bookings {
    prefix: /bookings

    POST / {
        authentication: RouteAuthentication.none
        input: BookRequest
        output: Booked
        run: book(input.seats)
    }
}
```

```jadpo check project=translation-failures file=domains/booking/translations/en_gb.jadpo hidden
translations {
    slot_taken: "That slot has just gone. The next is {next_free}, with {seats_left} seats left."
}
```

```jadpo check project=translation-failures file=domains/booking/translations/fr_fr.jadpo
translations {
    slot_taken: "Ce créneau vient d'être pris. Le prochain est {next_free}, avec {seats_left} places."
}
```

`jadpo check` (there is no `jadpo build` yet) writes, beside each API's
OpenAPI document in `build/openapi/`, one file per supported locale named
`<api>.failures.<locale>.json` (`main.failures.fr_fr.json`), for client apps
to show. It maps each
failure code, its keys sorted as in every generated JSON file, to an
object of the same shape, so a client has one way to
read it: `text` is the wording in `{field}` syntax, a literal brace doubled
as in a language file, and `formats` lists only the placeholders the
wording writes that the client must format, each as one of `number`,
`decimal`, `date`, `date_time` or `duration`. A locale in progress is sent
what the server would write for it, its regional sibling's wording and then
the default's, so a client has text for every code. The example above
writes this for `fr_fr` (only `slot_taken`'s entry is shown; the file also
holds Jadpo's own failures, below):

```json
{
  "slot_taken": {
    "formats": {
      "next_free": "date_time",
      "seats_left": "number"
    },
    "text": "Ce créneau vient d'être pris. Le prochain est {next_free}, avec {seats_left} places."
  }
}
```

The server's `message` stays fixed English for developers and logs (area 4).

Jadpo's own failures (`invalid_input`, `rate_limited` and the rest) are
worded with Jadpo's own messages (built in chunk 5.4e, "Jadpo's and
packages' text" below) and join every API's file: the ones its routes can
meet, which are the request failures for any API with routes, and the
failures of signing in for one with a route that needs a caller. A Jadpo
failure's text is the app's root wording for the locale, else what Jadpo
ships in it, else the in-progress fallback. The app's
own failure under the same code is sent in place of Jadpo's. An API with
routes therefore has a file for every supported locale, whether or not the
app declares failures of its own.

```jadpo check project=translation-failures file=translations/fr_fr.jadpo
translations {
    jadpo: {
        deadline_exceeded: "Cela a pris trop de temps, et rien n'a été enregistré. Réessayez."
        idempotency_key_reused: "Cette requête a déjà été envoyée avec d'autres détails. Recommencez."
        internal_fault: "Un problème est survenu de notre côté. Réessayez bientôt."
        invalid_input: "Certaines informations envoyées ne sont pas valides. Vérifiez-les et réessayez."
        invalid_request: "La requête n'a pas pu être lue. Réessayez."
        method_not_allowed: "Cette adresse ne permet pas cette action."
        rate_limited: "Trop de requêtes. Patientez un instant, puis réessayez."
        request_too_large: "Ce qui a été envoyé est trop volumineux. Envoyez moins et réessayez."
        route_not_found: "Nous n'avons pas trouvé ce que vous cherchiez."
        unavailable: "Un service dont dépend l'application est indisponible pour le moment. Réessayez bientôt."
    }
}
```

### Jadpo's and packages' text

> Built in chunk 5.4e: Jadpo's own messages and the root file. Built in
> chunk 7.1b3: library packages' text, its groups in the root file, its
> lines in the root record and its place in the publishing bar; the
> choices are under "Chosen in chunk 7.1b3" below. Built in chunk 7.1b4:
> its text in `jadpo translations export`, `import` and `unused`, under
> "Chosen in chunk 7.1b4". Built in chunk 7.1c3: two versions of a
> package worded side by side (area 11, "Wording two versions of a
> package", decided 2026-10-20), under "Chosen in chunk 7.1c3".

Text Jadpo writes itself (built-in emails and its failures' wording; it
generates endpoints only, never pages) is Jadpo's own messages. Jadpo ships
their wording in `en_gb` and `en_us` at first, and releases add locales; an
app supporting any other locale must word them itself, and until it does
`jadpo check` fails with one error for each such locale, pointing at the
locale in `supported:` (or at the missing file's name), whose reason lists
every missing message (`translation.jadpo_missing`; an `in_progress:`
locale falls back instead). The default locale is held to the same: a
default Jadpo does not ship must word the messages itself.
`translation.field_unused` does not apply to Jadpo's messages. Only the messages of features the app has switched on count:
rewording one of a feature that is off (magic-link wording without
`magic_link:`) is an error saying the feature is off
(`translation.feature_off`), and `jadpo translations builtin` lists only the
switched-on features' messages, with their fields. Every Jadpo message may
use `{app_name}`, the application's `display_name:`; an app that needs the
name translated rewords the messages. The magic-link email's messages are
listed in area 16, "Built-in emails".

Jadpo's messages are a catalogue, data in `jadpo-std` (`jadpo_text`): each
has a key, a kind (a message, a failure or an enum), the feature that
switches it on, the fields it may write besides `{app_name}`, a description
for translators and the wording Jadpo ships. They have no runtime call:
`translations.jadpo` is not a scope code can call. The wording of
Jadpo's enums the app has on is in the `TranslationPlan` as
`jadpo.<key>`, which an email template's `{{ locale.direction }}`
writes (chunk 7.3).

A feature is switched on by what the project has, and a message's name is
its key in the `jadpo: { }` group (a failure's is its code):

| Feature | On when | Its messages |
|---|---|---|
| Requests | an API serves routes | `invalid_request`, `invalid_input`, `route_not_found`, `method_not_allowed`, `request_too_large`, `idempotency_key_reused`, `rate_limited`, `unavailable`, `internal_fault`, `deadline_exceeded` |
| Signing in | a route needs a caller | `authentication_required`, `invalid_credentials`, `ambiguous_credentials`, `not_permitted`, `authentication_unavailable` |
| Signing users in | an API accepts a browser session or an API token | `user_disabled` |
| Signing services in | an API accepts a service key | `service_disabled` |
| Browser sessions | an API accepts browser sessions and a route that needs a caller writes | `csrf_rejected` |
| Locales | the app declares `locales { }` | `text_direction`, the wording of `TextDirection` |
| Magic links | the app signs people in with magic links (area 8, not built) | the messages in area 16, "Built-in emails" |

The failures of signing in that only some methods can give
(`user_disabled`, `service_disabled`, `csrf_rejected`) are on only where an
API accepts those methods, by the same rule as the OpenAPI document's
responses. An API's file for clients carries the Jadpo failures of its own
features; the application needs the union over its APIs. The wording of an
idempotency key still running (`conflict`, 409) is the app's: `conflict` is
a code an app declares too, so Jadpo does not word it. The twelve
magic-link messages are in the catalogue, but no project has the feature
on until area 8's `sign_in:` is built, so none is listed, required or
rewordable; privacy requests' messages (area 15) join the catalogue with
that feature. `TextDirection`'s wording, `ltr` and `rtl`, is the same HTML
values in every language, so Jadpo ships it in every locale and no app
locale has to word it; an email template's `{{ locale.direction }}`
writes it (chunk 7.3).

A library package carries translations as a domain does: a
`translations/` folder with its message types, its failures' wording and
one `<locale>.jadpo` per language it supports. The app calls a package's
message as `Postcodes.translations.<message>(...)`. A provider package
shows users no text, because its failures are always mapped to the app's
own, so it carries none.

The root `translations/<locale>.jadpo` holds the app's wording for text it
did not write, grouped by owner: `jadpo: { }` for Jadpo's own and the
package's name for a package's. This is the one place a language file
groups, the group being whose text it is, and each group is checked
against its owner's message and failure types. A member outside any group
is `translation.wording_shape`, and a group that names no owner,
`translation.group_unknown`. The
root folder holds language files and nothing else (`layout.wrong_folder`
for any other declaration; `Place::RootTranslations`), and a root file with
no `locales { }` is `translation.locales_required`. A
wording of a Jadpo message is checked as any is: only `{app_name}` and the
message's own fields may be written, and `text_direction`, which is Jadpo's
wording of its `TextDirection` enum, has one wording for `left_to_right` and
one for `right_to_left`.

Here the booking app above rewords what Jadpo ships in English and gives
`text_direction` its wording:

```jadpo check project=translation-failures file=translations/en_gb.jadpo
translations {
    jadpo: {
        rate_limited: "Steady on: {app_name} is busy. Try again in a minute."
        text_direction: {
            left_to_right: "ltr"
            right_to_left: "rtl"
        }
    }
}
```

A package's or Jadpo's message is looked up in the app's root wording for
that locale, then the owner's own; for an in-progress locale, then the
regional sibling in the same order, then the default locale in the same
order. The app's supported locales must be complete for every package and
Jadpo message the app can show, so an English-only package in an app that
supports French is an error until the root file supplies the French.

Here an app supporting English and French installs a package that words
its text in English only, calls its message, and words the French itself:

```jadpo check project=translation-packages file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=translation-packages file=packages/postcodes/v2/package.jadpo hidden
/// UK postcodes: reading them as people write them.
package Postcodes.v2 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=translation-packages file=packages/postcodes/v2/failures/invalid_postcode.jadpo hidden
/// The text isn't a UK postcode.
public type InvalidPostcode = Failure {
    kind: FailureKind.invalid_value
}
```

```jadpo check project=translation-packages file=packages/postcodes/v2/translations/postcode_hint.jadpo
/// Shown under a postcode field.
public type PostcodeHint = Message {}
```

```jadpo check project=translation-packages file=packages/postcodes/v2/translations/en_gb.jadpo
translations {
    invalid_postcode: "That isn't a UK postcode."
    postcode_hint: "Write it as it is on a letter, such as SW1A 1AA."
}
```

```jadpo check project=translation-packages file=jadpo.lock hidden
// Generated by Jadpo from the installed packages; update it with `jadpo package add`, `update` or `remove`, never by hand.
record lock 1

package Postcodes.v2 version 2.0.0 path ../postcodes digest sha256:219659d29cd2cd169f6f0d2cd1d6a0dbd2566ea91e65bdd73def7ca0dcaeda56
```

```jadpo check project=translation-packages file=domains/intake/actions/postcode_hint_text.jadpo
use packages/postcodes

/// What the postcode field says, in the reader's language.
action postcode_hint_text(reader_locale: Locale) -> PresentationText {
    return Postcodes.translations.postcode_hint(locale: reader_locale)
}
```

```jadpo check project=translation-packages file=translations/fr_fr.jadpo
translations {
    postcodes: {
        invalid_postcode: "Ce code postal n'existe pas."
        postcode_hint: "Écrivez-le comme sur une lettre."
    }
}
```

```jadpo check project=translation-packages file=domains/intake/actions/postcode_hint_text.test.jadpo
test "the package words the English and the app the French" {
    assert postcode_hint_text(Locale.en_gb) == "Write it as it is on a letter, such as SW1A 1AA."
    assert postcode_hint_text(Locale.fr_fr) == "Écrivez-le comme sur une lettre."
}
```

Without `translations/fr_fr.jadpo`, French has nothing to show:

```jadpo check project=translation-package-missing file=packages/postcodes/v2/package.jadpo hidden
/// UK postcodes: reading them as people write them.
package Postcodes.v2 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=translation-package-missing file=packages/postcodes/v2/translations/postcode_hint.jadpo hidden
/// Shown under a postcode field.
public type PostcodeHint = Message {}
```

```jadpo check project=translation-package-missing file=packages/postcodes/v2/translations/en_gb.jadpo hidden
translations {
    postcode_hint: "Write it as it is on a letter, such as SW1A 1AA."
}
```

```jadpo check project=translation-package-missing file=jadpo.lock hidden
// Generated by Jadpo from the installed packages; update it with `jadpo package add`, `update` or `remove`, never by hand.
record lock 1

package Postcodes.v2 version 2.0.0 path ../postcodes digest sha256:a6099979ba84a4240ad2c765eb37b5c7a8510345a84c534bcb39fef2e6563aef
```

```jadpo check project=translation-package-missing file=app.jadpo expect=translation.file_missing
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```text output
error: `Locale.fr_fr` is supported but package `Postcodes` has no wording in it for `postcode_hint`
 --> app.jadpo:5:31
  |
5 |     supported: [Locale.en_gb, Locale.fr_fr]
  |                               ^^^^^^^^^^^^ no `translations/fr_fr.jadpo`
  reason: Package `Postcodes` ships no `Locale.fr_fr` wording of `postcode_hint`, and the application supports `Locale.fr_fr`, so the application words it in its root file, under the package's folder name. A locale that isn't finished yet is listed in `in_progress:`, and falls back to the default until it is.
  next: Add `translations/fr_fr.jadpo` wording `postcode_hint` under `postcodes: { }` (agent)
  rule: translation.file_missing
  help: docs/language/10-configuration-and-time.html#translation-file-missing

check failed: 1 error, 4 files
```

To show a package failure under a different code, not just different
words, map it to the app's own failure at the call with `attempt`, as for
any package failure (area 11, Calling).

A package changing a message's or failure's fields is a breaking change by
its surface diff, so the app's rewording fails to compile until it is
updated; a package changing its own wording marks the app's rewording
stale through `sources.jadpo_record`, and the same holds for Jadpo's: the
root folder's `sources.jadpo_record` records, for each wording the app
gives outside the default locale, a hash of the default wording it was made
from, which is the app's own default-locale wording of that message or, if
the app gives none, what Jadpo ships; a default locale Jadpo does not ship
and the app does not word has nothing to compare with, so its translations
are not judged. A library package is published with
complete wording in its own default locale and a `///` description on
every public message.

While the app holds two versions of a library package side by side (area
11, "Where packages live"), it words each under a key carrying the
version, `postcodes_v2: { }` and `postcodes_v3: { }`, as its names carry
it; with one version held, the key is the folder's name. Here the app's
own `Postcodes` is held at `v2` and `v3` while callers move across, and
the French words each:

```jadpo check project=translation-package-versions file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=translation-package-versions file=shared/providers/postcodes/v2/package.jadpo hidden
/// UK postcodes: reading them as people write them.
package Postcodes.v2 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=translation-package-versions file=shared/providers/postcodes/v2/translations/postcode_hint.jadpo hidden
/// Shown under a postcode field.
public type PostcodeHint = Message {}
```

```jadpo check project=translation-package-versions file=shared/providers/postcodes/v2/translations/en_gb.jadpo hidden
translations {
    postcode_hint: "Write it as it is on a letter, such as SW1A 1AA."
}
```

```jadpo check project=translation-package-versions file=shared/providers/postcodes/v3/package.jadpo hidden
/// UK postcodes: reading them as people write them.
package Postcodes.v3 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=translation-package-versions file=shared/providers/postcodes/v3/translations/postcode_hint.jadpo hidden
/// Shown under a postcode field.
public type PostcodeHint = Message {}
```

```jadpo check project=translation-package-versions file=shared/providers/postcodes/v3/translations/en_gb.jadpo
translations {
    postcode_hint: "Write it as it is on a letter, spaces and all."
}
```

```jadpo check project=translation-package-versions file=domains/intake/actions/old_hint.jadpo
use shared/providers/postcodes

/// The hint as callers showed it before the upgrade.
action old_hint(reader_locale: Locale) -> PresentationText {
    return Postcodes.v2.translations.postcode_hint(locale: reader_locale)
}
```

```jadpo check project=translation-package-versions file=domains/intake/actions/new_hint.jadpo
use shared/providers/postcodes

/// The hint with the next version.
action new_hint(reader_locale: Locale) -> PresentationText {
    return Postcodes.v3.translations.postcode_hint(locale: reader_locale)
}
```

```jadpo check project=translation-package-versions file=translations/fr_fr.jadpo
translations {
    postcodes_v2: {
        postcode_hint: "Écrivez-le comme sur une lettre."
    }
    postcodes_v3: {
        postcode_hint: "Écrivez-le comme sur une lettre, espaces compris."
    }
}
```

```jadpo check project=translation-package-versions file=domains/intake/actions/old_hint.test.jadpo
test "the earlier version is worded under its key" {
    assert old_hint(Locale.fr_fr) == "Écrivez-le comme sur une lettre."
}
```

```jadpo check project=translation-package-versions file=domains/intake/actions/new_hint.test.jadpo
test "the next version is worded under its key" {
    assert new_hint(Locale.fr_fr) == "Écrivez-le comme sur une lettre, espaces compris."
    assert new_hint(Locale.en_gb) == "Write it as it is on a letter, spaces and all."
}
```

When the second version is added, the app's wording under the folder's
name no longer says which version it words. It was written while the
earlier version was held alone, so the check's edit moves it to that
version's key; the new version is worded afresh. When one of two is
removed, the other's key goes back to the folder's name, and the removed
version's group is the app's to delete:

```jadpo check project=translation-package-versions-moved file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
    in_progress: [Locale.fr_fr]
}
```

```jadpo check project=translation-package-versions-moved file=shared/providers/postcodes/v2/package.jadpo hidden
/// UK postcodes: reading them as people write them.
package Postcodes.v2 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=translation-package-versions-moved file=shared/providers/postcodes/v2/translations/postcode_hint.jadpo hidden
/// Shown under a postcode field.
public type PostcodeHint = Message {}
```

```jadpo check project=translation-package-versions-moved file=shared/providers/postcodes/v2/translations/en_gb.jadpo hidden
translations {
    postcode_hint: "Write it as it is on a letter, such as SW1A 1AA."
}
```

```jadpo check project=translation-package-versions-moved file=shared/providers/postcodes/v3/package.jadpo hidden
/// UK postcodes: reading them as people write them.
package Postcodes.v3 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=translation-package-versions-moved file=shared/providers/postcodes/v3/translations/postcode_hint.jadpo hidden
/// Shown under a postcode field.
public type PostcodeHint = Message {}
```

```jadpo check project=translation-package-versions-moved file=shared/providers/postcodes/v3/translations/en_gb.jadpo hidden
translations {
    postcode_hint: "Write it as it is on a letter, spaces and all."
}
```

```jadpo check project=translation-package-versions-moved file=domains/intake/actions/old_hint.jadpo hidden
use shared/providers/postcodes

/// The hint as callers showed it before the upgrade.
action old_hint(reader_locale: Locale) -> PresentationText {
    return Postcodes.v2.translations.postcode_hint(locale: reader_locale)
}
```

```jadpo check project=translation-package-versions-moved file=translations/fr_fr.jadpo expect=translation.group_unknown
translations {
    postcodes: {
        postcode_hint: "Écrivez-le comme sur une lettre."
    }
    postcodes_v3: {
        postcode_hint: "Écrivez-le comme sur une lettre, espaces compris."
    }
}
```

```text output
error: `postcodes` words package `Postcodes`, which is held in two versions
 --> translations/fr_fr.jadpo:2:5
  |
2 |     postcodes: {
  |     ^^^^^^^^^ which version?
  reason: While two versions of a package are held side by side, the application words each under its folder name and version, as its names carry the version: `postcodes_v2: { }` for `Postcodes.v2` and `postcodes_v3: { }` for `Postcodes.v3`. This group was written while one version was held, so it words the earlier one unless it says otherwise.
  next: Rename the group to `postcodes_v2`, or to the other version's key if it words that one (agent)
  rule: translation.group_unknown
  help: docs/language/10-configuration-and-time.html#translation-group-unknown

check failed: 1 error, 9 files
```

#### Chosen in chunk 7.1b3

What the pages leave to the build, each the most conservative reading:

- **A package's group is its folder's name.** The root file words
  `packages/postcodes/v2/`'s text under `postcodes: { }`, the name a
  domain can't share (`package.name_taken`); the record's lines and
  `jadpo translations missing` name its members `postcodes.<member>`.
- **What the app can word and call.** A package's public messages, its
  enums with wording and its failures, each by the name its own files use;
  a member naming anything else, or something the package keeps private,
  is `translation.member_unknown`, and a call of a message that isn't
  `public` is `package.not_public`. A package's own code doesn't call its
  messages: the app does, and `translations.postcodes.<message>(...)` is
  `translation.message_unknown` with the call through the package as its
  edit.
- **The package's own files** are checked by themselves, each against the
  locale its name gives, whether or not the app supports it: a package is
  published apart from any app. Their wording, shapes, placeholders and
  plural forms are checked as a domain's are.
- **What a supported locale needs.** Every public message, an enum once
  the package or the app words it, and a failure some route answers with,
  in the root file or the package's own file for that locale. With a root
  file for the locale, each one missing is `translation.wording_missing`;
  without one, the locale is one `translation.file_missing` naming them.
  A locale in progress falls back, as for the app's own messages.
- **A package's default locale.** The pages give a package no default
  locale, so the publishing bar holds every language it ships to
  completeness: each language file words every public message, and an
  enum once any file words it (a `wording` row, shown when the package
  carries translations), and a package with messages and no language file
  fails it. *Flagged for Richard:* this is stricter than "complete wording
  in its own default locale"; a `locale:` key in `package.jadpo` could
  name the default and let other languages ship part-done, at the cost of
  the index's two keys. Recommendation: keep it strict until a package
  needs to ship a language unfinished.
- **The surface.** The package's entry has a `locale` line for each
  language it words; each public message is an entry, `message
  translations.<message>` with its fields, after the functions. A new
  locale or message is the next minor version, a message's changed fields
  or a dropped locale are breaking, and changed wording is a patch.
- **The commands.** `missing`, `update` and `accept` take a package's
  text in; `export`, `import` and `unused` do from chunk 7.1b4, below.

#### Chosen in chunk 7.1b4

- **What is exchanged.** What a supported locale must word of a package
  (above) and anything else of it the app words in its root file, each
  unit `postcodes.<member>` with `.<value>` or `.<form>`, in an XLIFF
  `<file id="postcodes" original="translations">` after the app's own and
  before Jadpo's. It is translated from the app's default-locale wording
  of it, else the package's, the wording the record's `from` hashes. A
  package in a folder named `jadpo` or like a scope of the app's would
  share unit ids, so export refuses it, as it does a domain named `jadpo`.
- **What a locale has.** The app's root wording, else the package's own
  for that locale, which is what the app shows; the package's own is
  `translated`, never `reviewed` or stale, since no record of the app's
  judges it. Exporting and importing unchanged writes nothing.
- **What comes back** is the app's: written into the root file for the
  locale, in the package's group (created, beside `jadpo: { }`, when the
  file lacks one), the whole member when one plural form or value
  changed, its other forms from the package's wording, and a line in the
  root record. The package's files are never written: a translator who
  changes the package's wording makes the app reword it. Import's rows
  name a package's units `postcodes.<member>`, as `missing` does.
- **`unused`** lists, after the app's own and under `Package wording`,
  each member of the root files' package groups that shows nothing, with
  its place and why: a message or enum of the package no code of the
  app's calls (`no code calls it`), a name the package no longer has
  (`not in Postcodes`) or keeps private (`private to Postcodes`), and a
  group no installed library package words text under (`no such
  package`). These are the members to delete. A package's message the
  app neither calls nor words is the package's, and is not listed; a
  failure is shown by being reachable, as the app's are; Jadpo's group
  is not a package's. `--json` gives them as `packageWording`, each with
  a `reason` (`uncalled`, `notInPackage`, `private`, `noPackage`).

#### Chosen in chunk 7.1c3

Two versions of a library package side by side (area 11, Decisions,
"Wording two versions of a package", Richard 2026-10-20), each the most
conservative reading:

- **The key.** Exactly while names through the package carry the version
  (area 11, "Chosen in chunk 7.1c2"), the app words each version's text
  under `<folder>_<version>`, `postcodes_v3: { }`, even when only one of
  the two carries translations; otherwise under the folder's name. The
  key is the same everywhere the folder's name was: the root files'
  groups, the root record's members (`de_de.postcodes_v3.postcode_hint`),
  `jadpo translations missing`, `export`'s `<file id="postcodes_v3">` and
  unit ids, what `import` writes into, and `unused`, which names the
  package with its version (`not in Postcodes.v3`). The errors name the
  package as code does, `Postcodes.v3`. A domain named like a version's
  key is `package.name_taken` while two versions are held, as one named
  like the folder always is.
- **Adding a second version moves nothing by itself.** The app's wording
  under the folder's name was written while one version was held, so
  `jadpo check` reports the group (`translation.group_unknown`, "held in
  two versions") with the edit that renames it to the earlier version's
  key, unless the file already has that group, when its members are moved
  by hand. The new version starts with none of the app's wording: it may
  have changed its messages, so nothing is copied, and the check's
  missing wording lists what each supported locale needs of it.
- **Removing one of two** puts the other's key back to the folder's name,
  reported with the edit that renames it ("held in one version"); the
  removed version's group words nothing (`translation.group_unknown`,
  "which the app doesn't hold"), and is the author's to delete. Neither
  `jadpo package add` nor `remove` rewrites the app's root files, as they
  don't rewrite its names (chunk 7.1c2), and `jadpo fix` stays the type
  clashes' (area 14).
- **The record follows the wording.** A translation with no line under
  its new key takes the line it had under the package's other key in the
  same locale, the folder's name or another version's, when nothing is
  worded under that key any more and the wording is the one the line
  hashes; so it keeps its acceptance, and is stale when the version it
  now words has different default wording from what it was translated
  from. Wording changed on the way is a new translation, recorded afresh. `jadpo translations update` writes the
  line under the new key (`moved with its wording to the package's new
  key`) and drops the old one.
- **The publishing bar** checks one package in its own folder, where one
  version is held, so its `wording` row is unchanged.

### What the compiler checks

Built in chunk 5.4c, each with its own error below:

- **Errors**: a member no message, enum, email or failure declares
  (`translation.member_unknown`, with the nearest name); a member written
  twice (`translation.member_duplicate`); a member in the wrong shape for
  what it words (`translation.wording_shape`: a string where a block of
  values is needed, or the reverse); a wording that is not a plain string
  with `{field}` placeholders (`translation.wording_invalid`); a variant that
  is not a value of the enum, or a plural form the language does not use
  (`translation.variant_unknown`); a missing variant or plural form
  (`translation.variant_missing`); a placeholder that is not a field
  (`translation.placeholder_unknown`); an amount of money or a percentage
  written without its style, or a style on another field
  (`translation.placeholder_style`, chunks 5.5h2 and 5.5j2); a field type a wording cannot hold
  (`translation.field_type`), or more than one enum or `PluralCount` field
  (`translation.too_many_selectors`); a language file that is not named for
  a locale (`translation.file_name_invalid`) or is for one the application
  does not support (`translation.locale_unsupported`); a call of no message
  (`translation.message_unknown`); a `locale:` that is not a supported
  locale (`type.locale_unsupported`); and a message, a failure a route can
  answer with, or an enum once any language words it, missing from any
  supported locale not listed in `in_progress:` (`translation.file_missing` for a locale with no language
  file, `translation.wording_missing` for a file that lacks a member).
  Every supported locale must be complete. The missing wording's `next:`
  names the file and the member to add, with the placeholders to use
  (agent). Translations with no `locales { }` are
  `translation.locales_required`.
- **A judgement call**: a field no wording in a language uses
  (`translation.field_unused`; one variant leaving a field out is normal).
  There are no warnings in Jadpo (area 1, question 11), so this is an error
  a person can answer with `decline translation.field_unused: "<reason>"`
  above the member, which is right for a language that needs no such word.
- **In progress**: `in_progress:` is the explicit, declared opt-out. A
  locale it lists falls back where its language file has no wording, with no
  diagnostic: to the same language's regional sibling first (`fr_ca` tries
  `fr_fr`, if supported), then to the default locale, and the sentence is
  written entirely in the language it came from. Wording the file does
  give is checked as for any locale, so a mistake in it is still an error.
  Removing the locale from `in_progress:` makes the gaps errors again.
  Quality comes from the review setting below, not from waiting.
  `jadpo translations missing` lists what an in-progress locale falls back
  on.

Built in chunk 5.4d, each with its own error below:

- **Sources**: each `translations/` folder has a generated, committed
  `sources.jadpo_record` (a record in area 18's format, `record
  translations 1`, written by `jadpo translations update`, listed by `jadpo
  records` and merged by git's `union`): one line for each translation
  outside the default locale, sorted, with a hash of the default wording it
  was translated from (`from`) and of its own wording when that was
  recorded (`wording`). A hash is the first 12 hex digits of BLAKE3 over the
  member's whole text, every value and plural form in name order, so
  reordering a block changes nothing. A translation whose `from` no longer matches the default
  wording today is stale (`translation.stale`), an error until it is
  translated again and recorded; an in-progress locale's stale translation
  is listed by `jadpo translations missing` instead. A translation with no
  line yet is not stale: it was never recorded, so nothing says what it
  was made from (with review required, it is new and unaccepted), and a
  project that never runs `jadpo translations update` sees no stale error.
  A line Jadpo cannot read is `translation.sources_invalid`, which `jadpo
  translations update` fixes; a header Jadpo didn't write, or lines out of
  order, is area 18's `record.edited` (and a newer format
  `record.format_newer`), as for every record.
- **Review**: `translations: { review: TranslationReview.required }` in
  `application { }` (the default is `TranslationReview.none`) makes every
  new or changed wording outside the default locale unaccepted until a
  person runs `jadpo translations accept <locale>`, which records the
  acceptance in the same record. A line says a wording is accepted only
  while the wording is the one the line hashes. Unaccepted wording is
  listed by `jadpo translations missing`, and in the approval report once
  `jadpo approval` exists (area 17). It is not an error, so `jadpo check`
  passes with it: the host's code review is the gate (area 18). Jadpo cannot tell who wrote a wording, so this is "changed and
  not yet accepted", not "written by an agent". A `translations:` option
  other than `review:` is `translation.option_unknown`, and a `review:`
  that is not a `TranslationReview`, `translation.review_invalid`.

```jadpo check project=translation-review file=app.jadpo
application {
    translations: {
        review: TranslationReview.required
    }
}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.de_de]
}
```

```jadpo check project=translation-review file=domains/greeting/translations/hello.jadpo hidden
/// Says hello.
type Hello = Message {
    name: Text
}
```

```jadpo check project=translation-review file=domains/greeting/translations/en_gb.jadpo hidden
translations {
    hello: "Hello, {name}."
}
```

```jadpo check project=translation-review file=domains/greeting/translations/de_de.jadpo hidden
translations {
    hello: "Hallo, {name}."
}
```

Built in chunk 5.4e, each with its own error below:

- **Jadpo's messages**: a supported locale that is not in progress, in
  which Jadpo ships no wording and the root file words none of the
  messages of a feature the app has on, is one error for the locale listing
  what is missing (`translation.jadpo_missing`), pointing at `jadpo
  translations builtin`; a message of a feature that is off
  (`translation.feature_off`); a group of the root file that is no owner
  (`translation.group_unknown`). Everything else about a group's wording is
  checked as a folder's is: `translation.member_unknown` (with the nearest
  of Jadpo's messages), `member_duplicate`, `wording_shape`,
  `wording_invalid`, `placeholder_unknown`, `variant_missing` and
  `variant_unknown`.
- **Sources**: the root folder has its own `sources.jadpo_record`, kept by
  the same commands, with the owner in the member name
  (`translation fr_fr.jadpo.invalid_input from ... wording ...`), as a
  package's is `postcodes.<message>`.

Built in chunk 7.1b3, with the errors above:

- **A library package's text**: its own language files checked by
  themselves, as a domain's are; its group in the root file checked
  against its public messages, enums and failures
  (`translation.member_unknown` with the nearest of them); each supported
  locale not in progress wording all of it in the root file or the
  package's file (`translation.wording_missing` with a root file for the
  locale, `translation.file_missing` without); a call through the package
  of a message it doesn't have (`translation.message_unknown`) or doesn't
  make `public` (`package.not_public`); and a root record line made from
  the app's default wording or, when the app gives none, the package's,
  stale when that changes (`translation.stale`).

### Tooling

Built in chunk 5.4d, with `--json` on every command that prints (area 14).
The commands succeed whatever they list: they answer a question, and `jadpo
check` is what fails. `update` and `accept` stop on syntax errors.

- `jadpo translations missing [<locale>]` lists, under the language file
  each is in, the wording a locale lacks (and, for a locale in progress,
  what it falls back on), the translations made from wording that has since
  changed, and, with review required, the wording not yet accepted.
  `jadpo translations unused` lists the messages, and the enums with
  wording, that no code calls; a test does not count as a user. Failures
  and emails are used by being reachable and by the email's own machinery,
  so they are never listed. It also lists the app's wording of library
  packages' text that shows nothing (chunk 7.1b4, "Jadpo's and packages'
  text").
- `jadpo translations update` writes each folder's `sources.jadpo_record`.
  It can't tell a translation was redone, only that it changed: a
  translation with no line, or reworded since its line, is recorded as
  made from today's default wording, and loses its acceptance. One made from
  wording that changed and was not reworded stays stale, its line
  untouched, so the error stays until someone translates it again; `update`
  and `accept` say `still stale`. `jadpo translations accept <locale>`
  does the same for that locale and then accepts everything of its that is
  not stale, so a person does not run both. It takes a supported locale
  besides the default and the pseudo-locale; anything else is
  `cli.translations_arguments`, naming the ones it takes.
- Go to definition goes from a `translations.<domain>.<message>(...)` call
  to the message's type first, then to its member in each language file in
  path order; an enum's call the same.
- The pseudo-locale, `Locale.en_xa`, renders every message accented and
  lengthened, for development and tests, to expose text that is hard-coded
  and layouts that are too tight. Its tag is the one Android and Chrome
  use, in a region ISO 3166 leaves for private use, so it can't clash with
  a real locale; `Locale` has 534 variants, 533 from CLDR and this one. It
  is listed in `supported:` like any locale (an app takes it out before
  releasing, or keeps it for a staging build), needs no language file, is
  never asked for wording, and has no translations to list or accept. It
  writes the default locale's wording, with that locale's plural rules and
  number format, its letters accented (`a` is `á`), about two fifths
  longer, in brackets, so a cut end shows as a missing `]`; the values a
  wording writes are not accented. A language file `en_xa.jadpo`, if one
  exists, is written as it is. Failures' client text is exported in it the
  same way.

```jadpo check project=translation-pseudo file=app.jadpo
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.en_xa]
}
```

```jadpo check project=translation-pseudo file=domains/shop/translations/parcels.jadpo hidden
/// How many parcels an order has.
type Parcels = Message {
    count: PluralCount
}
```

```jadpo check project=translation-pseudo file=domains/shop/translations/en_gb.jadpo hidden
translations {
    parcels: {
        one: "{count} parcel"
        other: "{count} parcels"
    }
}
```

```jadpo check project=translation-pseudo file=tests/pseudo_locale.test.jadpo
test "the pseudo-locale accents and lengthens the default wording" {
    assert translations.shop.parcels(count: 1, locale: Locale.en_xa) == "[1 þáŕçéļ ~~~~]"
    assert translations.shop.parcels(count: 1234, locale: Locale.en_xa) == "[1,234 þáŕçéļš ~~~~~~]"
}
```

Built in chunk 5.4e, with `--json`: `jadpo translations builtin` lists
Jadpo's own messages the app can show, by feature, each with the fields it
may write, the wording Jadpo ships to translate from, and `needs` and the
locale for each supported locale whose lack of it fails the check, the
default included (none for a locale in progress). It succeeds whatever it
lists. Its summary counts the messages, the ones that need wording in the
problem colour, and the features; its one hint is `jadpo check`. `jadpo translations missing` and `update`,
`accept` take the root folder in with the rest: its gaps are listed under
`translations/<locale>.jadpo` as `jadpo.<message>` (never for the default
locale, which the error covers), and an in-progress locale falls back to
what Jadpo ships.

```text
Requests
  invalid_request         needs fr_fr
    The request couldn't be read. Try again.
  invalid_input           needs fr_fr
    Some of the information sent isn't valid. Check it and try again.
  ...

Locales
  text_direction
    left_to_right  ltr
    right_to_left  rtl

11 messages: 10 need wording, 2 features

next: jadpo check  fails until the application words them
```

Built in chunk 5.4f, with `--json`: `jadpo translations export [<locale>]`
and `jadpo translations import <file>`, which exchange the project's
messages with translation services (Crowdin, Lokalise or any that reads
XLIFF or JSON): every message, enum with wording, failure and email
subject the default locale words, Jadpo's own messages (those that differ
by language), the `///` description of each, and each locale's wording.

`export` writes one file for each translated locale (a supported locale
besides the default and the pseudo-locale), or for the one named, to
`build/translations/<locale>.xlf`, the same each time and overwritten by
the next export; `--format json` writes `<locale>.json` and `--out
<folder>` another folder, relative to the project and under it.
Naming the default locale writes the source file to upload: the same units
with no translations. A unit is one wording: a plain message, a value of
the enum that picks a wording, or a plural form of the locale; its id is
`<domain>.<message>`, with `.<value>` or `.<form>` after it (`shared` for
`shared/translations/`, `jadpo` for Jadpo's own, a library package's folder
name for its text, chunk 7.1b4), which XLIFF splits into
`<file id>` and `<unit id>`. A domain named `jadpo` would share ids with
Jadpo's own messages, so export refuses it, and wording every locale
shares (`TextDirection`'s) is not exchanged. The wording is written as it
is in the language file, `{field}` placeholders and doubled braces
included, not as XLIFF inline `<ph>` codes, so the same text goes to both
formats, round-trips exactly, and a service sees ordinary text (both
services recognise `{x}` as a placeholder). A plural form the default
locale has no form of is translated from the default's `other`.

```text
Wrote build/translations/de_de.xlf
    de_de  5 units, 5 translated

Wrote build/translations/pl_pl.xlf
    pl_pl  7 units, 7 to translate

Exported 2 locales: 12 units, xliff

next: jadpo translations import build/translations/pl_pl.xlf  reads the translations back
```

The XLIFF is 2.0 (namespace `urn:oasis:names:tc:xliff:document:2.0`), the
version every tool that reads 2.x reads: a `<file>` for each domain, a `<unit>` for each wording
with `description`, `context` (the enum value or plural form it is for),
`placeholders` and, for wording translated from default wording that has
since changed, `stale` notes, and a `<segment>` whose `state` is `initial`
without a translation, `translated` with one and `reviewed` when a person
accepted it. The JSON is `{ "<id>": { "message": ..., "description": ... } }`,
the shape Chrome extensions use for `messages.json`, which has the one
place for a description that Crowdin imports as it is ("Chrome JSON":
message and description fields, verified in its store listing); `message`
is empty until translated. A flat `{ id: text }` object is the shape both
services take, but it carries no description, which the exchange exists to
send. *Unverified:* that Lokalise reads XLIFF 2.0 (nor could Crowdin's
documentation be reached for the version) and that it imports the Chrome
shape; each format is the other's fallback.

```text
<unit id="parcels.one">
  <notes>
    <note category="description">How many parcels an order has, on the tracking page.</note>
    <note category="context">Used for the plural form `one` of `count`.</note>
    <note category="placeholders">{count}</note>
  </notes>
  <segment state="translated">
    <source xml:space="preserve">{count} parcel</source>
    <target xml:space="preserve">{count} Paket</target>
  </segment>
</unit>
```

`import` reads a file by what it starts with (`<` XLIFF 2.0 or 2.1, `{`
JSON, plain strings accepted for `message`, so a flat object from a service
that cannot keep descriptions reads too), for the locale it names
(XLIFF's `trgLang`), `--locale`, or a JSON file's name (`pl_pl.json`).
XLIFF 1.2 is refused with a message saying to ask the service for 2.x,
since the two differ in structure. Inline codes a service adds on the way
back (`<ph>`, `<pc>`) are read as the text they stand for. A unit with no
translation is left alone, and a status in the file (`reviewed`) is never
read back: acceptance is a person's `jadpo translations accept`. It then does what a person editing
the language files would have to: each changed wording goes into its
language file where its member is (the file's other members, comments and
order kept) or at the end, a missing file is created, the files are laid
out by `jadpo fmt`, and the folder's `sources.jadpo_record` gets a line
for each changed translation (only those, so a round trip never creates or
changes one), made from today's default wording and not accepted (a person
accepts, with `accept`). Before anything is written the project is checked
as `jadpo check` checks it, by the checker's own rules with nothing
special-cased for the exchange, and any new finding refuses
the whole import, with that finding's own rule, pointing at the unit of the
imported file that caused it: `translation.placeholder_unknown`,
`translation.wording_invalid`, `translation.variant_missing` for a plural
form or enum value left out, the declinable `translation.field_unused`
for a placeholder every wording dropped. A unit naming nothing the project
words, or a plural form the locale does not use, is
`translation.exchange_unit_unknown` with the nearest id; a file that is
not XLIFF 2.x or JSON, is not well formed, holds a unit twice or in
several segments, or was translated from another default language is
`translation.exchange_file_invalid` (area 14's command errors, so they exit
1: the command ran and the file was wrong). All reasons
are listed at once and nothing is written. An in-progress locale may be
translated in part, as its language file may. Exporting and importing
unchanged writes nothing and says `Already up to date`.

```text
Wrote domains/shipping/translations/pl_pl.jadpo
    farewell              added
    order_shipped.formal  added
    order_shipped.casual  added
    parcels.one           added
    parcels.few           added
    parcels.many          added
    parcels.other         added

Wrote domains/shipping/translations/sources.jadpo_record

Imported pl_pl: 7 added

next: jadpo check  confirms the imported wording
```

XLIFF carries what a translation was made from, so a unit whose `<source>`
is not today's default wording (the English changed while the service
worked) is written all the same and recorded as made from other wording:
it is stale, counted as `stale` in the summary, and `jadpo check` fails
with `translation.stale` until it is translated again, as for wording
changed by hand: it is the one exception to refusing, because the work is
not lost and `translation.stale` already says what to do. JSON does not
carry its source, so its translations are taken as made from today's
wording.

The app's own data (product names, articles) is not a translation: an app
models translated content as ordinary fields. Library packages' translations
are built in chunk 7.1b3 (open item 8 of the
[packages proposals](research/packages-and-extension-proposals.md)).

## Retention periods

Retention periods are ordinary settings (`soft_delete_retention: Duration`)
read by a lifecycle's `purge: { after: config.soft_delete_retention }`
(area 5), which takes a `Duration` setting that isn't secret
(`data.lifecycle_purge_after` otherwise). The period is the value the app
started with, so a purge run is given it (`jadpo_runtime::purge_after`), and
one that is not above zero deletes nothing. How retention fits the wider GDPR support (personal-data marking,
erasure, export) is designed in area 15 (Personal data).

## Errors

> The errors below run with this checkout's `jadpo`: the configuration errors (chunk 5.1), `type.clock_context` and the time errors after it (chunk 5.2), the formatting errors (chunk 5.2b) and the locale errors at the end (chunk 5.3).

### `configuration.binding_empty`

```jadpo check project=e-binding-empty expect=configuration.binding_empty
config {
    mail_sender: Email {
        binding: ""
    }
}
```

```text output
error: `binding:` names no variable
 --> main.jadpo:3:18
  |
3 |         binding: ""
  |                  ^^ empty
  reason: `binding:` names the environment variable a setting is read from; an empty name reads nothing.
  next: Name the variable, or leave `binding:` out to read the setting's name in capitals (agent)
  rule: configuration.binding_empty
  help: docs/language/10-configuration-and-time.html#configuration-binding-empty

check failed: 1 error, 1 file
```

### `configuration.binding_invalid`

```jadpo check project=e-binding-invalid expect=configuration.binding_invalid
config {
    mail_sender: Email {
        binding: "MAIL-SENDER"
    }
}
```

```text output
error: `MAIL-SENDER` can't be a variable name
 --> main.jadpo:3:18
  |
3 |         binding: "MAIL-SENDER"
  |                  ^^^^^^^^^^^^^ not a variable name
  reason: A variable name is capital letters, digits and `_`, starting with a capital letter, so every platform accepts it.
  next: Use `MAIL_SENDER` (agent)
  rule: configuration.binding_invalid
  help: docs/language/10-configuration-and-time.html#configuration-binding-invalid

check failed: 1 error, 1 file
```

### `configuration.duplicate_binding`

```jadpo check project=e-duplicate-binding expect=configuration.duplicate_binding
config {
    mail_sender: Email {
        binding: "MAIL"
    }

    reply_to: Email {
        binding: "MAIL"
    }
}
```

```text output
error: `reply_to` and `mail_sender` both read `MAIL`
 --> main.jadpo:7:18
  |
3 |         binding: "MAIL"
  |                  ------ first read here
7 |         binding: "MAIL"
  |                  ^^^^^^ the same variable
  reason: Each setting has its own environment variable; two settings sharing one would always hold the same value, and renaming either would change both.
  next: Give `reply_to` a variable of its own with `binding:`, or merge the two settings (agent)
  rule: configuration.duplicate_binding
  help: docs/language/10-configuration-and-time.html#configuration-duplicate-binding

check failed: 1 error, 1 file
```

### `configuration.duplicate_option`

```jadpo check project=e-duplicate-option expect=configuration.duplicate_option
config {
    mail_sender: Email {
        binding: "MAIL_SENDER"
        binding: "SENDER"
    }
}
```

```text output
error: `binding:` is given twice on `mail_sender`
 --> main.jadpo:4:9
  |
3 |         binding: "MAIL_SENDER"
  |         ------- first given here
4 |         binding: "SENDER"
  |         ^^^^^^^ given again
  reason: A setting says each of `secret:`, `default:`, `binding:` and `test:` once, so there is one answer to read.
  next: Remove one `binding:` (agent)
  rule: configuration.duplicate_option
  help: docs/language/10-configuration-and-time.html#configuration-duplicate-option

check failed: 1 error, 1 file
```

### `configuration.unknown_option`

```jadpo check project=e-unknown-option expect=configuration.unknown_option
config {
    mail_sender: Email {
        binding: "MAIL_SENDER"
        required: true
    }
}
```

```text output
error: `required` isn't an option a setting takes
 --> main.jadpo:4:9
  |
4 |         required: true
  |         ^^^^^^^^ unknown option
  reason: `mail_sender` can say `secret:`, `default:`, `binding:` and `test:`. A setting is required unless it has a `default:`; there are no optional settings, so give a default or leave the setting required.
  next: Remove `required` (agent)
  rule: configuration.unknown_option
  help: docs/language/10-configuration-and-time.html#configuration-unknown-option

check failed: 1 error, 1 file
```

### `configuration.expected_field`

```jadpo check project=e-expected-field expect=configuration.expected_field
config {
    mail_sender
}
```

```text output
error: `mail_sender` isn't a setting
 --> main.jadpo:2:5
  |
2 |     mail_sender
  |     ^^^^^^^^^^^ not `name: Type`
  reason: `config { }` holds settings, each written `name: Type`, with a block after the type when it has `secret:`, `default:`, `binding:` or `test:`.
  next: Write the setting as `name: Type`, or remove this line (agent)
  rule: configuration.expected_field
  help: docs/language/10-configuration-and-time.html#configuration-expected-field

check failed: 1 error, 1 file
```

### `configuration.multiple_declarations`

```jadpo check project=e-multiple expect=configuration.multiple_declarations
config {
    mail_sender: Email {
        binding: "MAIL_SENDER"
    }
}

config {
    browser_origin: Url {
        binding: "BROWSER_ORIGIN"
    }
}
```

```text output
error: `config { }` is declared twice
 --> main.jadpo:7:1
  |
1 | config {
  | ------ first declared here
7 | config {
  | ^^^^^^ declared again
  reason: Every setting the application needs is a field of the one `config { }`, in `config.jadpo`, so one place lists everything a deployment must provide.
  next: Move the settings this `config` holds into the first, and remove it (agent)
  rule: configuration.multiple_declarations
  help: docs/language/10-configuration-and-time.html#configuration-multiple-declarations

check failed: 1 error, 1 file
```

### `configuration.type_unsupported`

```jadpo check project=e-type-unsupported expect=configuration.type_unsupported
config {
    admin_emails: List<Email>
}
```

```text output
error: `admin_emails` can't be a `List<Email>`
 --> main.jadpo:2:19
  |
2 |     admin_emails: List<Email>
  |                   ^^^^^^^^^^^ not read from one variable
  reason: A setting is read from one environment variable, so its type must be decodable from one piece of text: text-based types, `Int`, `Decimal`, `Bool`, `Uuid`, `Email`, `Url`, `IpAddress`, `Duration`, `Instant`, `CalendarDate`, `ModelId`, `Money`, `Certificate` and enums. A list is one value in many variables or one variable holding a format; split it into settings.
  next: Split `admin_emails` into settings of those types (agent)
  rule: configuration.type_unsupported
  help: docs/language/10-configuration-and-time.html#configuration-type-unsupported

check failed: 1 error, 1 file
```

### `configuration.default_literal_required`

```jadpo check project=e-default-literal expect=configuration.default_literal_required
config {
    page_size: Int {
        default: largest_page
    }
}
```

```text output
error: `default:` on `page_size` isn't a literal
 --> main.jadpo:3:18
  |
3 |         default: largest_page
  |                  ^^^^^^^^^^^^ not a literal
  reason: A `default:` is a literal of the setting's type, checked while compiling (`default: 25`, `default: 5s`); it can't be computed, because nothing runs before the application starts.
  next: Write a literal value after `default:`, or remove it (agent)
  rule: configuration.default_literal_required
  help: docs/language/10-configuration-and-time.html#configuration-default-literal-required

check failed: 1 error, 1 file
```

Here `largest_page` names nothing. A constant of the setting's type is
refused the same way (built in chunk 5.3e): a value is a constant or a
setting, never both ([A constant or a setting](#a-constant-or-a-setting);
revised 2026-10-16, Richard, reversing the same day's decision that a
constant counts as a literal here). For a constant, the `next:` offers
`jadpo config move` (built in chunk 5.3f), which makes the constant this
setting: its value becomes the `default:` and every read of it reads
`config.page_size`:

```jadpo check project=e-default-constant expect=configuration.default_literal_required
type Pence = Int {
    min: 0
}

constant largest_page: Pence = 100

config {
    page_size: Pence {
        default: largest_page
    }
}

function page() -> Pence {
    return Pence(config.page_size + largest_page)
}
```

```text output
error: `default:` on `page_size` isn't a literal
 --> main.jadpo:9:18
  |
9 |         default: largest_page
  |                  ^^^^^^^^^^^^ not a literal
  reason: A `default:` is a literal of the setting's type, checked while compiling (`default: 25`, `default: 5s`); it can't be computed, because nothing runs before the application starts.
  next: Run `jadpo config move largest_page`, which makes the constant this setting, or write a literal value after `default:` (agent)
  rule: configuration.default_literal_required
  help: docs/language/10-configuration-and-time.html#configuration-default-literal-required

check failed: 1 error, 1 file
```

### `configuration.default_type`

```jadpo check project=e-default-type expect=configuration.default_type
config {
    page_size: Int {
        default: "many"
    }
}
```

```text output
error: `"many"` can't be the default of `page_size`
 --> main.jadpo:3:18
  |
3 |         default: "many"
  |                  ^^^^^^ the wrong kind of value
  reason: `page_size` is `Int`, and this is a different kind of value; the default must be a literal of the setting's own type.
  next: Write a `Int` value after `default:` (agent)
  rule: configuration.default_type
  help: docs/language/10-configuration-and-time.html#configuration-default-type

check failed: 1 error, 1 file
```

### `configuration.default_invalid`

A PostgreSQL address is not a `Url`:

```jadpo check project=e-default-invalid expect=configuration.default_invalid
type PrimaryDatabase = Url {}

config {
    primary_database: PrimaryDatabase {
        default: "postgres://localhost:5432/todo"
    }
}
```

```text output
error: `"postgres://localhost:5432/todo"` isn't a valid `PrimaryDatabase`
 --> main.jadpo:5:18
  |
5 |         default: "postgres://localhost:5432/todo"
  |                  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not a valid value
  reason: The value isn't an `http://` or `https://` URL with a host, such as `https://example.com`; a default is checked while compiling, with the rules of the setting's type.
  next: Write a valid `PrimaryDatabase` value (agent)
  rule: configuration.default_invalid
  help: docs/language/10-configuration-and-time.html#configuration-default-invalid

check failed: 1 error, 1 file
```

### `configuration.secret_default`

```jadpo check project=e-secret-default expect=configuration.secret_default
type PartnerKey = Text {
    min_length: 8
}

config {
    api_key: PartnerKey {
        secret: true
        default: "development-key"
    }
}
```

```text output
error: `api_key` is secret, and has a `default:` in source
 --> main.jadpo:8:9
  |
8 |         default: "development-key"
  |         ^^^^^^^ a secret written in source
  reason: A secret's value comes only from the deployment's environment, so a forgotten production secret stops the app instead of falling back to a value written in source; `jadpo test` generates its own for a test run, so it needs no `default:` either.
  next: Remove `default:` from `api_key` (agent)
  rule: configuration.secret_default
  help: docs/language/10-configuration-and-time.html#configuration-secret-default

check failed: 1 error, 1 file
```

### `configuration.secret_invalid`

```jadpo check project=e-secret-invalid expect=configuration.secret_invalid
config {
    api_key: Text {
        secret: yes
    }
}
```

```text output
error: `secret:` on `api_key` isn't `true` or `false`
 --> main.jadpo:3:17
  |
3 |         secret: yes
  |                 ^^^ not a flag
  reason: `secret: true` marks a value that must never leave the server; the setting is either secret or it is not.
  next: Write `secret: true`, or leave `secret:` out (agent)
  rule: configuration.secret_invalid
  help: docs/language/10-configuration-and-time.html#configuration-secret-invalid

check failed: 1 error, 1 file
```

### `configuration.secret_flow`

```jadpo check project=e-secret-flow expect=configuration.secret_flow
type PartnerKey = Text {
    min_length: 8
}

config {
    api_key: PartnerKey {
        secret: true
    }
}

function expose_secret() -> PartnerKey {
    return config.api_key
}
```

```text output
error: `config.api_key` is a secret
 --> main.jadpo:12:12
   |
12 |     return config.api_key
   |            ^^^^^^^^^^^^^^ a secret, read here
  reason: A secret never leaves the server, so it is accepted only where the compiler itself owns the place: a connection's `url:`, an integration's `credential:`, an authentication `signing_key:` and a `crypto` call's `key:`. Returning it, storing it, logging it, putting it in a failure, building another value from it or comparing it with something could leak it.
  next: Give `config.api_key` to the connection, integration or authentication that needs it in `app.jadpo` or `authentication.jadpo`, or as a `crypto` call's `key:`, or leave `secret: true` off `api_key` if its value is public (agent)
  rule: configuration.secret_flow
  help: docs/language/10-configuration-and-time.html#configuration-secret-flow

check failed: 1 error, 1 file
```

### `configuration.field_unknown`

```jadpo check project=e-field-unknown expect=configuration.field_unknown
type FromAddress = Email {}

config {
    mail_sender: FromAddress
}

function from_address() -> FromAddress {
    return config.mail_sendr
}
```

```text output
error: `config.mail_sendr` isn't a setting
 --> main.jadpo:8:12
  |
8 |     return config.mail_sendr
  |            ^^^^^^^^^^^^^^^^^ no such setting
  reason: `config { }` declares no setting called `mail_sendr`. Did you mean `mail_sender`?
  next: Use `config.mail_sender` instead of `config.mail_sendr` (agent)
  rule: configuration.field_unknown
  help: docs/language/10-configuration-and-time.html#configuration-field-unknown

check failed: 1 error, 1 file
```

### `configuration.url_type`

```jadpo check project=e-url-type expect=configuration.url_type file=config.jadpo
config {
    cache_url: RedisUrl
}
```

```jadpo check project=e-url-type expect=configuration.url_type file=app.jadpo
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.cache_url
        }
    }
}
```

```text output
error: `main`'s address must be a `PostgresUrl` setting
 --> app.jadpo:5:18
  |
5 |             url: config.cache_url
  |                  ^^^^^^^^^^^^^^^^ `cache_url` is `RedisUrl`
  reason: A connection opened with `driver: Driver.postgres` takes an address of its own type, which checks the scheme and is always secret; `config.cache_url` is `RedisUrl`.
  next: Declare `<name>: PostgresUrl` in `config.jadpo`, and write `url: config.<name>` (agent)
  rule: configuration.url_type
  help: docs/language/10-configuration-and-time.html#configuration-url-type

check failed: 1 error, 2 files
```

### `configuration.test_value_missing`

A test reads each setting's `test:` value, or its default; this one has neither:

```jadpo check project=e-test-value-missing expect=configuration.test_value_missing
type Greeting = Text {
    max_length: 80
}

config {
    greeting: Greeting
}

function hello() -> Greeting {
    return config.greeting
}

test "the greeting is read" {
    assert hello() == "hello"
}
```

```text output
error: `config.greeting` has no value in tests
 --> main.jadpo:10:12
   |
10 |     return config.greeting
   |            ^^^^^^^^^^^^^^^ no `test:` or `default:`
  reason: The test "the greeting is read" reaches this read. A test reads each setting's `test:` value, or its `default:`, and `greeting` has neither; only a secret is made up for a run, and `greeting` isn't one.
  next: Give `greeting` a `test:` value, or a `default:`, in `config.jadpo` (agent)
  rule: configuration.test_value_missing
  help: docs/language/10-configuration-and-time.html#configuration-test-value-missing

check failed: 1 error, 1 file
```

### `configuration.client_address_duplicate`

`client_address:` is declared once, in `app.jadpo`; a second one:

```jadpo check project=e-client-address-duplicate expect=configuration.client_address_duplicate file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.connection
    }
    client_address: {
        source: ClientAddressSource.cloudflare
    }
}
```

```text output
error: `client_address:` is declared twice
 --> app.jadpo:5:5
  |
2 |     client_address: {
  |     -------------- first declared here
5 |     client_address: {
  |     ^^^^^^^^^^^^^^ declared again
  reason: The client's address is read one way for the whole application, so every request is counted by the same rule.
  next: Keep one `client_address:` and remove the other (agent)
  rule: configuration.client_address_duplicate
  help: docs/language/10-configuration-and-time.html#configuration-client-address-duplicate

check failed: 1 error, 1 file
```

### `configuration.client_address_invalid`

`client_address:` is a block of `source:` and `hops:` and nothing else:

```jadpo check project=e-client-address-invalid expect=configuration.client_address_invalid file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.forwarded_for
        hop: 1
    }
}
```

```text output
error: `hop` isn't an option of `client_address:`
 --> app.jadpo:4:9
  |
4 |         hop: 1
  |         ^^^ unknown option
  reason: `client_address:` takes `source:` and `hops:`, and nothing else; which proxies to trust is a count and not a list. Did you mean `hops`?
  next: Use `hops` instead of `hop` (agent)
  rule: configuration.client_address_invalid
  help: docs/language/10-configuration-and-time.html#configuration-client-address-invalid

check failed: 1 error, 1 file
```

### `configuration.client_address_source_unknown`

`source:` is one of the four `ClientAddressSource` values:

```jadpo check project=e-client-address-source-unknown expect=configuration.client_address_source_unknown file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.forward_for
        hops: 1
    }
}
```

```text output
error: `ClientAddressSource.forward_for` isn't a source of the client's address
 --> app.jadpo:3:17
  |
3 |         source: ClientAddressSource.forward_for
  |                 ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not a `ClientAddressSource`
  reason: `ClientAddressSource` has no value called `forward_for`; the four are `connection`, `forwarded_for`, `forwarded`, `cloudflare`. Did you mean `forwarded_for`?
  next: Use `ClientAddressSource.forwarded_for` instead (agent)
  rule: configuration.client_address_source_unknown
  help: docs/language/10-configuration-and-time.html#configuration-client-address-source-unknown

check failed: 1 error, 1 file
```

### `configuration.client_address_hops_invalid`

`hops:` counts proxies from 1, a whole number or an `Int` setting that declares `min:` of 1 or more:

```jadpo check project=e-client-address-hops-invalid expect=configuration.client_address_hops_invalid file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.forwarded_for
        hops: 0
    }
}
```

```text output
error: `0` can't be a number of proxies
 --> app.jadpo:4:15
  |
4 |         hops: 0
  |               ^ below 1
  reason: `hops:` is how many proxies of the deployment's own sit in front of the app: a whole number from 1, or an `Int` setting whose type declares `min:` of at least 1.
  next: Write a whole number from 1, such as `hops: 1`, or read an `Int` setting that declares `min: 1` (agent)
  rule: configuration.client_address_hops_invalid
  help: docs/language/10-configuration-and-time.html#configuration-client-address-hops-invalid

check failed: 1 error, 1 file
```

### `configuration.client_address_hops_unused`

`cloudflare` and `connection` count no proxies, so `hops:` means nothing with them:

```jadpo check project=e-client-address-hops-unused expect=configuration.client_address_hops_unused file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.cloudflare
        hops: 1
    }
}
```

```text output
error: `hops:` means nothing with `cloudflare`
 --> app.jadpo:4:15
  |
4 |         hops: 1
  |               ^ no proxies are counted
  reason: Cloudflare sets `CF-Connecting-IP` on every request from its edge and overwrites a caller's, so there is no list to count from the right.
  next: Remove `hops:` (agent)
  rule: configuration.client_address_hops_unused
  help: docs/language/10-configuration-and-time.html#configuration-client-address-hops-unused

check failed: 1 error, 1 file
```

### `type.clock_context`

```jadpo check project=e-clock-context expect=type.clock_context
type ReminderCutoff = Instant {}

function cutoff() -> ReminderCutoff {
    return ReminderCutoff(clock.now)
}
```

```text output
error: A function can't read `clock.now`
 --> main.jadpo:4:27
  |
4 |     return ReminderCutoff(clock.now)
  |                           ^^^^^^^^^ reads the clock
  reason: A function is pure: it gives the same result for the same arguments, so the time it needs arrives as a parameter; only actions read the clock.
  next: Pass the time in as a parameter, read in the action that calls this (agent)
  rule: type.clock_context
  help: docs/language/10-configuration-and-time.html#type-clock-context

check failed: 1 error, 1 file
```

### `type.temporal_clock_invalid`

```jadpo check project=e-clock-invalid expect=type.temporal_clock_invalid
type ShopDay = CalendarDate {}

type Opening = ZonedTime {}

function opening(day: ShopDay) -> Opening {
    return Opening(temporal.resolve(day, at: "9:30", zone: Zone.utc, dst: Dst.later))
}
```

```text output
error: `"9:30"` isn't a clock time
 --> main.jadpo:6:46
  |
6 |     return Opening(temporal.resolve(day, at: "9:30", zone: Zone.utc, dst: Dst.later))
  |                                              ^^^^^^ not `HH:MM`
  reason: `temporal.resolve`'s `at:` is a clock time written `HH:MM`, or `HH:MM:SS` with seconds, each part two digits, from `00:00` to `23:59`.
  next: Write the time with two-digit hours and minutes, such as `"09:30"` (agent)
  rule: type.temporal_clock_invalid
  help: docs/language/10-configuration-and-time.html#type-temporal-clock-invalid

check failed: 1 error, 1 file
```

### `type.temporal_option_unused`

```jadpo check project=e-option-unused expect=type.temporal_option_unused
type ShopDay = CalendarDate {}

function next_day(day: ShopDay) -> ShopDay {
    return temporal.add_days(day, 1, dst: Dst.compatible)
}
```

```text output
error: `dst:` means nothing for a `ShopDay`
 --> main.jadpo:4:38
  |
4 |     return temporal.add_days(day, 1, dst: Dst.compatible)
  |                                      ^^^ `ShopDay` has no clock time
  reason: `dst:` chooses an instant when a local time falls in a daylight-saving gap or overlap; `temporal.add_days` on a `ShopDay` moves a date, which has no clock time, so no such time arises.
  next: Remove `dst: ...` (agent)
  rule: type.temporal_option_unused
  help: docs/language/10-configuration-and-time.html#type-temporal-option-unused

check failed: 1 error, 1 file
```

`zone:` on a date or a local time is reported the same way: a
`ZonedTime` carries its zone, and a `CalendarDate` has none.

### `type.temporal_zone_required`

```jadpo check project=e-zone-required expect=type.temporal_zone_required
type OrderedAt = Instant {}

function placed_label(ordered_at: OrderedAt, reader_locale: Locale) -> PresentationText {
    return text.format(ordered_at, locale: reader_locale, format: TimeFormat.date_long)
}
```

```text output
error: `text.format` needs the zone to show `OrderedAt` in
 --> main.jadpo:4:12
  |
4 |     return text.format(ordered_at, locale: reader_locale, format: TimeFormat.date_long)
  |            ^^^^^^^^^^^ no `zone:`
  reason: An `Instant` is one moment for everyone, and its date and clock time depend on where it is read; the server never guesses a zone, so `zone:` says which.
  next: Add `zone: ...` with the reader's zone, such as `zone: Zone.europe_london` (agent)
  rule: type.temporal_zone_required
  help: docs/language/10-configuration-and-time.html#type-temporal-zone-required

check failed: 1 error, 1 file
```

### `type.temporal_format_invalid`

```jadpo check project=e-format-invalid expect=type.temporal_format_invalid
type OrderedAt = Instant {}

function placed_label(
    ordered_at: OrderedAt,
    zone: Zone,
    reader_locale: Locale
) -> PresentationText {
    return text.format(
        ordered_at,
        zone: zone,
        locale: reader_locale,
        format: TimeFormat.date_longest
    )
}
```

```text output
error: `TimeFormat.date_longest` isn't a time format
 --> main.jadpo:12:17
   |
12 |         format: TimeFormat.date_longest
   |                 ^^^^^^^^^^^^^^^^^^^^^^^ not a format
  reason: `text.format`'s `format:` is written out: a built-in format, `TimeFormat.` followed by `date_`, `time_` or `date_time_` and `full`, `long`, `medium` or `short`, or the name of a format declared as `type Name = TimeFormat { ... }`.
  next: Write a built-in format such as `TimeFormat.date_long`, or declare one with `type Name = TimeFormat { ... }` (agent)
  rule: type.temporal_format_invalid
  help: docs/language/10-configuration-and-time.html#type-temporal-format-invalid

check failed: 1 error, 1 file
```

A format named as the type of a field, a parameter or a variable, or used
as a value, is reported the same way: it holds no value.

### `type.temporal_format_requires_time`

```jadpo check project=e-requires-time expect=type.temporal_format_requires_time
type InvoiceDate = CalendarDate {}

function invoice_dated(day: InvoiceDate, reader_locale: Locale) -> PresentationText {
    return text.format(day, locale: reader_locale, format: TimeFormat.date_time_long)
}
```

```text output
error: `TimeFormat.date_time_long` shows a clock time, which `InvoiceDate` doesn't have
 --> main.jadpo:4:60
  |
4 |     return text.format(day, locale: reader_locale, format: TimeFormat.date_time_long)
  |                                                            ^^^^^^^^^^^^^^^^^^^^^^^^^ shows a clock time
  reason: `InvoiceDate` is a date, with no clock time or zone, so `text.format` can show only its weekday, day, month and year.
  next: Use a date format such as `TimeFormat.date_long`, or format an `Instant` or a `ZonedTime` instead (agent)
  rule: type.temporal_format_requires_time
  help: docs/language/10-configuration-and-time.html#type-temporal-format-requires-time

check failed: 1 error, 1 file
```

### `type.temporal_format_components_empty`

```jadpo check project=e-components-empty expect=type.temporal_format_components_empty
type Blank = TimeFormat {}
```

```text output
error: `Blank` shows nothing
 --> main.jadpo:1:6
  |
1 | type Blank = TimeFormat {}
  |      ^^^^^ no parts
  reason: A format lists the parts of a date or a time it shows, each with a `ComponentStyle`, and the locale arranges them; with none listed there is nothing to show.
  next: List the parts to show, such as `day: ComponentStyle.numeric`, or use a built-in format such as `TimeFormat.date_long` (agent)
  rule: type.temporal_format_components_empty
  help: docs/language/10-configuration-and-time.html#type-temporal-format-components-empty

check failed: 1 error, 1 file
```

### `type.temporal_format_component_invalid`

```jadpo check project=e-component-invalid expect=type.temporal_format_component_invalid
type ShortDate = TimeFormat {
    day: ComponentStyle.long
    month: ComponentStyle.short
}
```

```text output
error: `day` isn't shown `long`
 --> main.jadpo:2:10
  |
2 |     day: ComponentStyle.long
  |          ^^^^^^^^^^^^^^^^^^^ not a style `day` takes
  reason: A format's `day` is shown `ComponentStyle.numeric` or `ComponentStyle.two_digit`; the locale decides the rest, such as the order and the punctuation.
  next: Write `day: ComponentStyle.numeric` (agent)
  rule: type.temporal_format_component_invalid
  help: docs/language/10-configuration-and-time.html#type-temporal-format-component-invalid

check failed: 1 error, 1 file
```

A part that isn't one a format shows, such as `days:`, is
`type.unknown_setting`, with the nearest part's name.

### `type.temporal_format_component_duplicate`

```jadpo check project=e-component-duplicate expect=type.temporal_format_component_duplicate
type ShortDate = TimeFormat {
    day: ComponentStyle.numeric
    month: ComponentStyle.short
    day: ComponentStyle.two_digit
}
```

```text output
error: `day` is given twice
 --> main.jadpo:4:5
  |
2 |     day: ComponentStyle.numeric
  |     --- first given here
4 |     day: ComponentStyle.two_digit
  |     ^^^ given again
  reason: A format shows each part once, in one style, so there is never a question of which `day` counts.
  next: Remove one `day` (agent)
  rule: type.temporal_format_component_duplicate
  help: docs/language/10-configuration-and-time.html#type-temporal-format-component-duplicate

check failed: 1 error, 1 file
```

### `type.presentation_text_persistence`

```jadpo check project=e-presentation-persist expect=type.presentation_text_persistence
entity Receipt {
    id: Uuid
    issued_label: PresentationText
}
```

```text output
error: `Receipt.issued_label` would store text formatted for a person
 --> main.jadpo:3:19
  |
3 |     issued_label: PresentationText
  |                   ^^^^^^^^^^^^^^^^ `PresentationText`, stored
  reason: `Receipt.issued_label` holds `PresentationText`: text in one reader's language and zone, which another reader, a change of locale or a correction to the zone database would show differently, and which can't be read back as a time. A record keeps the value itself and formats it when it is shown.
  next: Store the `Instant`, `CalendarDate` or `ZonedTime` instead, and format `Receipt.issued_label` in the output that shows it (agent)
  rule: type.presentation_text_persistence
  help: docs/language/10-configuration-and-time.html#type-presentation-text-persistence

check failed: 1 error, 1 file
```

### `type.temporal_locales_required`

A project that formats text for people declares the locales it supports:

```jadpo check project=e-locales-required file=app.jadpo hidden
application {}
```

```jadpo check project=e-locales-required file=domains/shop/types/ordered_at.jadpo hidden
/// When an order was placed.
type OrderedAt = Instant {}
```

```jadpo check project=e-locales-required file=domains/shop/types/placed_text.jadpo hidden
/// An order's placing, shown to its customer.
type PlacedText = Output {
    placed: PresentationText
}
```

```jadpo check project=e-locales-required file=domains/shop/actions/show_placed.jadpo expect=type.temporal_locales_required
use shop/types { OrderedAt, PlacedText }

/// The placing, in the customer's language.
action show_placed(ordered_at: OrderedAt, reader_locale: Locale) -> PlacedText {
    return PlacedText {
        placed: text.format(
            ordered_at,
            zone: Zone.utc,
            locale: reader_locale,
            format: TimeFormat.date_long
        )
    }
}
```

```text output
error: `text.format` needs the application's locales
 --> domains/shop/actions/show_placed.jadpo:6:17
  |
6 |         placed: text.format(
  |                 ^^^^^^^^^^^ no locales declared
  reason: Text for a person is written in one of the locales the application supports. None is declared, so there is nothing to check `locale:` against, and nothing for a request with no usable `Accept-Language` to fall back to.
  next: Declare them in `app.jadpo`, for instance `locales { default: Locale.en_gb supported: [Locale.en_gb] }` (agent)
  rule: type.temporal_locales_required
  help: docs/language/10-configuration-and-time.html#type-temporal-locales-required

check failed: 1 error, 4 files
```

The same rule covers choosing a locale from a request, since there is
nothing to choose among and nothing to fall back to:

```jadpo check project=e-locales-required-choose file=app.jadpo hidden
application {}
```

```jadpo check project=e-locales-required-choose file=domains/shop/types/browser_languages.jadpo hidden
/// What a browser says its reader's languages are.
type BrowserLanguages = Text {
    max_length: 2000
}
```

```jadpo check project=e-locales-required-choose file=domains/shop/actions/locale_for_reader.jadpo expect=type.temporal_locales_required
use shop/types { BrowserLanguages }

/// The language the customer's browser asks for.
action locale_for_reader(languages_header: BrowserLanguages?) -> Locale {
    return locale.from_accept_language(languages_header)
}
```

```text output
error: `locale.from_accept_language` needs the application's locales
 --> domains/shop/actions/locale_for_reader.jadpo:5:12
  |
5 |     return locale.from_accept_language(languages_header)
  |            ^^^^^^^^^^^^^^^^^^^^^^^^^^^ no locales declared
  reason: `locale.from_accept_language` chooses among the locales the application supports and falls back to its `default:`. None is declared, so there is nothing to choose among and nothing to fall back to.
  next: Declare them in `app.jadpo`, for instance `locales { default: Locale.en_gb supported: [Locale.en_gb] }` (agent)
  rule: type.temporal_locales_required
  help: docs/language/10-configuration-and-time.html#type-temporal-locales-required

check failed: 1 error, 3 files
```

And sorting by a locale's alphabet, since a deployable carries collation
for the declared locales only (Richard, 2026-10-22):

```jadpo check project=e-locales-required-sort file=app.jadpo hidden
application {}
```

```jadpo check project=e-locales-required-sort file=domains/shop/types/loaf_name.jadpo hidden
/// A loaf's name on the shelf.
type LoafName = Text {
    min_length: 1
    max_length: 60
}
```

```jadpo check project=e-locales-required-sort file=domains/shop/actions/in_name_order.jadpo expect=type.temporal_locales_required
use shop/types { LoafName }

/// The loaves in the order the shopper's alphabet puts them.
action in_name_order(loaves: List<LoafName>, shopper: Locale) -> List<LoafName> {
    return list.sort_by(loaves, loaf -> loaf, locale: shopper)
}
```

```text output
error: `list.sort_by` needs the application's locales
 --> domains/shop/actions/in_name_order.jadpo:5:12
  |
5 |     return list.sort_by(loaves, loaf -> loaf, locale: shopper)
  |            ^^^^^^^^^^^^ no locales declared
  reason: `locale:` orders text by the alphabet of one of the locales the application supports, and a deployable carries only those locales' collation. None is declared, so there is no alphabet to sort by.
  next: Declare them in `app.jadpo`, for instance `locales { default: Locale.en_gb supported: [Locale.en_gb] }` (agent)
  rule: type.temporal_locales_required
  help: docs/language/10-configuration-and-time.html#type-temporal-locales-required

check failed: 1 error, 3 files
```

### `syntax.locales_default_required`

```jadpo check project=e-locales-default expect=syntax.locales_default_required
locales {
    supported: [Locale.en_gb]
}
```

```text output
error: `locales` has no `default:`
 --> main.jadpo:1:1
  |
1 | locales {
  | ^^^^^^^ no `default:`
  reason: The default is the locale a choice from an `Accept-Language` header falls back to, so every request ends up with a language the application supports.
  next: Add `default: Locale.en_gb` (agent)
  rule: syntax.locales_default_required
  help: docs/language/10-configuration-and-time.html#syntax-locales-default-required

check failed: 1 error, 1 file
```

### `syntax.locales_supported_required`

```jadpo check project=e-locales-supported expect=syntax.locales_supported_required
locales {
    default: Locale.en_gb
}
```

```text output
error: `locales` has no `supported:`
 --> main.jadpo:1:1
  |
1 | locales {
  | ^^^^^^^ no `supported:`
  reason: `supported:` lists every locale formatting and translations accept; a locale not listed is a compile error where it is written and an invalid value where it is read.
  next: Add `supported: [Locale.en_gb]`, listing every locale the application speaks (agent)
  rule: syntax.locales_supported_required
  help: docs/language/10-configuration-and-time.html#syntax-locales-supported-required

check failed: 1 error, 1 file
```

### `syntax.locales_unknown_option`

There is no `unsupported:` setting any more (see "Locales"). A misspelt
option (`suported:`) is reported the same way, with the nearest name:

```jadpo check project=e-locales-unknown expect=syntax.locales_unknown_option
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
    unsupported: Locale.fr_fr
}
```

```text output
error: `unsupported` isn't an option of `locales`
 --> main.jadpo:4:5
  |
4 |     unsupported: Locale.fr_fr
  |     ^^^^^^^^^^^ unknown option
  reason: There is no `unsupported:` setting. A request field typed `Locale` is decoded like any enum, so a tag the application doesn't support is an `invalid_value` failure, and choosing a language from an `Accept-Language` header falls back to `default:`.
  next: Remove `unsupported:` (agent)
  rule: syntax.locales_unknown_option
  help: docs/language/10-configuration-and-time.html#syntax-locales-unknown-option

check failed: 1 error, 1 file
```

### `syntax.locales_duplicate_option`

```jadpo check project=e-locales-twice expect=syntax.locales_duplicate_option
locales {
    default: Locale.en_gb
    default: Locale.fr_fr
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```text output
error: `default:` is given twice
 --> main.jadpo:3:5
  |
2 |     default: Locale.en_gb
  |              ------------ first given here
3 |     default: Locale.fr_fr
  |     ^^^^^^^ given again
  reason: `locales` says each of `default:` and `supported:` once.
  next: Keep one `default:` and remove the other (agent)
  rule: syntax.locales_duplicate_option
  help: docs/language/10-configuration-and-time.html#syntax-locales-duplicate-option

check failed: 1 error, 1 file
```

### `type.locale_invalid`

A locale is a name in `Locale`, not text; a tag in quotes is the common
slip, and the fix is one edit:

```jadpo check project=e-locale-invalid expect=type.locale_invalid
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, "fr-FR"]
}
```

```text output
error: `"fr-FR"` isn't written as a locale
 --> main.jadpo:3:31
  |
3 |     supported: [Locale.en_gb, "fr-FR"]
  |                               ^^^^^^^ not a `Locale`
  reason: A locale is a name in `Locale`, written `Locale.<tag>` with the tag in lower case and `_` for `-`: `Locale.en_gb` is en-GB. Closed choices are names, not text, so a misspelt one is caught while compiling.
  next: Write `Locale.fr_fr` instead (agent)
  rule: type.locale_invalid
  help: docs/language/10-configuration-and-time.html#type-locale-invalid

check failed: 1 error, 1 file
```

### `type.locale_duplicate`

```jadpo check project=e-locale-duplicate expect=type.locale_duplicate
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.en_gb]
}
```

```text output
error: `Locale.en_gb` is listed twice
 --> main.jadpo:3:31
  |
3 |     supported: [Locale.en_gb, Locale.en_gb]
  |                               ^^^^^^^^^^^^ listed again
  |                 ------------ first listed here
  reason: A locale is listed or it is not; listing it twice says nothing more.
  next: Remove the second `Locale.en_gb` (agent)
  rule: type.locale_duplicate
  help: docs/language/10-configuration-and-time.html#type-locale-duplicate

check failed: 1 error, 1 file
```

### `type.locale_default_unsupported`

```jadpo check project=e-locale-default expect=type.locale_default_unsupported
locales {
    default: Locale.fr_fr
    supported: [Locale.en_gb]
}
```

```text output
error: The default `Locale.fr_fr` isn't a supported locale
 --> main.jadpo:2:14
  |
2 |     default: Locale.fr_fr
  |              ^^^^^^^^^^^^ not in `supported:`
3 |     supported: [Locale.en_gb]
  |                -------------- the supported locales
  reason: The default is what a request with no usable `Accept-Language` is given, and formatting and translations accept only the supported locales, so the default has to be one.
  next: Add `Locale.fr_fr` to `supported:`, or choose a default that is listed (agent)
  rule: type.locale_default_unsupported
  help: docs/language/10-configuration-and-time.html#type-locale-default-unsupported

check failed: 1 error, 1 file
```

### `type.locale_unsupported`

```jadpo check project=e-locale-unsupported expect=type.locale_unsupported
type OrderedAt = Instant {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}

function placed_label(ordered_at: OrderedAt) -> PresentationText {
    return text.format(
        ordered_at,
        zone: Zone.utc,
        locale: Locale.fr_fr,
        format: TimeFormat.date_long
    )
}
```

```text output
error: `Locale.fr_fr` isn't a locale this application supports
 --> main.jadpo:12:17
   |
12 |         locale: Locale.fr_fr,
   |                 ^^^^^^^^^^^^ not in `locales`
  reason: `text.format` writes text in the locales listed in `locales { supported: }`, in `app.jadpo`; `Locale.fr_fr` is not one of them, and the server never guesses a language.
  next: Add `Locale.fr_fr` to `supported:` in `app.jadpo`, or use a locale that is listed (agent)
  rule: type.locale_unsupported
  help: docs/language/10-configuration-and-time.html#type-locale-unsupported

check failed: 1 error, 1 file
```

### `semantic.multiple_locale_declarations`

```jadpo check project=e-locales-multiple expect=semantic.multiple_locale_declarations
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}

locales {
    default: Locale.en_us
    supported: [Locale.en_us]
}
```

```text output
error: `locales { }` is declared twice
 --> main.jadpo:6:1
  |
1 | locales {
  | ------- first declared here
6 | locales {
  | ^^^^^^^ declared again
  reason: An application lists the locales it supports once, in `app.jadpo`, so formatting and translations all accept the same ones.
  next: Move what this `locales` lists into the first, and remove it (agent)
  rule: semantic.multiple_locale_declarations
  help: docs/language/10-configuration-and-time.html#semantic-multiple-locale-declarations

check failed: 1 error, 1 file
```

### `translation.file_name_invalid`

A language file is named for the locale it words, the tag written as `Locale` writes it: lower case, with `_` for `-`.

```jadpo check project=e-tr-file-name-invalid file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-file-name-invalid file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-file-name-invalid file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```jadpo check project=e-tr-file-name-invalid file=domains/shipping/translations/fr_fr.jadpo hidden
translations {
    order_shipped: "La commande de {name} a été expédiée."
}
```

```jadpo check project=e-tr-file-name-invalid file=domains/shipping/translations/pt-BR.jadpo expect=translation.file_name_invalid
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```text output
error: `domains/shipping/translations/pt-BR.jadpo` isn't named for a locale
 --> domains/shipping/translations/pt-BR.jadpo:1:1
  |
1 | translations {
  | ^^^^^^^^^^^^ `pt-BR` is no `Locale`
  reason: A language file is named for the locale it words, the tag in lower case with `_` for `-`: `en_gb.jadpo` is `Locale.en_gb`. Did you mean `pt_br`?
  next: Rename the file to `domains/shipping/translations/pt_br.jadpo` (agent)
  rule: translation.file_name_invalid
  help: docs/language/10-configuration-and-time.html#translation-file-name-invalid

check failed: 1 error, 5 files
```

### `translation.locale_unsupported`

A language file for a locale the application doesn't list in `locales { supported: }` would never be used.

```jadpo check project=e-tr-locale-unsupported file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-locale-unsupported file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-locale-unsupported file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```jadpo check project=e-tr-locale-unsupported file=domains/shipping/translations/fr_fr.jadpo hidden
translations {
    order_shipped: "La commande de {name} a été expédiée."
}
```

```jadpo check project=e-tr-locale-unsupported file=domains/shipping/translations/de_de.jadpo expect=translation.locale_unsupported
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```text output
error: `domains/shipping/translations/de_de.jadpo` is for a locale this application doesn't support
 --> domains/shipping/translations/de_de.jadpo:1:1
  |
1 | translations {
  | ^^^^^^^^^^^^ `Locale.de_de` isn't supported
  reason: Wording is written for the locales listed in `locales { supported: }`, in `app.jadpo`; a file for another is never used, and the server never guesses a language.
  next: Add `Locale.de_de` to `supported:` in `app.jadpo`, or remove the file (agent)
  rule: translation.locale_unsupported
  help: docs/language/10-configuration-and-time.html#translation-locale-unsupported

check failed: 1 error, 5 files
```

### `translation.file_missing`

Every supported locale has a language file for each folder that has messages, unless it is listed in `in_progress:`; here French has none.

```jadpo check project=e-tr-file-missing file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-file-missing file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-file-missing file=domains/shipping/translations/en_gb.jadpo expect=translation.file_missing
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```text output
error: `Locale.fr_fr` is supported but has no language file for `order_shipped`
 --> app.jadpo:5:31
  |
5 |     supported: [Locale.en_gb, Locale.fr_fr]
  |                               ^^^^^^^^^^^^ no `domains/shipping/translations/fr_fr.jadpo`
  reason: Every supported locale has a language file for each folder that has messages, so a call with `locale: Locale.fr_fr` has wording to show. A locale that isn't finished yet is listed in `in_progress:`, and falls back to the default until it is.
  next: Add `domains/shipping/translations/fr_fr.jadpo` with a `translations { }` that words `order_shipped` (agent)
  rule: translation.file_missing
  help: docs/language/10-configuration-and-time.html#translation-file-missing

check failed: 1 error, 3 files
```

### `translation.wording_missing`

A message is worded in every supported locale that isn't in progress, so a call in French has words to show.

```jadpo check project=e-tr-wording-missing file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-wording-missing file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-wording-missing file=domains/shipping/translations/parcels.jadpo hidden
/// How many parcels an order has.
type Parcels = Message {
    count: PluralCount
}
```

```jadpo check project=e-tr-wording-missing file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
    parcels: {
        one: "{count} parcel"
        other: "{count} parcels"
    }
}
```

```jadpo check project=e-tr-wording-missing file=domains/shipping/translations/fr_fr.jadpo expect=translation.wording_missing
translations {
    order_shipped: "La commande de {name} a été expédiée."
}
```

```text output
error: `parcels` has no wording in `domains/shipping/translations/fr_fr.jadpo`
 --> domains/shipping/translations/fr_fr.jadpo:1:1
  |
1 | translations {
  | ^^^^^^^^^^^^ no `parcels`
  reason: A message is worded in every supported locale that isn't in progress, so a call with `locale: Locale.fr_fr` has words to show.
  next: Add `parcels`, with a wording for each plural form (`one`, `many` and `other`) to `domains/shipping/translations/fr_fr.jadpo`, which may write `{count}` (agent)
  rule: translation.wording_missing
  help: docs/language/10-configuration-and-time.html#translation-wording-missing

check failed: 1 error, 5 files
```

A failure a route can answer with is worded the same way, keyed by its code, so a client app showing French has text for it.

```jadpo check project=e-tr-wording-missing-failure file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-wording-missing-failure file=api/main.jadpo_record hidden
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

GET /openapi.json
    generated openapi
    authentication none
    status 200

POST /openings
    authentication none
    input SlotRequest (body)
    output Slot
    status 200
    fails slot_taken

output Slot
    seats: Int { min: 0 }

input SlotRequest
    seats: Int { min: 0 }

failure slot_taken
    status 409
```

```jadpo check project=e-tr-wording-missing-failure file=policy.jadpo hidden
use booking

policy {
    actions: {
        booking.next_free: [Access.public]
    }
}
```

```jadpo check project=e-tr-wording-missing-failure file=domains/booking/types/seat_count.jadpo hidden
/// How many seats.
type SeatCount = Int {
    min: 0
}
```

```jadpo check project=e-tr-wording-missing-failure file=domains/booking/types/slot_request.jadpo hidden
use booking/types { SeatCount }

/// A slot to ask for.
type SlotRequest = Input {
    seats: SeatCount
}
```

```jadpo check project=e-tr-wording-missing-failure file=domains/booking/types/slot.jadpo hidden
use booking/types { SeatCount }

/// A slot shown.
type Slot = Output {
    seats: SeatCount
}
```

```jadpo check project=e-tr-wording-missing-failure file=domains/booking/failures/slot_taken.jadpo hidden
type SlotTaken = Failure {
    kind: FailureKind.conflict
}
```

```jadpo check project=e-tr-wording-missing-failure file=domains/booking/actions/next_free.jadpo hidden
use booking/types { SeatCount, Slot }
use booking/failures { SlotTaken }

/// The next slot, or why there is none.
action next_free(seats: SeatCount) fails SlotTaken -> Slot {
    reject SlotTaken
}
```

```jadpo check project=e-tr-wording-missing-failure file=domains/booking/routes/openings.jadpo hidden
use booking/types { Slot, SlotRequest }
use booking/actions { next_free }

routes Openings {
    prefix: /openings

    POST / {
        authentication: RouteAuthentication.none
        input: SlotRequest
        output: Slot
        run: next_free(input.seats)
    }
}
```

```jadpo check project=e-tr-wording-missing-failure file=domains/booking/translations/en_gb.jadpo hidden
translations {
    slot_taken: "That slot has just gone."
}
```

```jadpo check project=e-tr-wording-missing-failure file=translations/fr_fr.jadpo hidden
translations {
    jadpo: {
        deadline_exceeded: "Cela a pris trop de temps, et rien n'a été enregistré. Réessayez."
        idempotency_key_reused: "Cette requête a déjà été envoyée avec d'autres détails. Recommencez."
        internal_fault: "Un problème est survenu de notre côté. Réessayez bientôt."
        invalid_input: "Certaines informations envoyées ne sont pas valides. Vérifiez-les et réessayez."
        invalid_request: "La requête n'a pas pu être lue. Réessayez."
        method_not_allowed: "Cette adresse ne permet pas cette action."
        rate_limited: "Trop de requêtes. Patientez un instant, puis réessayez."
        request_too_large: "Ce qui a été envoyé est trop volumineux. Envoyez moins et réessayez."
        route_not_found: "Nous n'avons pas trouvé ce que vous cherchiez."
        unavailable: "Un service dont dépend l'application est indisponible pour le moment. Réessayez bientôt."
    }
}
```

```jadpo check project=e-tr-wording-missing-failure file=domains/booking/translations/fr_fr.jadpo expect=translation.wording_missing
translations {}
```

```text output
error: `slot_taken` has no wording in `domains/booking/translations/fr_fr.jadpo`
 --> domains/booking/translations/fr_fr.jadpo:1:1
  |
1 | translations {}
  | ^^^^^^^^^^^^ no `slot_taken`
  reason: A failure a route can answer with is worded in every supported locale that isn't in progress, so a client app showing `Locale.fr_fr` has text for its code.
  next: Add `slot_taken: "..."` to `domains/booking/translations/fr_fr.jadpo` (agent)
  rule: translation.wording_missing
  help: docs/language/10-configuration-and-time.html#translation-wording-missing

check failed: 1 error, 11 files
```

### `translation.locales_required`

Each language file is for one of the locales the application supports, and each supported locale words every message. With no `locales { }` there is nothing to check the wording against.

```jadpo check project=e-tr-locales-required file=app.jadpo hidden
application {}
```

```jadpo check project=e-tr-locales-required file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-locales-required file=domains/shipping/translations/en_gb.jadpo expect=translation.locales_required
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```text output
error: Translations need the application's locales
 --> domains/shipping/translations/en_gb.jadpo:1:1
  |
1 | translations {
  | ^^^^^^^^^^^^ no locales declared
  reason: Each language file is for one of the locales the application supports, and each supported locale words every message. None is declared, so there is nothing to check the wording against.
  next: Declare them in `app.jadpo`, for instance `locales { default: Locale.en_gb supported: [Locale.en_gb] }` (agent)
  rule: translation.locales_required
  help: docs/language/10-configuration-and-time.html#translation-locales-required

check failed: 1 error, 3 files
```

### `translation.member_unknown`

A member of `translations { }` words one message, enum or email of the folder's scope, by its name in lower_snake_case.

```jadpo check project=e-tr-member-unknown file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-member-unknown file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-member-unknown file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```jadpo check project=e-tr-member-unknown file=domains/shipping/translations/fr_fr.jadpo hidden
translations {
    order_shipped: "La commande de {name} a été expédiée."
}
```

```jadpo check project=e-tr-member-unknown file=domains/shipping/translations/fr_fr.jadpo expect=translation.member_unknown
translations {
    order_shipped: "La commande de {name} a été expédiée."
    order_cancelled: "La commande est annulée."
}
```

```text output
error: `order_cancelled` words nothing
 --> domains/shipping/translations/fr_fr.jadpo:3:5
  |
3 |     order_cancelled: "La commande est annulée."
  |     ^^^^^^^^^^^^^^^ no such message, enum or email
  reason: A member of `translations { }` words one message, enum or email of this folder's scope, by its name in lower_snake_case, or a failure by its code. `order_cancelled` is none of them; the scope has `order_shipped`.
  next: Remove `order_cancelled` from `domains/shipping/translations/fr_fr.jadpo`, or declare the message it words beside the language files (agent)
  rule: translation.member_unknown
  help: docs/language/10-configuration-and-time.html#translation-member-unknown

check failed: 1 error, 4 files
```

### `translation.member_duplicate`

A member words one message once; a second would be dropped, or the first, whichever a reader of the file believed.

```jadpo check project=e-tr-member-duplicate file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-member-duplicate file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-member-duplicate file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```jadpo check project=e-tr-member-duplicate file=domains/shipping/translations/fr_fr.jadpo expect=translation.member_duplicate
translations {
    order_shipped: "La commande de {name} a été expédiée."
    order_shipped: "Expédiée."
}
```

```text output
error: `order_shipped` is written twice in `domains/shipping/translations/fr_fr.jadpo`
 --> domains/shipping/translations/fr_fr.jadpo:3:5
  |
2 |     order_shipped: "La commande de {name} a été expédiée."
  |     ------------- first written here
3 |     order_shipped: "Expédiée."
  |     ^^^^^^^^^^^^^ written again
  reason: A member words one message, enum or email, once; the second would be dropped or the first, whichever a reader of the file believed.
  next: Remove one `order_shipped` (agent)
  rule: translation.member_duplicate
  help: docs/language/10-configuration-and-time.html#translation-member-duplicate

check failed: 1 error, 4 files
```

### `translation.wording_shape`

How a message is worded follows its fields: a plural count needs a wording for each plural form, so a plain string is the wrong shape.

```jadpo check project=e-tr-wording-shape file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-wording-shape file=domains/shipping/translations/parcels.jadpo hidden
/// How many parcels an order has.
type Parcels = Message {
    count: PluralCount
}
```

```jadpo check project=e-tr-wording-shape file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    parcels: {
        one: "{count} parcel"
        other: "{count} parcels"
    }
}
```

```jadpo check project=e-tr-wording-shape file=domains/shipping/translations/fr_fr.jadpo hidden
translations {
    order_shipped: "La commande de {name} a été expédiée."
}
```

```jadpo check project=e-tr-wording-shape file=domains/shipping/translations/fr_fr.jadpo expect=translation.wording_shape
translations {
    parcels: "{count} colis"
}
```

```text output
error: `parcels` is a string, but needs one wording for each value or plural form
 --> domains/shipping/translations/fr_fr.jadpo:2:5
  |
2 |     parcels: "{count} colis"
  |     ^^^^^^^ needs a block with a wording for each plural form: `one`, `many` and `other`
  reason: How a message is worded follows its type: no choice, a plain string; one enum field, a wording for each value of the enum; one `PluralCount` field, a wording for each plural form of the language.
  next: Write `parcels` as a block with a wording for each plural form: `one`, `many` and `other` (agent)
  rule: translation.wording_shape
  help: docs/language/10-configuration-and-time.html#translation-wording-shape

check failed: 1 error, 4 files
```

### `translation.wording_invalid`

A wording is a plain string whose only syntax is `{field}`, and a literal brace is doubled; here a brace is never closed.

```jadpo check project=e-tr-wording-invalid file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-wording-invalid file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-wording-invalid file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```jadpo check project=e-tr-wording-invalid file=domains/shipping/translations/fr_fr.jadpo expect=translation.wording_invalid
translations {
    order_shipped: "La commande de {name a été expédiée."
}
```

```text output
error: This wording isn't a plain string with `{field}` placeholders
 --> domains/shipping/translations/fr_fr.jadpo:2:36
  |
2 |     order_shipped: "La commande de {name a été expédiée."
  |                                    ^^^^^ this brace is never closed
  reason: A `{` starts a field, `{field}`, and a literal brace is doubled: `{{` writes `{`.
  next: Close the field with `}`, or write `{{` for a literal brace (agent)
  rule: translation.wording_invalid
  help: docs/language/10-configuration-and-time.html#translation-wording-invalid

check failed: 1 error, 4 files
```

### `translation.placeholder_unknown`

A wording writes only the fields its message has.

```jadpo check project=e-tr-placeholder-unknown file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-placeholder-unknown file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-placeholder-unknown file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```jadpo check project=e-tr-placeholder-unknown file=domains/shipping/translations/fr_fr.jadpo expect=translation.placeholder_unknown
translations {
    order_shipped: "La commande de {nom} a été expédiée."
}
```

```text output
error: `{nom}` isn't a field of `OrderShipped`
 --> domains/shipping/translations/fr_fr.jadpo:2:36
  |
2 |     order_shipped: "La commande de {nom} a été expédiée."
  |                                    ^^^^^ no such field
  reason: `OrderShipped` has no field `nom` a wording can write; this wording may write `{name}`.
  next: Remove `{nom}`, or add the field to `OrderShipped` (agent)
  rule: translation.placeholder_unknown
  help: docs/language/10-configuration-and-time.html#translation-placeholder-unknown

check failed: 1 error, 4 files
```

### `translation.placeholder_style`

An amount of money is written with its style, `{price, number,
currency}`, so a translator sees it is a price, and a percentage with
its own, `{rate, number, percent}`; a style on a field of another kind
is the same mistake the other way round.

```jadpo check project=e-tr-placeholder-style file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.es_mx]
}
```

```jadpo check project=e-tr-placeholder-style file=domains/orders/types/order_amount.jadpo hidden
/// What an order costs, in the customer's currency.
type OrderAmount = Money {}
```

```jadpo check project=e-tr-placeholder-style file=domains/orders/translations/order_total.jadpo hidden
use orders/types { OrderAmount }

/// What an order comes to, on its confirmation page.
type OrderTotal = Message {
    total: OrderAmount
}
```

```jadpo check project=e-tr-placeholder-style file=domains/orders/translations/en_gb.jadpo hidden
translations {
    order_total: "Your order comes to {total, number, currency}."
}
```

```jadpo check project=e-tr-placeholder-style file=domains/orders/translations/es_mx.jadpo expect=translation.placeholder_style
translations {
    order_total: "Tu pedido suma {total}."
}
```

```text output
error: `{total}` is an amount of money, written `{total, number, currency}`
 --> domains/orders/translations/es_mx.jadpo:2:34
  |
2 |     order_total: "Tu pedido suma {total}."
  |                                  ^^^^^^^ no style
  reason: `OrderTotal.total` is money, and a wording writes an amount of money with its style, `{total, number, currency}`, so the translator sees it is one; it is shown as `text.format` shows it in the reader's locale, with its symbol and the currency's places.
  next: Write `{total, number, currency}` (agent)
  rule: translation.placeholder_style
  help: docs/language/10-configuration-and-time.html#translation-placeholder-style

check failed: 1 error, 5 files
```

### `translation.variant_unknown`

A language gives one wording for each plural form CLDR gives it. English has `one` and `other`, so `few` is a form it never uses.

```jadpo check project=e-tr-variant-unknown file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-variant-unknown file=domains/shipping/translations/parcels.jadpo hidden
/// How many parcels an order has.
type Parcels = Message {
    count: PluralCount
}
```

```jadpo check project=e-tr-variant-unknown file=domains/shipping/translations/fr_fr.jadpo hidden
translations {
    parcels: {
        one: "{count} colis"
        many: "{count} de colis"
        other: "{count} colis"
    }
}
```

```jadpo check project=e-tr-variant-unknown file=domains/shipping/translations/en_gb.jadpo expect=translation.variant_unknown
translations {
    parcels: {
        one: "{count} parcel"
        few: "{count} parcels, a few"
        other: "{count} parcels"
    }
}
```

```text output
error: `few` isn't a plural form of `Locale.en_gb`
 --> domains/shipping/translations/en_gb.jadpo:4:9
  |
4 |         few: "{count} parcels, a few"
  |         ^^^ no such wording to give
  reason: `Locale.en_gb` has no use for the plural form `few`: CLDR gives it `one` and `other`, so those are the forms a wording is written for.
  next: Remove `few` from `parcels`, or move its wording into one of the forms the language uses (agent)
  rule: translation.variant_unknown
  help: docs/language/10-configuration-and-time.html#translation-variant-unknown

check failed: 1 error, 4 files
```

### `translation.variant_missing`

French counts a million as `many`, so a wording for `many` is part of a French plural.

```jadpo check project=e-tr-variant-missing file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-variant-missing file=domains/shipping/translations/parcels.jadpo hidden
/// How many parcels an order has.
type Parcels = Message {
    count: PluralCount
}
```

```jadpo check project=e-tr-variant-missing file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    parcels: {
        one: "{count} parcel"
        other: "{count} parcels"
    }
}
```

```jadpo check project=e-tr-variant-missing file=domains/shipping/translations/fr_fr.jadpo expect=translation.variant_missing
translations {
    parcels: {
        one: "{count} colis"
        other: "{count} colis"
    }
}
```

```text output
error: `parcels` in `domains/shipping/translations/fr_fr.jadpo` has no wording for the plural form `many`
 --> domains/shipping/translations/fr_fr.jadpo:2:5
  |
2 |     parcels: {
  |     ^^^^^^^ no `many`
  reason: `Locale.fr_fr` needs a wording for each plural form CLDR gives it, so a count in any of them has words to show.
  next: Add a wording for `many` to `parcels`, each able to write `{count}` (agent)
  rule: translation.variant_missing
  help: docs/language/10-configuration-and-time.html#translation-variant-missing

check failed: 1 error, 4 files
```

### `translation.field_type`

A message's fields are the placeholders of its wording: text, numbers, amounts of money, a plural count, or an enum that picks the wording. A moment is formatted in code and passed in as `PresentationText`.

```jadpo check project=e-tr-field-type file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-field-type file=domains/shipping/translations/left_at.jadpo expect=translation.field_type
/// When a parcel left, on the tracking page.
type LeftAt = Message {
    when: Instant
}
```

```text output
error: `LeftAt.when` is typed `Instant`, which a wording can't write
 --> domains/shipping/translations/left_at.jadpo:3:5
  |
3 |     when: Instant
  |     ^^^^ not text, a number, money, a percentage or a choice
  reason: A message's fields are the placeholders of its wording, and a placeholder is text (`Text`, `PresentationText`), a number (`Int`, `Decimal`), an amount of money (`Money`, written `{field, number, currency}`), a percentage (`Percentage`, written `{field, number, percent}`) or a plural count (`PluralCount`), or a type built on one of them; one `enum` field picks which wording is written. Dates and times are formatted in code, with `text.format`, and passed in as `PresentationText`.
  next: Make `when` a `PresentationText` and format the value where the message is called (agent)
  rule: translation.field_type
  help: docs/language/10-configuration-and-time.html#translation-field-type

check failed: 1 error, 2 files
```

A failure's `client:` fields are placeholders too, filled by the client app, which formats a date, a time, a number or a duration for its own language. A choice has no such format.

```jadpo check project=e-tr-field-type-failure file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

```jadpo check project=e-tr-field-type-failure file=domains/shipping/types/hold_reason.jadpo hidden
type HoldReason = Enum {
    customs
    payment
}
```

```jadpo check project=e-tr-field-type-failure file=domains/shipping/translations/en_gb.jadpo hidden
translations {}
```

```jadpo check project=e-tr-field-type-failure file=domains/shipping/failures/parcel_held.jadpo expect=translation.field_type
use shipping/types { HoldReason }

type ParcelHeld = Failure {
    kind: FailureKind.rejected
    client: {
        reason: HoldReason
    }
}
```

```text output
error: `ParcelHeld.reason` is typed `HoldReason`, which a client can't write into a wording
 --> domains/shipping/failures/parcel_held.jadpo:6:9
  |
6 |         reason: HoldReason
  |         ^^^^^^ not text, a number, a date, a time or a duration
  reason: A failure's `client:` fields are the placeholders of its wording, and the client app fills each one: text as it is, and a number, a date, a time or a duration formatted for its own language. A choice, an object, a list or an optional value has no such format.
  next: Make `reason` text, or send the text in a `PresentationText` field (agent)
  rule: translation.field_type
  help: docs/language/10-configuration-and-time.html#translation-field-type

check failed: 1 error, 4 files
```

### `translation.too_many_selectors`

An enum field gives one wording for each of its values and a `PluralCount` field one for each plural form; together they would be a grid for a translator to fill.

```jadpo check project=e-tr-too-many-selectors file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-too-many-selectors file=domains/shipping/types/formality.jadpo hidden
type Formality = Enum {
    formal
    casual
}
```

```jadpo check project=e-tr-too-many-selectors file=domains/shipping/translations/combo.jadpo expect=translation.too_many_selectors
use shipping/types { Formality }

/// A combo of wordings.
type Combo = Message {
    formality: Formality
    count: PluralCount
}
```

```text output
error: `Combo` has two fields that choose its wording, `formality` and `count`
 --> domains/shipping/translations/combo.jadpo:6:5
  |
5 |     formality: Formality
  |     --------- the first
6 |     count: PluralCount
  |     ^^^^^ a second choice
  reason: An enum field gives one wording for each of its values, and a `PluralCount` field one for each plural form of the language. Both at once would need a wording for every pairing of a value and a form, which a translator would have to fill in.
  next: Split `Combo` into two messages, one for each choice, and call both (agent)
  rule: translation.too_many_selectors
  help: docs/language/10-configuration-and-time.html#translation-too-many-selectors

check failed: 1 error, 3 files
```

### `translation.field_unused`

A call gives every field a value, so a language that never writes one is either right (it needs no such word) or has forgotten a placeholder. It is a judgement call: a person answers it with a reason.

```jadpo check project=e-tr-field-unused file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-field-unused file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-field-unused file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```jadpo check project=e-tr-field-unused file=domains/shipping/translations/fr_fr.jadpo expect=translation.field_unused
translations {
    order_shipped: "La commande a été expédiée."
}
```

```text output
error: No wording of `order_shipped` writes `{name}`
 --> domains/shipping/translations/fr_fr.jadpo:2:5
  |
2 |     order_shipped: "La commande a été expédiée."
  |     ^^^^^^^^^^^^^ `{name}` is never written
  reason: `OrderShipped` has a field `name`, and a call gives it a value, but this language never writes it. That is right for a language that needs no such word, and it is also what a forgotten placeholder looks like.
  next: Write `{name}` in a wording of `order_shipped`, or remove the field, or write `decline translation.field_unused: "<why>"` above it if this is deliberate (agent)
  rule: translation.field_unused
  help: docs/language/10-configuration-and-time.html#translation-field-unused

check failed: 1 error, 4 files
```

### `translation.message_unknown`

A call names a message by its domain and its name; here the name is misspelt.

```jadpo check project=e-tr-message-unknown file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-message-unknown file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-message-unknown file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```jadpo check project=e-tr-message-unknown file=domains/shipping/translations/fr_fr.jadpo hidden
translations {
    order_shipped: "La commande de {name} a été expédiée."
}
```

```jadpo check project=e-tr-message-unknown file=domains/shipping/types/shipping_text.jadpo hidden
/// What a customer is told.
type ShippingText = Output {
    subject: PresentationText
}
```

```jadpo check project=e-tr-message-unknown file=domains/shipping/actions/describe_order.jadpo expect=translation.message_unknown
use shipping/types { ShippingText }

/// What a customer is told about their order.
action describe_order(reader_locale: Locale) -> ShippingText {
    return ShippingText {
        subject: translations.shipping.order_shiped(name: "Ana", locale: reader_locale)
    }
}
```

```text output
error: `translations.shipping.order_shiped` is not a message
 --> domains/shipping/actions/describe_order.jadpo:6:18
  |
6 |         subject: translations.shipping.order_shiped(name: "Ana", locale: reader_locale)
  |                  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ no such message
  reason: `order_shiped` is not a message or enum `shipping` words. Did you mean `order_shipped`?
  next: Use `translations.shipping.order_shipped` (agent)
  rule: translation.message_unknown
  help: docs/language/10-configuration-and-time.html#translation-message-unknown

check failed: 1 error, 6 files
```

### `translation.jadpo_missing`

Jadpo ships its own wording in `en_gb` and `en_us`. An application that supports another locale words the messages of the features it has on itself, in the root `translations/<locale>.jadpo` under `jadpo: { }`, so the users and client apps using it are not shown English; here the notes app has a route and French has no root file.

```jadpo check project=e-tr-jadpo-missing file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-jadpo-missing file=api/main.jadpo_record hidden
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

POST /notes
    authentication none
    input NoteRequest (body)
    output NoteShown
    status 200

GET /openapi.json
    generated openapi
    authentication none
    status 200

input NoteRequest
    text: Text { max_length: 200 }

output NoteShown
    text: Text { max_length: 200 }
```

```jadpo check project=e-tr-jadpo-missing file=policy.jadpo hidden
use notes

policy {
    actions: {
        notes.echo_note: [Access.public]
    }
}
```

```jadpo check project=e-tr-jadpo-missing file=domains/notes/types/note_text.jadpo hidden
/// A note's text.
type NoteText = Text {
    max_length: 200
}
```

```jadpo check project=e-tr-jadpo-missing file=domains/notes/types/note_request.jadpo hidden
use notes/types { NoteText }

/// A note to show.
type NoteRequest = Input {
    text: NoteText
}
```

```jadpo check project=e-tr-jadpo-missing file=domains/notes/types/note_shown.jadpo hidden
use notes/types { NoteText }

/// A note shown.
type NoteShown = Output {
    text: NoteText
}
```

```jadpo check project=e-tr-jadpo-missing file=domains/notes/actions/echo_note.jadpo hidden
use notes/types { NoteRequest, NoteShown }

/// Gives a note back.
action echo_note(input: NoteRequest) -> NoteShown {
    return NoteShown {
        text: input.text
    }
}
```

```jadpo check project=e-tr-jadpo-missing file=domains/notes/routes/notes.jadpo expect=translation.jadpo_missing
use notes/types { NoteRequest, NoteShown }
use notes/actions { echo_note }

routes Notes {
    prefix: /notes

    POST / {
        authentication: RouteAuthentication.none
        input: NoteRequest
        output: NoteShown
        run: echo_note(input)
    }
}
```

```text output
error: `Locale.fr_fr` is supported but Jadpo's own messages aren't worded in it
 --> app.jadpo:5:31
  |
5 |     supported: [Locale.en_gb, Locale.fr_fr]
  |                               ^^^^^^^^^^^^ no `translations/fr_fr.jadpo`
  reason: Jadpo ships its own wording in `en_gb` and `en_us` and no other locale, so users and client apps working in `Locale.fr_fr` would be shown English. An application words the messages of the features it has on itself, in the `jadpo: { }` group of the root `translations/fr_fr.jadpo`. Still to word: `invalid_request`, `invalid_input`, `route_not_found`, `method_not_allowed`, `request_too_large`, `idempotency_key_reused`, `rate_limited`, `unavailable`, `internal_fault` and `deadline_exceeded`.
  next: Run `jadpo translations builtin` to list them with their fields, then word each under `jadpo: { }` in `translations/fr_fr.jadpo` (agent)
  rule: translation.jadpo_missing
  help: docs/language/10-configuration-and-time.html#translation-jadpo-missing

check failed: 1 error, 7 files
```

### `translation.feature_off`

Jadpo writes the messages of a feature only for an application that has it, so wording one of a feature that is off changes nothing and would hide a mistake. Here the application signs no one in with magic links.

```jadpo check project=e-tr-feature-off file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

```jadpo check project=e-tr-feature-off file=translations/en_gb.jadpo expect=translation.feature_off
translations {
    jadpo: {
        magic_link_email: "Your link to sign in"
    }
}
```

```text output
error: `magic_link_email` is a message of a feature this application has off
 --> translations/en_gb.jadpo:3:9
  |
3 |         magic_link_email: "Your link to sign in"
  |         ^^^^^^^^^^^^^^^^ a feature this application has off
  reason: `magic_link_email` is part of Jadpo's magic links text, which Jadpo writes when the application signs users in with magic links. This application doesn't, so wording it changes nothing and would hide a mistake.
  next: Remove `magic_link_email` from `translations/en_gb.jadpo`, or switch the feature on (agent)
  rule: translation.feature_off
  help: docs/language/10-configuration-and-time.html#translation-feature-off

check failed: 1 error, 2 files
```

### `translation.group_unknown`

The root file groups its wording by whose text it is: `jadpo` for Jadpo's own messages, or a library package's folder name (`postcodes`) for that package's. A group that names neither words nothing.

```jadpo check project=e-tr-group-unknown file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

```jadpo check project=e-tr-group-unknown file=translations/en_gb.jadpo expect=translation.group_unknown
translations {
    jadp: {
        text_direction: {
            left_to_right: "ltr"
            right_to_left: "rtl"
        }
    }
}
```

```text output
error: `jadp` isn't a group Jadpo reads
 --> translations/en_gb.jadpo:2:5
  |
2 |     jadp: {
  |     ^^^^ no such group
  reason: The root file words the text the application did not write, grouped by whose it is: `jadpo` for Jadpo's own messages, or the folder name of a library package the application uses for that package's. Did you mean `jadpo`?
  next: Rename the group to `jadpo` (agent)
  rule: translation.group_unknown
  help: docs/language/10-configuration-and-time.html#translation-group-unknown

check failed: 1 error, 2 files
```

### `translation.stale`

A translation is made from the default locale's wording, and `jadpo translations update` records which. When the default wording changes the translation may no longer say the same thing, so it is an error until it is translated again; a locale listed in `in_progress:` is listed by `jadpo translations missing` instead.

```jadpo check project=e-tr-stale file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.de_de]
}
```

```jadpo check project=e-tr-stale file=domains/greeting/translations/hello.jadpo hidden
/// Says hello.
type Hello = Message {
    name: Text
}
```

```jadpo check project=e-tr-stale file=domains/greeting/translations/en_gb.jadpo hidden
translations {
    hello: "Good day, {name}."
}
```

```jadpo check project=e-tr-stale file=domains/greeting/translations/sources.jadpo_record hidden
// Generated by Jadpo from this folder's translations; update it with `jadpo translations update`, never by hand.
record translations 1

translation de_de.hello from 2e5b444cecc5 wording acb00cba6dc3
```

```jadpo check project=e-tr-stale file=domains/greeting/translations/de_de.jadpo expect=translation.stale
translations {
    hello: "Hallo, {name}."
}
```

```text output
error: `hello` in `de_de` was translated from `en_gb` wording that has since changed
 --> domains/greeting/translations/de_de.jadpo:2:5
  |
2 |     hello: "Hallo, {name}."
  |     ^^^^^ made from the old `en_gb` wording
  reason: `jadpo translations update` recorded the `en_gb` wording this message's translation was made from, and the `en_gb` wording has changed since, so the translation may no longer say the same thing.
  next: Reword `hello` in `domains/greeting/translations/de_de.jadpo` to match the new `en_gb` wording, then run `jadpo translations update` (agent)
  rule: translation.stale
  help: docs/language/10-configuration-and-time.html#translation-stale

check failed: 1 error, 4 files
```

### `translation.sources_invalid`

The record in a `translations/` folder says which wording each translation was made from and who accepted it. Jadpo writes it, and a stale or unaccepted translation is judged from it, so a file Jadpo can't read hides both.

```jadpo check project=e-tr-sources-invalid file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.de_de]
}
```

```jadpo check project=e-tr-sources-invalid file=domains/greeting/translations/hello.jadpo hidden
/// Says hello.
type Hello = Message {
    name: Text
}
```

```jadpo check project=e-tr-sources-invalid file=domains/greeting/translations/en_gb.jadpo hidden
translations {
    hello: "Hello, {name}."
}
```

```jadpo check project=e-tr-sources-invalid file=domains/greeting/translations/de_de.jadpo hidden
translations {
    hello: "Hallo, {name}."
}
```

```jadpo check project=e-tr-sources-invalid file=domains/greeting/translations/sources.jadpo_record expect=translation.sources_invalid
// Generated by Jadpo from this folder's translations; update it with `jadpo translations update`, never by hand.
record translations 1

translation de_de.hello was made from Hello, {name}.
```

```text output
error: `domains/greeting/translations/sources.jadpo_record` can't be read: this line is not a translation entry
 --> domains/greeting/translations/sources.jadpo_record:4:1
  |
4 | translation de_de.hello was made from Hello, {name}.
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ this line is not a translation entry
  reason: This file records which wording each translation was made from and who accepted it. Jadpo writes it, and a stale or unaccepted translation is judged from it, so a file it can't read hides both.
  next: Run `jadpo translations update`, which writes it again (agent)
  rule: translation.sources_invalid
  help: docs/language/10-configuration-and-time.html#translation-sources-invalid

check failed: 1 error, 4 files
```

### `translation.option_unknown`

`translations:` in `application { }` takes `review:` and nothing else.

```jadpo check project=e-tr-option-unknown file=app.jadpo expect=translation.option_unknown
application {
    translations: {
        reviews: TranslationReview.required
    }
}
```

```text output
error: `reviews` isn't an option of `translations`
 --> app.jadpo:3:9
  |
3 |         reviews: TranslationReview.required
  |         ^^^^^^^ unknown option
  reason: `translations:` in `application` takes `review:` and nothing else. Did you mean `review`?
  next: Use `review` instead of `reviews` (agent)
  rule: translation.option_unknown
  help: docs/language/10-configuration-and-time.html#translation-option-unknown

check failed: 1 error, 1 file
```

### `translation.review_invalid`

`review:` says whether wording outside the default locale waits for a person to accept it: `TranslationReview.none`, the default, or `TranslationReview.required`.

```jadpo check project=e-tr-review-invalid file=app.jadpo expect=translation.review_invalid
application {
    translations: {
        review: TranslationReview.always
    }
}
```

```text output
error: `review:` isn't a `TranslationReview`
 --> app.jadpo:3:17
  |
3 |         review: TranslationReview.always
  |                 ^^^^^^^^^^^^^^^^^^^^^^^^ not `TranslationReview.none` or `.required`
  reason: `review:` says whether wording outside the default locale waits for a person to accept it: `TranslationReview.none`, the default, or `TranslationReview.required`.
  next: Write `review: TranslationReview.required`, or leave `review:` out (agent)
  rule: translation.review_invalid
  help: docs/language/10-configuration-and-time.html#translation-review-invalid

check failed: 1 error, 1 file
```

### `type.locale_in_progress_unsupported`

`in_progress:` names supported locales whose wording isn't finished; a locale that isn't supported isn't written at all.

```jadpo check project=e-locale-in-progress-unsupported file=app.jadpo expect=type.locale_in_progress_unsupported
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
    in_progress: [Locale.de_de]
}
```

```text output
error: `Locale.de_de` is in progress but isn't supported
 --> app.jadpo:6:19
  |
5 |     supported: [Locale.en_gb, Locale.fr_fr]
  |                ---------------------------- the supported locales
6 |     in_progress: [Locale.de_de]
  |                   ^^^^^^^^^^^^ not in `supported:`
  reason: `in_progress:` names supported locales whose wording isn't finished: it lets them fall back where wording is missing. A locale that isn't supported isn't written at all.
  next: Add `Locale.de_de` to `supported:`, or remove it from `in_progress:` (agent)
  rule: type.locale_in_progress_unsupported
  help: docs/language/10-configuration-and-time.html#type-locale-in-progress-unsupported

check failed: 1 error, 1 file
```

### `type.locale_in_progress_default`

An in-progress locale falls back to the default where wording is missing, so the default is worded in full.

```jadpo check project=e-locale-in-progress-default file=app.jadpo expect=type.locale_in_progress_default
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
    in_progress: [Locale.en_gb]
}
```

```text output
error: The default `Locale.en_gb` can't be in progress
 --> app.jadpo:6:19
  |
4 |     default: Locale.en_gb
  |              ------------ the default
6 |     in_progress: [Locale.en_gb]
  |                   ^^^^^^^^^^^^ the default locale
  reason: An in-progress locale falls back to the default where wording is missing, so the default is worded in full, and the last place a missing wording can come from.
  next: Remove `Locale.en_gb` from `in_progress:`, or choose another default (agent)
  rule: type.locale_in_progress_default
  help: docs/language/10-configuration-and-time.html#type-locale-in-progress-default

check failed: 1 error, 1 file
```

## Review

### Status

The new compiler has built the time literals and the built-in `Zone`,
`Weekday` and `Locale` enums (chunk 1.9), and reports `clock.now` outside an
action as `type.clock_context`. Settings, secrets, signing keys, connection
addresses, `jadpo config`, the check before the app starts and a test's own
values were built in chunk 5.1 (the readings it chose are in the Decisions
table), and the trusted proxies a client's address is read through in chunk
5.1c. Chunk 5.2 built time: `ZonedTime` in place of the time of day the
first build had, durations, arithmetic and comparison, the `temporal`
library over the IANA database bundled into every target, time values on
the wire, in OpenAPI and in queries, and a test's `clock:` and
`clock.advance(...)` (area 13); its readings are in the Decisions table.
Chunk 5.2b built `TimeFormat`, declared and built in, `ComponentStyle` and
`PresentationText`, and checks `temporal.format` and `format_relative`; its
readings are in the Decisions table too. Chunk 5.3 built locales
(`locales { }` with its errors, `locale:` checked against it, a `Locale` on
the wire or in a setting limited to the supported ones) and the text
`temporal.format` and `format_relative` give, over CLDR data compiled into
every target. Chunk 5.3c built `locale.from_accept_language(header)`,
which chooses a supported `Locale` from an `Accept-Language` header and falls
back to `default:`. Chunk 5.4c built translations as messages, as decided
on 2026-10-16: `Message` types and the built-in `PluralCount`,
each domain's `translations { }` language files and their checks, the
generated `translations.<domain>.<message>` calls, enum wording, email
subjects and `in_progress:` locales (see Translations as messages). Chunk
5.4d built failures' client text exported per locale, stale translations
and review, `locale.direction`, the commands `missing`, `unused`, `update`
and `accept`, go to definition and the pseudo-locale. Chunk 5.4e built
Jadpo's own messages, the root file's owner groups and `jadpo translations
builtin`. Chunk 5.4f built `jadpo translations export` and `import`.
Chunk 7.1b3 built library packages' translations: a package's
`translations/`, `Postcodes.translations.<message>(...)`, the root file's
package groups and their record lines, and a package's messages in its
surface and the publishing bar. Chunk 8.2c built the locale data a
deployable carries, its supported locales' only, and the text a `Locale`
the app doesn't support is written in (see Locales).

### Consistency with the rest of the language

- **Names that repeat a name.** The config declaration's name and every
  `binding:` restate a name already present (syntax review #2, #55); area 1
  dropped singleton names, and area 4 derived failure codes the same way.
- **Two duration spellings** (`5m` in configuration, `Duration("PT5M")` in
  code), and each place rejects the other (syntax review #56).
- **Locales written as text** in their declaration but as `Locale.en_gb` in
  code (syntax review #57), against area 2's rule that closed options are
  names. Fixed by question 8: `locales { }` lists `Locale.en_gb`, and a
  tag in quotes is `type.locale_invalid` with the one-edit fix.
- **An anonymous object as an argument.** `components: { day: numeric }`
  breaks area 2's decision that there are no anonymous objects (syntax
  review #58).
- **One-value settings.** `profile: FriendlyTimeFormat.conversational` has
  only one possible value, the kind of ritual line the syntax review removes
  elsewhere (§4).
- **`secret` means five things** (syntax review #46): the config flag, an
  authentication key, an integration credential, a test fixture wrapper and a
  design-only setting. Keep it for the config flag only (areas 8, 11, 13).
- **Config fields escape the named-type rule.** `oidc_audience: Text`
  compiles, although area 2 requires named types in signatures. A setting
  read into an action is a signature in all but name; recommend the same rule
  (`type OidcAudience = Text { }`).
- **Policy options in code are qualified** (`overlap: LocalOverlap.reject`),
  consistent with area 2's rule that every option is written `Type.value`,
  in settings and in code (syntax review #59).

### Carried to later areas

| Item | Area | Status |
|---|---|---|
| Fixture configuration values, the fixture clock and `advance clock by` (syntax review proposes `clock.advance(5m)` and plain fixture values); `configuration.fixture_*` and `type.test_clock_context` (a test's own `config:` is built, chunk 5.1; `configuration.field_unknown` is area 10's now) | 13 | Input to area 13 |
| `jadpo config set`, `config check`, `dev` restarts, `configuration.value_invalid`, `configuration.value_missing`, `configuration.local_*` and `cli.config_*`, none of which `jadpo check` raises | 14 | Built (chunk 5.1) |
| The startup report of a deployable (`runtime.startup_failed`), the generated app refusing to start | 14 | Built in chunk 8.1o: a native deployable logs `runtime.startup_failed` naming each setting and its variable, never the value, and ends (area 14, "A native deployable") |
| Health and readiness, including the generated `health { }` proposal (syntax review #65) and `semantic.service_configuration_missing` | 11 | Input to area 11 |
| Integration credentials and authentication signing keys as the only secret sinks; their setting names (`token:`, `signing_key:`) | 8, 11 | Input to areas 8 and 11 |
| Mail templates using translations and `PresentationText` | 11 | Input to area 11 (Superseded 2026-10-16: templates read `translations.` and are checked strictly; see area 16 question 11.) |
| A library package's own messages, and whether an app can reword them | 11 | Built in chunk 7.1b3 ("Jadpo's and packages' text") |
| `config.typo` should report the configuration field | 10 | Fixed (chunk 5.1): `configuration.field_unknown` |
| `jadpo fmt` writes `temporal.format (` with a space | 14 | Bug |
| Trusted proxies: which proxies a deployment trusts to say a client's address (`X-Forwarded-For`, `Forwarded`, Cloudflare's `CF-Connecting-IP`) | 7, 10, 16 | Built (chunk 5.1c, question 12); a source other than `cloudflare` refused for Workers comes with chunk 8.2m |
| GDPR: retention periods as settings, alongside personal-data marking, erasure and export | 15 | Moved to area 15 |

### Open questions

1. **Should the environment variable name come from the field name?** Every
   binding in the repository is the field name in capitals. Deriving it
   removes a line per field; the risk is that renaming a field silently
   renames the variable. *Recommendation:* derive it (`mail_sender` reads
   `MAIL_SENDER`), keep `binding:` only to differ, and show a changed variable
   name in the approval review as a deployment change, through the shared
   "accept a reviewed change" mechanism carried to area 7. Rails credentials
   and Spring Boot's relaxed binding derive names the same way. *Decided 2026-10-12:* yes, as recommended.
2. **Which types can a setting have?** Today text-based, number, boolean,
   UUID, email, URL, IP, duration, instant and date types. *Recommendation:*
   add enums, written `Type.value`, and `Zone` and `Locale`, so a log level or
   business time zone is checked at startup instead of parsed by hand; keep
   lists and objects out (one variable, one value). *Decided 2026-10-12:* yes, as recommended.
3. **How is a database address typed?** `Url` cannot hold `postgres://`, so
   area 5's `url: config.database_url` cannot work. *Recommendation:*
   built-in `PostgresUrl` and `RedisUrl`, each checking its schemes, always
   secret, with a store's `url:` required to match its `engine:`; the runtime
   connects only through it. Also make the startup and `config check`
   validators the same code, so they cannot disagree again. *Decided 2026-10-12:* yes, as recommended. *Built in chunk 5.1* (the Decisions table says how).
4. **One duration spelling, and does `d` stay?** *Recommendation:* `500ms`,
   `30s`, `5m`, `2h` and `30d` everywhere in source, including constraints
   (`max: 30s`), with ISO 8601 only in JSON. Keep `d` as exactly 24 hours,
   because retention and token lifetimes are elapsed time and `720h` is
   unreadable; calendar days stay a date operation. Go has no `d` and
   Rust's `humantime` has it; no `w`, months or years, which vary. *Decided 2026-10-12:* yes, as recommended. *Built in chunk 5.2.*
5. **Rename `Time` to `ZonedTime`?** Java, Temporal and Elixir all use "time"
   for a time of day, so `Time` will be misread by people and agents.
   *Recommendation:* yes, `ZonedTime` (Temporal says `ZonedDateTime`, jiff
   `Zoned`), with the wire form unchanged. *Decided 2026-10-12:* yes, as recommended. *Built in chunk 5.2*, replacing the time of day the first build had called `Time`.
6. **Does adding a duration keep the instant's named type?** Today
   `due_at - notice` is a plain `Instant` and must be rebuilt.
   *Recommendation:* yes for `+` and `-` with a `Duration`, the one mixed
   case area 3's operator rule did not cover; `Instant - Instant` gives
   `Duration`. *Decided 2026-10-12:* yes, as recommended. *Built in chunk 5.2.*
7. **Should the time library and formatting be simplified?** About 35
   functions, six option enums, two daylight-saving options on every call, a
   two-step format and an anonymous components object.
   *Recommendation:* Option B in the
   [time and formatting proposals](research/10-configuration-and-time-proposals.md):
   `temporal.format(value, zone:, locale:, format:)` with named built-in or
   declared formats, `format_relative` without a profile, one `dst:` option
   that defaults to failing, and about 20 functions. *Decided 2026-10-12:* yes, as recommended. *Built in chunks 5.2, 5.2b and 5.3.*
8. **Locales as names, and drop `unsupported:`?** *Recommendation:* yes,
   `default: Locale.en_gb` and `supported: [Locale.en_gb, Locale.fr_fr]`, in
   `app.jadpo`, with `Locale` a built-in enum of the standard locales. An
   unsupported tag in a request is an ordinary `invalid_value` like any
   enum; header negotiation is a library call that falls back to `default`. *Decided 2026-10-12:* yes, as recommended. *Built in chunk 5.3*; the library call, which the page left without a name, was named `locale.from_accept_language(header)` on 2026-10-16 and *built in chunk 5.3c*.
9. **What format does `translations/` use?** *Recommendation:* one JSON file
   per supported locale with ICU MessageFormat messages (what FormatJS, Java
   and translation services such as Crowdin and Lokalise use), typed calls
   generated from the default locale's keys, and missing keys or
   placeholders in other locales as compile errors. Build it when the first
   email template needs it (area 11), not before. *Decided 2026-10-12:* yes, as recommended.
   *Amended 2026-10-13 (Richard):* translations are supported out of the box
   from the first build, not deferred: mail, the generated privacy pages,
   client failure messages and any generated page use them. *Built in chunk 5.3b* as JSON files, since replaced.
   *Revised 2026-10-16 (Richard):* JSON and ICU MessageFormat are dropped,
   and the default locale's file no longer defines the keys. Each message
   is a `type X = Message` in its domain's `translations/` folder, its
   fields the placeholders; each language is an unnamed `translations { }`
   in a file named after its locale, whose wordings are plain strings with
   `{field}` placeholders, plural forms from CLDR for a `PluralCount` field and
   one wording per value for an enum field. JSON has no comments and
   quotes everything, and the compiler parses only Jadpo (area 11). A
   failure's client text is keyed by its code in the same files and
   exported per locale for client apps, which resolves this question's
   "client failure messages" against area 4 question 12. See
   [Translations as messages](#translations-as-messages) for the whole design;
   built in chunk 5.4c, which took 5.3b's JSON files out; failures' client
   text and the export in chunk 5.4d.
10. **How are signing keys rotated?** (carried from area 8) Area 8 keeps
    signing keys, the OIDC issuer and audience in configuration. How does a
    deployment replace a signing key without signing everyone out?
    *Decided 2026-10-12:* a built-in `SigningKey` type. `session_signing_key`
    reads `SESSION_SIGNING_KEY` and an optional `SESSION_SIGNING_KEY_PREVIOUS`;
    new tokens are signed with the current key and both are accepted. Rotate
    by setting the new key and moving the old one to `_PREVIOUS`, then remove
    it once the longest token lifetime has passed. *Built in chunk 5.1* as
    far as reading and checking the two values, and signing with them in
    chunk 5.1b (area 8).
11. **Do configuration fields carry `test:` values?** (carried from area 13)
    Area 13 has tests take each configuration field's `test:` value, while
    "Values on your machine and in production" above says tests get their
    values from fixtures. Which does configuration declare?
    *Decided 2026-10-12:* a field may carry a `test:` value in `config.jadpo`;
    secrets need none, because `jadpo test` generates random ones; a single
    test can override a value. *Built in chunk 5.1.*
12. **How does a deployment say which proxies to trust for a client's
    address?** (raised in chunk 5.1; areas 7 and 16 count requests by
    address, and no page says how the address is read.) Behind a load balancer
    the connection's address is the proxy's, so every client shares one
    rate-limit allowance until the real address is read from the forwarding
    header; reading it from a header anyone can send lets a caller choose
    its own address. Neither area 7, 10 nor 16 decides it. *Recommendation:*
    declare it once in `app.jadpo`, beside the connections, as
    `client_address: { source: ClientAddressSource.forwarded_for hops:
    config.proxy_hops }`, where `ClientAddressSource` is a closed set of
    four: `connection` (the default natively: the connection's own address,
    no header read), `forwarded_for` (`X-Forwarded-For`), `forwarded`
    (RFC 7239's `Forwarded: for=`) and `cloudflare` (`CF-Connecting-IP`),
    and `hops` is how many proxies of the deployment's own sit in front of
    the app, an `Int` setting (a list of addresses cannot be a setting, and a
    count cannot be spoofed the way an allow-list of ranges can drift). With
    `hops: N` the client is the N-th address from the right of the header's
    addresses, all its lines joined in order, since each trusted proxy
    appends the address it saw on its right and whatever a caller adds sits to
    the left; fewer than N addresses, or one that is not an address, falls
    back to the connection's own, never to a guess. On Workers there is no
    connection address to fall back to, and Cloudflare sets `CF-Connecting-IP`
    on every request from the edge and overwrites a caller's, so `cloudflare`
    is the only source accepted there (and its default); `hops` means nothing
    with it. The address chosen is the one rate limits count, `jadpo dev`'s
    request lines show and the request timeline records (areas 16 and 17).
    *Decided 2026-10-16:* yes, as recommended. *Built in chunk 5.1c.*

### Decisions

| Topic | Decision | Date |
|---|---|---|
| Unnamed declarations | Carried from area 1: `config { }` and `locales { }` carry no name | 2026-10-12 |
| Root files | Carried from area 1: settings live in `config.jadpo`; there is no `env.jadpo`; values come from the platform environment and one `.env.local`, with secrets entered through `jadpo config set` | 2026-10-12 |
| Translations folder | Carried from area 1: `translations/` is a fixed root folder; its format is decided here (Superseded 2026-10-16: translations live in each domain's `translations/` folder; the root folder holds only the app's wording for Jadpo's own messages.) | 2026-10-12 |
| Failure messages | Carried from area 4: clients translate by failure `code`; `message` stays fixed English | 2026-10-12 |
| Store addresses | Carried from area 5: stores are declared in `app.jadpo` and take `url: config.<field>` | 2026-10-12 |
| Signature types | Carried from area 2: signatures use named types built on `Instant`, `CalendarDate`, `Time` and `Duration`; `Zone`, `Locale` and `Weekday` may be used directly | 2026-10-12 |
| Option names | Carried from area 2: closed options in settings are lower_snake_case names; in code, enum values are qualified (Superseded 2026-10-12: options are written `Type.value`; see area 2 question 17.) | 2026-10-12 |
| Environment variable names | Derived from the field name (`mail_sender` reads `MAIL_SENDER`); `binding:` only to differ; a changed name shows in the approval review | 2026-10-12 |
| Setting types | Adds enums, `Zone` and `Locale`; no lists or objects | 2026-10-12 |
| Database address type | Built-in `PostgresUrl` and `RedisUrl`, always secret, matching the store's engine; startup and `config check` share one validator | 2026-10-12 |
| Duration spelling | `500ms`, `30s`, `5m`, `2h`, `30d` everywhere in source, `d` exactly 24 hours; ISO 8601 only in JSON | 2026-10-12 |
| `Time` name | Renamed `ZonedTime` | 2026-10-12 |
| Instant arithmetic | Adding or subtracting a `Duration` keeps the named type; `Instant - Instant` gives `Duration` | 2026-10-12 |
| Time library and formatting | Option B: `temporal.format(value, zone:, locale:, format:)` with named formats, one `dst:` option failing by default, about 20 functions | 2026-10-12 |
| Locales declaration | `locales: { default: Locale.en_gb supported: [...] }` in `app.jadpo`; `Locale` built in; no `unsupported:` | 2026-10-12 |
| Locales in progress | `in_progress: [...]` in `locales:` lists supported locales allowed to fall back where wording is missing, and one not in `supported:` is an error; the rest must be complete (Richard). Revised 2026-10-17 for area 1, question 11 (no warnings): listing a locale is the explicit, declared opt-out, so it falls back with no diagnostic and `jadpo translations missing` lists the gaps | 2026-10-16 |
| Failure export shape | `<api>.failures.<locale>.json` maps each code to `{ "text": ..., "formats": { placeholder: format } }`, the same shape for every code; `formats` lists only placeholders the client formats, as `number`, `decimal`, `date`, `date_time` or `duration`, decided by the field's type (`CalendarDate` date, `Instant` date_time in the reader's zone, `Time` date_time in its own zone, `Duration` duration, `Int` number, `Decimal` decimal); a type the client cannot format is an error (decided for Richard) | 2026-10-16 |
| Translations format | One JSON file per locale with ICU MessageFormat; typed calls from the default locale's keys; missing keys or placeholders are compile errors; built with the first email template (Superseded 2026-10-13 and 2026-10-16: built from the start; the format is under Translations, above.) | 2026-10-12 |
| Signing key rotation | Built-in `SigningKey` reading `NAME` and optional `NAME_PREVIOUS`; sign with current, accept both | 2026-10-12 |
| Test values for settings | `test:` on the field; random secrets generated by `jadpo test`; per-test override | 2026-10-12 |
| Translations from the start | Built into the first build, used by mail, generated pages and client failure messages (Richard) | 2026-10-13 |
| Where translations live | In the domain that uses them, `domains/<domain>/translations/`; a message a second domain uses moves to `shared/translations/` under area 1's promotion rule; the root `translations/` holds only the app's wording for Jadpo's own messages (Richard) | 2026-10-16 |
| Messages | `type X = Message { ... }`, a type base beside `Failure`, `Event` and `EmailMessage`, one per file; fields are placeholders; `///` descriptions are translator context (Richard) | 2026-10-16 |
| Language files | One unnamed `translations { }` per locale in a file named after it (`de_de.jadpo`); members are message, failure, enum or email names in lower_snake_case; every wording a plain string whose only syntax is `{field}`, a literal brace doubled (`{{`, `}}`); no grouping or nesting beyond variants and plural forms; JSON and ICU MessageFormat dropped (Richard) | 2026-10-16 |
| Mapping a message to wording | No fields: a string; `Text`, `PresentationText`, `Int`, `Decimal`: placeholders, numbers written the locale's way; one enum field: one wording per value; one `PluralCount` field: one wording per CLDR plural form; any other field type, or more than one enum or `PluralCount` field, an error; a placeholder that is not a field an error; a field no wording uses a warning (Richard). Revised 2026-10-17 for area 1, question 11: a declinable error, `translation.field_unused`. Revised by area 19, question 16 (built in chunk 5.5h2): a field built on `Money` is a placeholder written `{price, number, currency}`, the one style, and `translation.placeholder_style` refuses it written otherwise. Revised by the same question (built in chunk 5.5j2): a field built on `Percentage` is written `{rate, number, percent}`, the second style | 2026-10-16 |
| Generated calls | `translations.<domain>.<message>(field: value, ..., locale: ...)`, every field by name, returning `PresentationText`; `translations.shared.<message>` for `shared/translations/`; in email templates the email's locale (Richard) | 2026-10-16 |
| Email subjects | An `EmailMessage`'s member in its domain's language files is its subject line, its fields usable as placeholders; the body is the `.html` and `.txt` templates (Richard) | 2026-10-16 |
| Enum wording | An enum can be written out once its domain's language files give each value a wording, a value's fields usable as placeholders; area 2's "no display labels" stands (Richard) | 2026-10-16 |
| Failures' client text | Keyed by the failure's code in its domain's language files, client fields as placeholders; `jadpo build` writes `<api>.failures.<locale>.json` beside each API's OpenAPI document, from code to wording in `{field}` syntax (revised the same day: with each placeholder's formatting); the server's `message` stays English (Richard) | 2026-10-16 |
| Translation checks | Every supported locale is complete: missing wording for a message, a failure that can reach a response or an enum is an error whose `next:` names the file, member and placeholders; only a locale in `locales: { in_progress: [...] }` falls back, with no diagnostic (revised 2026-10-17 for area 1, question 11), regional sibling first (`fr_ca` to `fr_fr`), then the default; stale translations are an error (chunk 5.4d, as a warning once, revised 2026-10-17), judged by a hash of the default wording in each folder's generated, committed `translations/sources.json`; with `translations: { review: TranslationReview.required }` in `app.jadpo` (default `.none`), new or changed non-default wording is unaccepted, and listed in the approval report, until `jadpo translations accept <locale>` (Richard) | 2026-10-16 |
| Text direction | `locale.direction` is the built-in enum `TextDirection` (`.left_to_right`, `.right_to_left`), whose shipped wording is `ltr` and `rtl` (Richard) | 2026-10-16 |
| Jadpo's own text | Jadpo's own messages, with wording shipped in `en_gb` and `en_us` at first and more locales added by releases; another supported locale is an error listing the missing messages until the app words them; only switched-on features' messages count, rewording another is an error, and `jadpo translations builtin` lists only those (Richard) | 2026-10-16 |
| Text the app did not write | The root `translations/<locale>.jadpo` groups wording by owner, `jadpo: { }` and a package's name, the one grouping allowed, checked against the owner's types; lookup is the app's root wording, then the owner's, then (in progress) the regional sibling, then the default; supported locales must be complete for every package and Jadpo message the app can show (Richard) | 2026-10-16 |
| Package translations | A library package carries a `translations/` folder like a domain's, called `Postcodes.translations.<message>(...)`; provider packages carry none; changed fields break the app's rewording, changed wording makes it stale; published with complete default-locale wording and `///` on every public message; a different code for a package failure is mapped with `attempt` (Richard) | 2026-10-16 |
| Failures sent to clients | Wording lives in the language files, never on the failure type (one rule for all text; Jadpo's and packages' failures could not carry it in place); every failure that can reach a response has wording in every supported locale; dates and numbers in client fields are sent as values and formatted by the client, and the per-locale export says which placeholders need formatting and how (Richard) | 2026-10-16 |
| Translation tooling | `jadpo translations missing` and `unused` with `--json`; `update`, `accept <locale>` and `builtin`; `export` and `import` as XLIFF or JSON; `--json` wherever they print; go to definition from a call to its type and wording; a pseudo-locale for development and tests; the app's own data is not a translation (Richard) | 2026-10-16 |
| Reading `config { }` (chosen in chunk 5.1) | A setting is `name: Type` with an optional block of `secret:`, `default:`, `binding:` and `test:`, each once and no others (`configuration.duplicate_option`, `configuration.unknown_option`); a line that is not a setting is `configuration.expected_field`, a second `config { }` is `configuration.multiple_declarations`, and a name that is no setting is `configuration.field_unknown` with the nearest name. A variable set to nothing is not set; the process's environment wins over `.env.local` | 2026-10-16 |
| What a setting can be (chosen in chunk 5.1) | A named or built-in type of one piece of text, a number, `Bool`, `Uuid`, `Duration`, `Instant` or `CalendarDate`, an enum with no data, `Zone` (read as `Europe/London`) or `Locale` (read as `en-GB`). Not `Bytes`, `Time`, a list, object, map or `T?`, or the types the runtime makes (`PageCursor`, `PageSize`, `ApiToken`, `ServiceKey`): `configuration.type_unsupported`. A secret is text, so a test run can make one. A setting may be declared with a built-in type; naming the type code passes it as saves building it | 2026-10-16 |
| Variables (chosen in chunk 5.1) | A variable is capitals, digits and `_` starting with a capital (`configuration.binding_invalid`, `configuration.binding_empty`); no two settings share one, and a `SigningKey`'s `_PREVIOUS` counts as taken (`configuration.duplicate_binding`) | 2026-10-16 |
| `default:` and `test:` (chosen in chunk 5.1) | A literal of the setting's type, checked against every rule of the type and its parents (`configuration.default_literal_required`, `configuration.default_type`, `configuration.default_invalid`, the same three for `test:` and a test's `config:`); an enum's variant is written `Type.value`. A secret has neither (`configuration.secret_default`), and `secret:` is `true` or `false` (`configuration.secret_invalid`) | 2026-10-16 |
| Built-in connection types (chosen in chunk 5.1) | `Url` accepts only `http://` and `https://`. `PostgresUrl` (`postgres://`, `postgresql://`), `RedisUrl` (`redis://`, `rediss://`) and `SigningKey` (at least 32 visible characters) are built in, text underneath, always secret and never stored by an entity | 2026-10-16 |
| Connection URL structure (chosen in chunk 9.1d2a) | Connection types check URI structure at literals, startup and `config check`, without network I/O; preserve PostgreSQL multi-host, IPv6, encoded credentials and local sockets, reject malformed targets, escapes and ports, and report every invalid declaration without values | 2026-10-23 |
| A database's authority (chosen in chunk 8.1n) | A setting may be area 19's `Certificate`, read from its PEM, as a database connection's `authority:` needs (area 5, "PostgreSQL over TLS"); a `PostgresUrl` that sets `sslmode` or another `ssl…` setting is invalid | 2026-10-21 |
| Where a secret may go (chosen in chunk 5.1) | The value of a `url:`, `credential:`, `signing_key:` or `secret:` member of `application { }` or `authentication { }`, and nowhere else: no function, action, query, fixture or test is a place the compiler owns, so any read of a secret there is `configuration.secret_flow`, which needs no tracking through variables and calls, and covers comparing | 2026-10-16 |
| Connection addresses (chosen in chunk 5.1) | A connection's `url: config.<name>` reads a setting of the type its driver needs (`configuration.url_type`); an address written in source is refused the same way. A project with no database declared opens `main` at `config.database_url` when it declares one (of the right type) | 2026-10-16 |
| Values in tests (chosen in chunk 5.1) | A test reads each setting's `test:` value, then its default; a secret is a fresh random hexadecimal value for each run (a `PostgresUrl` or `RedisUrl` an address at `127.0.0.1:1`); the environment is never read. A test's `config:` changes non-secret settings for that test and the code it calls; a secret there is `configuration.secret_default`. A setting a test reaches through calls with none of the three is `configuration.test_value_missing` (a request through a route is not followed; the run's fault names it) | 2026-10-16 |
| Starting (chosen in chunk 5.1) | `jadpo_runtime::config::load` checks every setting against its type with a host's variables (`Host::variable`: the process natively, bindings on Workers) and gives each problem as the setting, its variable and the rule it breaks, never the value; the same function is behind `jadpo config check` and the interpreter, so they cannot disagree | 2026-10-16 |
| `jadpo config` (chosen in chunk 5.1) | `check` lists each setting as `set`, `default`, `missing` or `invalid` with its variable, shows each problem as `configuration.value_missing` or `configuration.value_invalid`, exits 1 on any and supports `--json`. `set <field>` needs a terminal and refuses `--json`, hides what is typed for a secret and shows it otherwise, checks the value before writing, and writes `.env.local` only as an ordinary file, through a private temporary file, if unchanged since read, and adds it to `.gitignore` | 2026-10-16 |
| `jadpo dev` and `jadpo migrate` (chosen in chunk 5.1) | `dev` checks the environment over `.env.local` as the app starts, watches `.env.local` with the source, and keeps the last good version serving through an invalid change; a connection whose address no variable gives gets a database on a disposable PostgreSQL (`--url` still names the database directly). `migrate apply` and `status` use a connection's setting when `--url` is not given | 2026-10-16 |
| Retention period (chosen in chunk 5.1) | `purge: { after: config.x }` takes a `Duration` setting that is not secret (`data.lifecycle_purge_after` otherwise); the period is the value the app started with, given to a run (`jadpo_runtime::purge_after`), and a period not above zero deletes nothing | 2026-10-16 |
| Not built here (chosen in chunk 5.1) | A browser session's `origin:` and `signing_key:` reading settings, with signed sessions and tokens, are chunk 5.1b (built there, area 8). Which proxies a deployment trusts for a client's address is chunk 5.1c's, below | 2026-10-16 |
| Trusted proxies | `client_address: { source: ClientAddressSource.x hops: <Int> }` once in `app.jadpo`; sources `connection` (the native default), `forwarded_for`, `forwarded` and `cloudflare`; the client is the `hops`-th address from the right, falling back to the connection's address when the header is short or invalid; on Workers only `cloudflare` (`CF-Connecting-IP`), its default (question 12, Richard) | 2026-10-16 |
| Trusted proxies (chosen in chunk 5.1c) | `hops:` is required with `forwarded_for` and `forwarded` and refused with `connection` and `cloudflare` (`configuration.client_address_hops_unused`); written out it is a whole number from 1, and read from a setting that setting is an `Int` (or a type built on it) declaring `min:` of at least 1, so a count below 1 never starts (`configuration.client_address_hops_invalid`). A second `client_address:` is `configuration.client_address_duplicate`, a `client_address:` that is not a block of `source:` and `hops:` is `configuration.client_address_invalid`, and a `source:` that is no `ClientAddressSource` is `configuration.client_address_source_unknown` | 2026-10-16 |
| Reading the address (chosen in chunk 5.1c) | Every line of the header joined in order, split at commas; `Forwarded` by elements, each giving the entry of its `for`; an empty entry, a name, `unknown`, an obfuscated identifier or a zone takes its place in the count but is no address; a `Forwarded` that does not parse, or a `CF-Connecting-IP` that is not exactly one address, falls back whole. Addresses are accepted with a port or in brackets and kept in one spelling (IPv6 lower case and shortened, an IPv4-mapped address as IPv4), the connection's too. With no `client_address:` the address is the connection's natively and `CF-Connecting-IP` on Workers, where any other source is refused and an address Cloudflare did not give is unknown | 2026-10-16 |
| Where the address is chosen (chosen in chunk 5.1c) | In the runtime, as plan data (`ClientAddressPlan` in the route table) read with the settings the app started with: `serve_request` counts a request by the address `ClientAddress` chooses from its headers and the connection address the host gives, and carries it on the response, so `jadpo dev` shows it after the time on each request line and as `address` with `--json`. The native host only supplies the connection's address | 2026-10-16 |
| `ZonedTime` (chosen in chunk 5.2) | Replaces the time of day the first build called `Time`: an instant and the `Zone` it is seen in, with no literal, sent as `{ "instant": ..., "zone": "Europe/London" }`. Two are equal when both parts are; they have no order of their own, their instants do (`temporal.instant(...)`). An entity's `ZonedTime` field is stored in two columns, its instant as `timestamptz(3)` and its zone as `text`; it is not a setting. The only clock time is `resolve`'s `at:`, written out | 2026-10-16 |
| Time range (chosen in chunk 5.2) | An `Instant` holds 0000-01-01T00:00:00Z to 9999-12-30T22:00:00.999Z, the last whose local date is in year 9999 in every zone, and a `CalendarDate` 0000-01-01 to 9999-12-31; a literal past them is refused while compiling, and arithmetic past them is the fault `time_out_of_range` | 2026-10-16 |
| Time arithmetic (chosen in chunk 5.2) | `Instant ± Duration` and `Duration + Instant` keep the instant's named type; `Instant - Instant` of the same type after widening gives a plain `Duration`, negative when the second is later; `Duration ± Duration` of the same type, or with a literal, gives a plain `Duration`. A `CalendarDate` or `ZonedTime` takes no `Duration`, and `*`, `/` and `%` take no time value (`type.arithmetic_operand`, naming `temporal.add_days`) | 2026-10-16 |
| Time on the wire (chosen in chunk 5.2) | A `Duration` is written ISO 8601 in hours, minutes and seconds (`PT720H` for `30d`, `-PT5M`), and read the same way in JSON, query strings, paths and headers, days counting 24 hours; `5m` is source's spelling and refused there, but a setting keeps it. A `Zone` is sent as its IANA name and a `Locale` as its BCP 47 tag, as a setting reads them, and OpenAPI lists them so; a `ZonedTime` is an object of a `date-time` `instant` and a `zone`, both required | 2026-10-16 |
| Unclear local times (chosen in chunk 5.2) | `dst:` and `invalid_day:` each settle a condition a call fails on, which its `attempt` maps by the option's name (`dst:`, `invalid_day:`) as a construction's broken rule is mapped: an `invalid_value` failure, `invalid:` for the rest, `failure.rule_unmapped` and `failure.rule_cannot_break` as for rules. Only a value written out other than `reject` settles one; a value passed in might be `reject`. `Dst` keeps Temporal's meanings (earlier or later of an overlap; in a gap, the clock moved back or forward by its length; `compatible` earlier in an overlap and later in a gap); `InvalidDay` is `reject` or `last_valid_day`. `dst:` on a `CalendarDate` is `type.temporal_option_unused` | 2026-10-16 |
| `at:` (chosen in chunk 5.2) | A clock time written out, `HH:MM`, `HH:MM:SS` or `HH:MM:SS.mmm`, each field two digits; anything else, a value passed in included, is `type.temporal_clock_invalid`, since no type holds a time of day | 2026-10-16 |
| The `temporal` library (chosen in chunk 5.2) | Option B's operations, nineteen built: `in_zone`, `resolve`, `between`, `add_days`, `add_months` and `add_years` (twelve months a year) on a date or a local time, keeping its named type; `day_bounds`, `week_bounds` (from `starts_on:`), `month_bounds` and `year_bounds` giving an `InstantRange` whose `start` is the period's first instant (midnight, or the first time a zone's clocks show when they skip it) and whose `end` is the next period's, not included; `year`, `month`, `day` and `weekday` of a date or a local time's date; `date`, `hour`, `minute`, `zone` and `instant` of a local time, `instant` added because a `ZonedTime` has no order of its own. A query compares a field with an `InstantRange` parameter's `start` and `end` | 2026-10-16 |
| Time zone data (chosen in chunk 5.2) | The `temporal` operations run in `jadpo-value` on jiff 0.2.38 with its bundled database (`jiff-tzdb`, IANA 2026e, about 200 KB): pure Rust, built for wasm32, with Temporal's handling of gaps and overlaps, and nothing read from the platform's zone files or clock, so the interpreter, native deployables and Workers answer alike. Every `Zone` variant is in it, which a test holds; a newer release is a change of its own | 2026-10-16 |
| Not built here (chosen in chunk 5.2) | The formats (`TimeFormat`, `ComponentStyle`, the built-in formats), `PresentationText` and its errors, and `temporal.format` and `format_relative` checked are chunk 5.2b; the text they give, in each locale, is chunk 5.3 | 2026-10-16 |
| Formats (chosen in chunk 5.2b) | `TimeFormat` is a base, as `Object` is: `type Name = TimeFormat { part: ComponentStyle.style ... }` declares a format, and `TimeFormat.` names the twelve built-in ones (`date_`, `time_`, `date_time_` with `full`, `long`, `medium`, `short`). The parts are `weekday` (`long`, `short`, `narrow`), `day`, `year`, `hour`, `minute` and `second` (`numeric`, `two_digit`), `month` (all five) and `zone_name` (`long`, `short`), as `Intl.DateTimeFormat` takes them; the locale orders them. At least one part (`type.temporal_format_components_empty`), each once (`type.temporal_format_component_duplicate`), in a style it takes (`type.temporal_format_component_invalid`); any other name is `type.unknown_setting`. A format holds no value: `format:` names one written out, and a format as a field's, parameter's or variable's type, as a value or passed in is `type.temporal_format_invalid`. The operation is given it as text, `day:numeric month:short year:numeric` or `date_long` (`jadpo_std::TimeFormatSpec`), as `at:` gives its clock time | 2026-10-16 |
| What formatting takes (chosen in chunk 5.2b) | `format` shows an `Instant`, a `CalendarDate` or a `ZonedTime`, and `format_relative` an `Instant` or a `ZonedTime` against `now:`, an `Instant`. `zone:` is given exactly for an `Instant` (`type.temporal_zone_required`), and with a date or a local time is `type.temporal_option_unused`. A date takes only a date format: a `time_` or `date_time_` one, or a part `hour`, `minute`, `second` or `zone_name`, is `type.temporal_format_requires_time`, which names that now rather than an instant without a zone. `locale:` was any `Locale` until chunk 5.3, which checks it against the supported ones. `type.temporal_format_choice` is gone: `format:` replaced `style:` and `components:` (Option B) | 2026-10-16 |
| `PresentationText` (chosen in chunk 5.2b) | A built-in type that is its own representation, so no constructor turns it into `Text` or back, and nothing orders it, adds to it or reads a time from it; `==` and `!=` compare it. A signature may use it directly, as `Email`, and text in quotes may stand for one, which is how a test compares formatted text. An `Output` holds it, sent as a JSON string, and a request may send one as text. An entity field holding it, directly or through a named type, a collection, an object's field or a variant's data, is `type.presentation_text_persistence`; a setting of it is `configuration.type_unsupported` | 2026-10-16 |
| Not built here (chosen in chunk 5.2b) | The text: a call to `temporal.format` or `format_relative` that checked was `toolchain.not_built_yet`, naming chunk 5.3. Chunk 5.3 built it (below) and removed the refusal | 2026-10-16 |
| `locales { }` (chosen in chunk 5.3) | An unnamed singleton declaration in `app.jadpo` (as area 1 reserves the word), not a member of `application { }`: the page's `locales: { ... }` was written with a colon and is amended to `locales { default: Locale.en_gb supported: [Locale.en_gb, ...] }`, each once and no others, entries written `Locale.<tag>` (`type.locale_invalid` for text, a non-list, a variant `Locale` has not; the fix for `"fr-FR"` is one edit), none twice (`type.locale_duplicate`), the default among them (`type.locale_default_unsupported`). A missing option is `syntax.locales_default_required` or `syntax.locales_supported_required`, a given-twice option `syntax.locales_duplicate_option`, any other name `syntax.locales_unknown_option` (`unsupported:` among them: there is no such setting), a second `locales { }` `semantic.multiple_locale_declarations`. `syntax.locales_unsupported_required` and `syntax.locales_unsupported_invalid` are gone with `unsupported:` | 2026-10-16 |
| What a locale is checked against (chosen in chunk 5.3) | A project formats text for people only with `locales { }` declared (`type.temporal_locales_required`, at the call); files handed to the compiler on their own have no application to declare them, so the rule does not apply. A `Locale.x` written as `locale:`, or as the `default:` or `test:` of a `Locale` setting, that is not supported is `type.locale_unsupported`. A `Locale` passed in is read through the wire or a setting, which accept only the supported tags (`invalid_value`, rule `variant`; OpenAPI lists only those); with no `locales { }` every `Locale` is accepted as before. A problem in the declaration is reported once, and the call is not also reported | 2026-10-16 |
| Locale data (chosen in chunk 5.3) | ICU4X 2.3 with Unicode CLDR 48 compiled into the binary (`icu_datetime`, `icu_calendar`, `icu_time`, `icu_locale` 2.3 and `icu_experimental` 0.6 for the relative-time words, pinned exactly; pure Rust, builds for wasm32): the interpreter, native deployables and Workers write the same text. A WebAssembly module that formats is about 3.5 MB larger than one that does not (3.9 MB against 0.35 MB; 1.2 MB against 0.1 MB compressed with gzip, 0.8 MB with Brotli), because the data holds every locale `Locale` lists. The next lever, if a deployable needs it, is data exported for the supported locales only | 2026-10-16 |
| Locale data a deployable carries (chosen in chunk 8.2c) | `jadpo build` bakes again, from the ICU4X data compiled into Jadpo, each marker's entries for the locales a supported locale's lookups walk through (its fallback for that marker, down to `und`; only `und` with no `locales { }`), every locale's collation (`list.sort_by` took any `Locale`, area 19; until chunk 8.4i, which kept the supported locales', and chunk 8.4j, which made sorting by a locale need `locales { }`), and every marker with one entry, into a folder of the build cache named by the data's digest (chunk 8.4g; docs/compiler-runtime.md §2.5), which its build compiles through ICU4X's `ICU4X_DATA_DIR`; `jadpo test` and `jadpo dev` keep every locale. A `Locale` the app doesn't support reaching `text.format`, `temporal.format_relative` or a message at run time is written in the supported locale `locale.from_accept_language` would give its tag, else `default:`, on every target, so both write the same text. Measured on `jadpo-value`'s `formats` example for wasm32 in release, which links every operation: 18,486,066 bytes with every locale, 8,276,025 with the root only, 8,415,647 with `en_gb`, 8,486,985 with `en_gb` and `fr_fr` (gzip 6,131,551, 2,650,565, 2,693,731 and 2,718,160); keeping every locale's collation costs 889,760 of them | 2026-10-21 |
| Sorting in a locale the app doesn't support | `list.sort_by` given a `Locale` the app doesn't support order by the supported locale `locale.from_accept_language` would give its tag, else `default:`, as formatting does since chunk 8.2c, so a deployable carries collation for its supported locales only (about 0.9 MB of 8.2c's wasm module; example 23 on Workers declaring `en_gb`, release, gzip -9: 13,614,174 bytes before, 12,727,979 after, 4,154,206 and 3,842,244 gzipped); built in chunk 8.4i | Richard, 2026-10-22 |
| Sorting with no `locales { }` | `list.sort_by` with `locale:` in a project that declares no `locales { }` is `type.temporal_locales_required`, as formatting is, at the call (not when `locale:` is already `type.temporal_option_unused` for a key that isn't text), so `jadpo build` keeps collation for the declared locales only; files handed to the compiler on their own have no application to declare them, and sort by the `Locale` they are given. Replaces chunk 8.4i's choice that an app with no `locales { }` sorts by any `Locale` and keeps every locale's collation. Only `sort_by` takes `locale:`; `min_by` and `max_by` order by code point. A one-action Workers project declaring no locales (release, gzip -9) went from 13,017,859 bytes to 12,111,677 (3,959,704 to 3,647,526 gzipped). Built in chunk 8.4j | Richard, 2026-10-22 |
| Writing a time (chosen in chunk 5.3) | Always the Gregorian calendar (as `Intl.DateTimeFormat` with `calendar: "gregory"`), the locale's own digits, CLDR's narrow no-break space written as a plain space. A built-in format is the locale's pattern at CLDR's four lengths (`date_full` with the weekday), so `date_time_long` is "27 September 2026 at 14:30:05 BST" in `en_gb`, with seconds as `timeStyle: "long"` has. A declared format takes the locale's pattern for its parts (a month as a name from the long or medium date pattern, a number from the short one; a date and a time joined as the locale joins them for that month) and sets each part's width: `numeric` leaves a number as the locale writes it, `two_digit` makes it at least two. Parts with no pattern together are written one after another with a space, and `zone_name` alone is the name alone | 2026-10-16 |
| `format_relative` (chosen in chunk 5.3) | Against `now:`, in the zone the value is seen in: under a minute either way "just now" (English; the locale's "now" elsewhere); under an hour whole minutes, a past value rounded down and a future one up; the same local date "Today at" and the short time; the date before or after "Yesterday at" or "Tomorrow at"; two to six days away the weekday and the short time; anything else the long date. The words are the locale's, the first letter a capital; a locale with no word for yesterday or tomorrow shows the long date | 2026-10-16 |
| Not built here (chosen in chunk 5.3) | Translations (`translations/`, ICU MessageFormat, typed calls, their errors) and choosing a locale from an `Accept-Language` header, which the page called a library call without naming it, were chunks 5.3b and 5.3c; both are built now | 2026-10-16 |
| Translations as JSON (chosen in chunk 5.3b, superseded) | `translations/<tag>.json`, ICU MessageFormat, one call per key (`translations.<key>(...)`), decided and built on 2026-10-16 and replaced the same week by translations as messages; chunk 5.4c took out the files, the call, MessageFormat and the errors `translation.file_invalid`, `key_duplicate`, `key_extra`, `key_invalid`, `key_missing`, `key_unknown`, `message_invalid`, `other_missing`, `placeholder_mismatch` and `placeholder_type` | 2026-10-17 |
| Chosen in 5.4c: no warnings | The design said "warning" for a field no wording uses, for wording missing from an in-progress locale and for a stale translation. Area 1, question 11 (decided 2026-10-17) has no warning severity, so: `in_progress:` is the declared opt-out and its gaps fall back with no diagnostic; a field no wording uses is the declinable `translation.field_unused`; a stale translation is an error in 5.4d | 2026-10-17 |
| Chosen in 5.4c: the default is never in progress | It is where every in-progress locale falls back last: listing it is `type.locale_in_progress_default`; a listed locale `supported:` lacks is `type.locale_in_progress_unsupported`; the list is a list of `Locale.x`, each once (`type.locale_invalid`, `type.locale_duplicate`) | 2026-10-17 |
| Chosen in 5.4c: what a file words | The messages, the emails and the enums of the folder's own scope, a domain's or `shared/`'s; an enum is worded in the scope that declares it. Members are the type's name in lower_snake_case (`translation.member_unknown`, `translation.member_duplicate`); a member is a string, or a block of strings for the values of an enum or the plural forms (`translation.wording_shape`); the language file's name is a `Locale` variant (`translation.file_name_invalid`) the application supports (`translation.locale_unsupported`) | 2026-10-17 |
| Chosen in 5.4c: when wording is required | Every message in every supported locale not in progress (`translation.file_missing` for a locale with no language file in the folder, `translation.wording_missing` for a member the file lacks, `translation.variant_missing` for a value or plural form); an enum as soon as any language file words it; an email subject never yet (7.3). Translations without `locales { }` are `translation.locales_required` | 2026-10-17 |
| Chosen in 5.4c: wordings | A string whose only syntax is `{field}` and doubled braces; anything else, including ICU syntax, is `translation.wording_invalid` at the character; a field that is not one the message has (or the enum that picks the wording) is `translation.placeholder_unknown`; an enum value's own fields are placeholders in that value's wording; a field of a type a wording can't hold, a nullable field included, is `translation.field_type`, and two choosing fields `translation.too_many_selectors` | 2026-10-17 |
| Chosen in 5.4c: plural forms | One member per CLDR cardinal category the locale has and no other (`translation.variant_unknown`, `translation.variant_missing`); `jadpo-std` holds the table, tested against ICU4X's data; the form is chosen by the locale the wording came from, and `{count}` is written the locale's way | 2026-10-17 |
| Chosen in 5.4c: calls | `translations.<domain>.<message>(field: value, ..., locale: ...)`, fields by name or in order, `locale:` named; an enum is `translations.<domain>.<enum>(value, locale: ...)`; an email subject is not called; `translations.shared` for `shared/translations/`; a call naming no message is `translation.message_unknown` with the nearest name, one naming another domain's private message `visibility.private_cross_domain` (moving it to `shared/translations/` is by hand until `layout.promotion_required` covers messages) | 2026-10-17 |
| Chosen in 5.4c: running | The wording is read into `jadpo_std::translation::Template`, carried as `TranslationPlan` entries (parameters, and the wording of each locale that has it complete), and written by `jadpo_value::present::translate`, which walks an in-progress locale's fallback chain (itself, its regional siblings with the CLDR-likely one first, the default) and writes the sentence in the language it found; each message and enum is a `translation_key` graph node digested with its wording in every language file, so a save of a language file reruns the tests that reach it without checking a call again | 2026-10-17 |
| Chosen in 5.4d: the record | `translations/sources.jadpo_record`, `record translations 1`, one line for each locale and member: `translation de_de.parcels from <12 hex> wording <12 hex> [accepted]`, BLAKE3 over the member's text in name order; a translation with no line is not stale; `update` records a translation that is new or reworded and leaves one whose wording is as it was stale; `accept <locale>` records, then accepts what is not stale; a record that doesn't read is `translation.sources_invalid` (7.0 adds the generic checks) | 2026-10-17 |
| Chosen in 5.4d: review | `translations: { review: TranslationReview.required }` in `application { }` (`translation.option_unknown`, `translation.review_invalid`); unaccepted wording is listed by `jadpo translations missing`, not an error | 2026-10-17 |
| Chosen in 5.4d: failures | The member is the failure's code; required only for a failure some route's `fails` lists, in every supported locale not in progress; client fields of text, `Uuid`, `Email`, `Url` or `IpAddress` are written as they are, numbers, dates, times and durations are formatted by the client (`number`, `decimal`, `date`, `date_time`, `duration`), anything else is `translation.field_type`; `jadpo check` writes `build/openapi/<api>.failures.<locale>.json` with the locale spelled as its variant, keys sorted, and no file for an API with nothing worded | 2026-10-17 |
| Chosen in 5.4d: text direction | `locale.direction(value)`, an operation of the `locale` namespace giving a `TextDirection`; the template variable comes with 7.3 | 2026-10-17 |
| Chosen in 5.4d: the pseudo-locale | `Locale.en_xa`, listed in `supported:` like any locale, no language file, never asked for wording; writes the default locale's wording with its letters accented, about two fifths longer and in brackets; values written into it are not accented; failures' client text is exported in it too | 2026-10-17 |
| Chosen in 5.4d: the commands | `jadpo translations missing [locale]`, `unused`, `update` and `accept <locale>`, each with `--json`; `missing` groups by language file and succeeds whatever it lists; go to definition lands on the message's type and each language file's member | 2026-10-17 |
| Chosen in 5.4e: Jadpo's text | Jadpo's messages are a catalogue in `jadpo-std` gated by feature; the root file's `jadpo: { }` is checked against it; a supported locale Jadpo does not ship is one `translation.jadpo_missing` per locale; `translation.feature_off` and `translation.group_unknown`; the root folder's `sources.jadpo_record` names members `jadpo.<message>`; Jadpo's failures join every API's export; `TextDirection`'s wording ships in every locale; `jadpo translations builtin`; `export` and `import` move to 5.4f | 2026-10-17 |
| Chosen in 5.4f: the exchange | `export` writes XLIFF 2.0 (or Chrome-style JSON) for each translated locale to `build/translations/`, one unit for each plain wording, enum value or plural form, with its description; `import` reads XLIFF 2.x or JSON, checks the result as `jadpo check` does and refuses it whole with those rules, or writes the language files and a record line for each changed translation; a translation of superseded source is written and stale; `translation.exchange_file_invalid` and `translation.exchange_unit_unknown` | 2026-10-17 |
| Name of the locale call (Richard, 2026-10-16) | `locale.from_accept_language(header)`, giving a `Locale`, in a new `locale` standard-library namespace beside `temporal`. `header` is `Text?`, so a header read with `header: "Accept-Language"` and `optional: true` passes straight in; the call never fails and picks only from `locales { supported: }`, falling back to `default:`. It is a pure function of the header and the application's locales, so the interpreter and every compiled target agree | 2026-10-16 |
| Matching an `Accept-Language` header (chosen in chunk 5.3c) | RFC 9110: split at commas, a weight `;q=` of 0 to 1 with up to three decimals, a weight of 0 excluded, `*` ignored, an entry that does not parse skipped (a range is read in any case, `_` as `-`). Tags are taken by descending weight then header order, and the first that matches wins: an exact supported locale, else a supported locale of the same language, the one CLDR likely subtags give for the language (`ICU4X` `LocaleExpander`, extended data) before any other, else the first listed in `supported:`. Only the language is compared when no supported locale has the exact tag, never the script (`zh-TW` with no supported `zh_tw` is answered by whichever supported `zh` locale the rule picks) | 2026-10-16 |
| What the call is checked against (chosen in chunk 5.3c) | A standard-library call: `header` is a `Text?` parameter the call must pass (the operation table marks it `nullable`, not `optional`), taking a `Text`, a type built on `Text`, `none`, or any of those that may be `none`. With no `locales { }` in a project it is `type.temporal_locales_required` with its reason worded for this call (files on their own have none to require). In a query it is `query.value_unsupported`, as every call is | 2026-10-16 |
| `locale` is a reserved namespace (chosen in chunk 5.3c) | A standard-library namespace cannot name a parameter, variable, declaration, domain or library (`semantic.standard_namespace_reserved`, area 1), so no parameter or variable is called `locale` any more. This page's examples, and the compiler's tests, call theirs `reader_locale`. A field (`input.locale`) or a named argument (`locale:`) is not a variable and keeps the name | 2026-10-16 |
| Where the choice runs (chosen in chunk 5.3c) | `jadpo_value::locale::from_accept_language(header, &LocalePlan)`, given the plan's locales by `jadpo_value::run` (a fourth parameter, the application's `Option<&LocalePlan>`, which `jadpo-plan` builds from `locales { }` and the interpreter passes), over `icu_locale`'s likely subtags, so the code generator will call the same function | 2026-10-16 |
| Constants as defaults (Richard) | A `constant` (area 3) counts as a literal for a setting's `default:`, so code and a setting can share one value; `configuration.default_literal_required` still refuses anything computed. Rule of thumb: if a deployment could ever need a different value it is a setting, otherwise a constant. (Revised 2026-10-16 (Richard): a constant does not count as a literal for `default:` or `test:`, which stay literals as chosen in chunk 5.1, `configuration.default_literal_required` unchanged; the rule of thumb stands. See the next row.) | 2026-10-16 |
| A constant or a setting (Richard) | A value is a constant (fixed, read `security.max_login_attempts`) or a setting (per deployment, read `config.max_login_attempts`), never both: one source, one way to read it. If a constant could seed a setting, code reading the constant would silently ignore the environment variable (the `DEFAULT_PORT` bug of TypeScript and pydantic code); `default:` on the field already names the default. Tiers: `constant` (fixed in code), `config` (per deployment), `flags { }` (area 16, changes at runtime). Decided; constants built in chunk 5.3e, with a constant refused as `default:` | 2026-10-16 |
| Making a constant a setting (Richard) | `jadpo config move <constant>`, and the editor's "Make this a setting" running the same code, adds a `config` field of the constant's name and type with its value as `default:` and its `///` description, deletes the constant and rewrites every read to `config.<name>`, so a reviewer sees each place that can now differ per deployment. Offered, never required, so not part of `jadpo fmt --layout`. Refuses, editing nothing, a type a setting can't hold, a name a setting already has, and a constant read in a query, an update by `id`, a backfill or another constant. No command moves a setting back. Rejected: a constant an environment variable can override (`setting: true`: hides per-deployment behaviour at the call site, agents add it everywhere, no single list of variables, moving a file renames its variable); a `config { }` per domain; settings declared beside their use and gathered by the compiler; values per environment in source (`production:`, `staging:`), against no layering. Built in chunk 5.3f | 2026-10-16 |
| Fields in name order (Richard) | `jadpo fmt` sorts `config { }` fields by name, comments moving with each, so parallel workspaces adding settings rarely touch the same lines; nothing gives a field's position meaning (settings are read, bound and reported by name). `jadpo fmt --check` fails on one out of order. The page examples are sorted already. Built in chunk 5.3f | 2026-10-16 |
| An IPv6 client's count (chunk 8.4d) | The address chosen is shown whole, and the default rate limits count an IPv6 one by its /64 (area 16, "Rate limits") (decided 2026-10-22) | 2026-10-22 |
