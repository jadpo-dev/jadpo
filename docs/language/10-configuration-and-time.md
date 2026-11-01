---
title: Configuration and time
area: 10
status: decided
checked-with: this checkout's jadpo on every gate run
---

# Configuration and time
<!-- topic: configuration-and-time -->

This page covers the values your app reads from its environment, such as a
database address or an API key, and everything to do with time: the current
time, dates and time zones, and showing dates, times and other text to
people in their own language.

Two rules run through the page:

- **Your code never reads the environment or the system clock itself.**
  Settings arrive as typed `config.<field>` values, checked before the app
  starts. The time arrives as `clock.now`, which stays the same for the
  whole operation.
- **Values for machines and text for people never mix.** An `Instant` is
  stored and compared. A formatted date is only shown.

## Settings: `config.jadpo`
<!-- topic: settings-config-jadpo -->

A setting is a value that can differ between deployments, such as the
database's address or the address emails are sent from. Every setting your
app needs is a field of one `config { }` declaration, in the file
`config.jadpo` at the project's root. A field has a type and, optionally,
`secret: true`, a `default:`, a `binding:` and a `test:` value, each
explained below. Code reads a setting as `config.<field>` anywhere,
functions included, because a setting can't change while the app runs.

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

Each value comes from an environment variable named after the field in
capitals: `mail_sender` reads `MAIL_SENDER`. The declaration has no name,
since a project has only one. There is no `env.jadpo` and no second file
that maps settings to variables.

A field is required unless it has a `default:`. There are no optional
settings: give the field a default, or leave the feature's setting
required.

What code reads has the field's type. `config.page_size` below is the
`ListLimit` the field declares, checked when the app starts, so code never
builds it again. Declare the field with the named type your code uses
(`page_size: ListLimit { default: 25 }`) and read `config.page_size`
wherever a `ListLimit` goes. You can declare a setting with a built-in type
such as `Duration` or `Int`, but a function's parameters and results take
named types ([Signatures use named types](02-values-and-types.md#signatures-use-named-types)).
Building a named type from a plainly typed setting is an ordinary
construction, which can fail, so it needs `attempt`.

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

By default the variable's name is the field's name in capitals. Write
`binding:` only when it must be something else, for example to keep a name
a hosting platform already sets:

```jadpo check project=binding
config {
    database_url: PostgresUrl {
        binding: "HEROKU_POSTGRESQL_URL"
    }
}
```

A binding is capital letters, digits and `_`, starting with a capital
letter, and no two settings read the same variable. A `SigningKey` setting
also reads its variable with `_PREVIOUS` after it
([Signing keys](#signing-keys)), so no other setting may bind that name.

Because the name comes from the field, renaming a field renames its
variable, and a deployment that still sets the old name breaks. So a
changed variable name is shown for review like any other change to the
app's contract ([Records and manifest](18-records-and-manifest.md)).

### What a setting can hold

A setting is read from one environment variable, so its type must be one
that can be read from a single piece of text:

- text-based types, `Int`, `Decimal`, `Bool`, `Uuid`, `Email`, `Url`,
  `IpAddress`, `Duration`, `Instant` and `CalendarDate`, and any named type
  built on one of them;
- an enum whose variants carry no data, and the built-in enums `Zone` and
  `Locale`;
- a `ModelId`, read from its text such as `openai/gpt-4.1-mini`, since every
  model an app's `ai:` names is one ([AI](21-ai.md));
- a `Money`, read from its canonical text such as `"GBP 20.00"`, since an AI
  budget, `ai: { limits: { per_day: { cost: } } }`, names one;
- a `Certificate`, read from its PEM text, since a database connection's
  `authority:` names one ([PostgreSQL over TLS](05-data.md#postgresql-over-tls)).

Lists, objects, maps, `T?` and enums whose variants carry data are refused
(`configuration.type_unsupported`). Split such a value into several fields.

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

Here `LOG_LEVEL=verbose` and `BUSINESS_ZONE=Europe/London` are valid values.
Each type is written this way in a variable:

- an enum as the variant's name;
- a `Zone` as its IANA name, such as `Europe/London`;
- a `Locale` as its BCP 47 tag, such as `en-GB`;
- a `Duration` as it is in source (`30s`, `5m`, `2h`, `30d`) or as its
  ISO 8601 text (`PT5M`);
- a `Bool` as `true` or `false`;
- an `Instant` in ISO 8601 ending in `Z`, never with an offset, and a
  `CalendarDate` in ISO 8601.

A variable set to nothing counts as not set.

### Defaults and test values

A default is a literal of the field's type, such as `default: 25`,
`default: 5s` or `default: Verbosity.normal`. The compiler checks it against
every rule of the type and of the types it is built on. `test:` is the value
every test reads, and is checked the same way. A field with no default that
a test reads needs a `test:` value
([Configuration in tests](13-testing.md#configuration-in-tests)).

Neither can name a [constant](03-logic.md#constants), even though a
constant is also fixed when the code compiles: a value is a constant or a
setting, never both ([A constant or a setting](#a-constant-or-a-setting)).

A secret can have neither. So a production secret someone forgot to set
stops the app, instead of falling back to a value written in source, and
secrets never appear in test code.

### A constant or a setting

A value that could differ between deployments is a setting. One that never
does is a [constant](03-logic.md#constants). A value is one or the other,
never both, so it has one source and one way to read it: a constant is read
as `security.max_login_attempts`, a setting as `config.max_login_attempts`.

That is why a `default:` never names a constant. If it could, code reading
the constant directly would quietly ignore the environment. Say
`security.standard_attempts` (5) were `max_login_attempts`'s default, and
someone set `MAX_LOGIN_ATTEMPTS=10`: code reading
`security.standard_attempts` would still allow 5. `default:` on the field
already names the default, in the one place a reader looks.

There are three kinds of value, from fixed to changing while the app runs:

| Kind | Where | Changes | Read as |
|---|---|---|---|
| `constant` | Beside its use, or in a library ([Constants](03-logic.md#constants)) | With a code change | `security.max_login_attempts` |
| Setting | `config { }` in `config.jadpo` | Per deployment, when the app starts | `config.max_login_attempts` |
| Flag | `flags { }` in `flags.jadpo` ([Feature flags](16-built-in-services.md#feature-flags)) | While the app runs | `flags.new_checkout` |

#### Making a constant a setting

When a fixed value needs to differ per deployment, a tool makes the change
for you: `jadpo config move <constant>` on the command line, or "Make this
a setting" in the editor on the constant's declaration or any read of it.
Both run the same code, as the editor's rename runs `jadpo rename`
([Editor support](14-tooling.md#editor-support)). The move is offered,
never required, so `jadpo fmt --layout`, which makes only the moves an
error asks for, doesn't make it. In one step it:

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

Every read changes, so a reviewer sees each place whose behaviour can now
differ per deployment, and the tool does the rewriting. The new variable,
`MAX_LOGIN_ATTEMPTS`, comes from the field's name as any setting's does.

The move refuses, editing nothing and saying why, for:

- a constant whose type a setting can't hold ([What a setting can
  hold](#what-a-setting-can-hold): a list, an object, `T?`, an enum with
  data);
- a constant whose name a setting already has, naming that setting;
- a constant read where a setting can't be: in a query, an update by `id`,
  a backfill or another constant's value, listing each such read.

Each reason is an error of its own: `configuration.move_type_unsupported`,
`configuration.move_name_taken` and `configuration.move_read_unsupported`,
which are [command errors](14-tooling.md#command-errors). They are all
given at once, ending `config move refused: nothing changed`.

A project with other errors isn't moved: its errors are shown as `jadpo
check` shows them, and the editor shows the action as unavailable. A result
that wouldn't check (another constant left unused, say) is undone and
refused with the errors it would have, and nothing is written.

When exactly one setting's `default:` names the constant
(`configuration.default_literal_required`, below), that error alone
doesn't stop the move: that setting is the one the constant becomes, its
`default:` becomes the constant's value, and every read becomes
`config.<that setting>`.

Moving a setting back to a constant has no command. It is rare, and
removing a setting changes every deployment's environment, so you make it
by hand.

Jadpo has no constant that an environment variable can override, no
`config { }` block per domain, no settings gathered from beside their use,
and no values per environment in source (`production:`, `staging:`). Each
would make a value look fixed where it differs per deployment, or lose the
one list of variables an operator sets ([Values on your machine and in
production](#values-on-your-machine-and-in-production)).

### Fields in name order

`jadpo fmt` sorts the fields of `config { }` by name, each with the comments
written above it and after it on its line. So two changes that each add a
setting rarely touch the same lines, and they merge cleanly.

A setting's position means nothing: it is read, bound and reported by name.
`jadpo config check` and the check before the app starts list settings in
the order they are written, which is name order once formatted. `jadpo fmt
--check` fails on a `config { }` out of order. The examples on these pages
are in name order.

## Secrets
<!-- topic: secrets -->

A secret is a setting that must never leave the server, such as a password
or an API key. Mark it with `secret: true`. Some types are always secret
and need no flag: `PostgresUrl`, `RedisUrl` and `SigningKey`. Here the
database's address, a `PostgresUrl`, is read by the connection that uses
it:

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

Your code can use a secret only where Jadpo itself takes the value and
keeps it on the server:

- a connection's `url:`;
- an integration's `credential:`, and its webhook `secret:`
  ([Connecting in `app.jadpo`](11-integrations.md#connecting-in-appjadpo));
- an authentication `signing_key:`
  ([The application and revocation](08-identity.md#the-application-and-revocation)).

All of these are written in `app.jadpo` or `authentication.jadpo`. The one
other place is a `crypto` call's `key:`, written `key: config.<name>` with
an `EncryptionKey`, `SigningKey` or `PrivateKey` setting
([Hashing and cryptography](19-standard-library.md#hashing-and-cryptography)).

Anywhere else in a function, action, query or test, reading a secret at all
is an error, `configuration.secret_flow`. That covers returning it, storing
it, logging it, putting it in a failure, building another value from it,
passing it on, and even comparing it: `key == config.api_key` is refused,
because a yes-or-no answer gives the secret away one guess at a time.

Because code never reads the secret, nothing your app prints, stores,
returns or logs can hold it. `jadpo config check`, `jadpo dev` and the check
before the app starts say which setting and which variable is wrong, never
the value.

## Database and other connection URLs
<!-- topic: database-and-other-connection-urls -->

A database or a cache takes its address from a setting. Each one is a
connection in the `connections:` block of `app.jadpo`, with a driver that
says what kind of store it is
([How packages, connections, capabilities and services fit together](11-integrations.md#how-packages-connections-capabilities-and-services-fit-together)
says what a connection, a driver and a provider package are):

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

A project that declares no database has one called `main` on PostgreSQL.
It opens at `config.database_url` when the project declares that setting as
a `PostgresUrl`.

### Connection types

`Url` accepts only `http://` and `https://`, so it can't hold an address
such as `postgres://user:password@host/db`. Each driver has a built-in
type that can:

- `PostgresUrl`, for `postgres://` or `postgresql://`;
- `RedisUrl`, for `redis://` or `rediss://`.

They are always secret, because they normally carry a password. A
connection's `url:` must read a setting of the type its `driver:` needs
(`configuration.url_type`), and an address written in source is refused the
same way. The runtime opens the connection only through that setting.

A connection type checks the address's structure as well as its scheme,
with the same check for literals, the check before the app starts and
`jadpo config check`. Checking never looks up a name or opens a connection.
These are invalid:

- an empty target after the user information;
- a malformed percent escape;
- an invalid port;
- broken IPv6 brackets.

These pass: PostgreSQL addresses with several hosts, encoded credentials,
bracketed IPv6, and explicit socket addresses (`postgres://%2Ftmp/app`,
`postgresql:///app?host=/tmp`). The type also accepts `postgres:///app`,
but that alone gives the driver no socket folder, and the driver assumes
no default one. The driver checks its own options, and whether the
database can be reached, separately. A Redis address needs a host and,
when it has a database path, a number from 0. Every invalid declaration is
reported at once, and credentials are never shown.

### TLS and certificates

A `PostgresUrl` can't set TLS itself (`sslmode` and libpq's other `ssl…`
settings), because its connection in `app.jadpo` decides that
([PostgreSQL over TLS](05-data.md#postgresql-over-tls)). Such a value is
`configuration.value_invalid`, which says so.

A setting can be a `Certificate`, read from its PEM text. It is what a
database connection's `authority:` reads: the authority that signed the
database server's certificate.

### Provider packages

A connection to a provider package, a package that calls an outside
service ([Connecting in `app.jadpo`](11-integrations.md#connecting-in-appjadpo)),
reads its settings the same way, never a value written in source:

- `credential:` reads a `secret: true` setting;
- `environment:` reads a `ProviderEnvironment` setting, the built-in enum
  `.sandbox` or `.live`, which chooses between the package's hosts;
- `host:`, for a package with no fixed hosts, reads a `Url` setting.

A wrong one is `connection.value_invalid`.

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
<!-- topic: values-on-your-machine-and-in-production -->

In production, values come from the platform's environment, or from its
secret manager, which sets the same variables. On your machine they come
from one file, `.env.local`, which git ignores. Jadpo has no `.env`,
`.env.development` or layering of files. When the process's environment and
the file both set a variable, the environment wins, and nothing else is
read.

Tests use each field's `test:` value, then its default, and `jadpo test`
makes a random throwaway value for every secret. A single test can override
a value with its `config:` setting
([Configuration in tests](13-testing.md#configuration-in-tests)).

### Setting a value: `jadpo config set`

Never type a secret into a chat with an agent. The agent asks you to run
this, and you type the value at the prompt:

```text
$ jadpo config set session_signing_key
? Enter SESSION_SIGNING_KEY hidden
session_signing_key configured locally
    variable  SESSION_SIGNING_KEY
    file      .env.local
```

The command asks at a prompt, never through a command-line argument, and
hides what you type when the setting is secret. It checks the value against
the field's type before writing anything. A value that breaks the type is
refused by naming the rule it breaks, never by printing the value.

It writes `.env.local` safely: only to an ordinary file, through a
temporary file only its owner can read, only if the file hasn't changed
since it was read, and with `.env.local` added to `.gitignore`.

### Checking values: `jadpo config check`

`jadpo config check` lists each field as `set`, `default`, `missing` or
`invalid`, with the variable it reads, and never the value:

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
  help: https://jadpo.dev/docs/errors/configuration.value_missing

config incomplete: 1 problem, 3 settings

next: jadpo config set session_signing_key  stores it in .env.local
```

`config set` needs a terminal and refuses `--json`. `config check --json`
lists the same settings as data. Both commands are listed with the others
in [Commands for the database, settings and translations](14-tooling.md#commands-for-the-database-settings-and-translations).

## Signing keys
<!-- topic: signing-keys -->

A `SigningKey` is a built-in secret type for the key that sessions and
tokens are signed with: at least 32 characters, with no spaces.
Authentication reads it as `signing_key: config.session_signing_key`. When
your app sets a revocation delay, it signs sessions and tokens with that key
([The application and revocation](08-identity.md#the-application-and-revocation)).

A `SigningKey` setting reads its variable and, when that is set,
`<NAME>_PREVIOUS` as well: `session_signing_key` reads
`SESSION_SIGNING_KEY` and `SESSION_SIGNING_KEY_PREVIOUS`. New tokens are
signed with the current key, and tokens signed with either key are
accepted. So to rotate a key, set the new one and move the old one to
`_PREVIOUS`. Remove the old one once the longest token lifetime has passed.
Both keys are checked when the app starts, and neither is ever printed.

## Client addresses
<!-- topic: client-addresses -->

By default a native app takes the client's address from the connection,
and an app on Workers from Cloudflare's `CF-Connecting-IP` header. You don't
write anything for that.

The address matters because rate limits count a request by its address
while no one is signed in ([Rate limits](16-built-in-services.md#rate-limits)),
and `jadpo dev`'s request lines and the
[request timeline](17-debugging-and-review.md#the-request-timeline-in-jadpo-dev)
show it. Behind a load balancer or a CDN, the connection's own address is
the proxy's, so every client would share one allowance. The real address is
in a header the proxy adds, but anyone can send that header, so reading it
blindly lets a caller choose its own address.

`client_address:` in `app.jadpo` says, once, which header holds the real
address and how many proxies of your own deployment to believe:

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

`source:` is one of the four values of `ClientAddressSource`:

| Source | The client's address is | `hops:` |
|---|---|---|
| `connection` | the connection's own address; no header is read (the native default) | not allowed |
| `forwarded_for` | in `X-Forwarded-For` | required |
| `forwarded` | in RFC 7239's `Forwarded: for=` | required |
| `cloudflare` | in `CF-Connecting-IP` (the default on Workers, the only source accepted there) | not allowed |

### `hops:`

`hops:` is how many proxies of your own deployment sit in front of the app.
It is a count, not a list of trusted address ranges, so it can't go out of
date when a provider's addresses change. It is never guessed:
`forwarded_for` and `forwarded` need it, and `connection` and `cloudflare`,
which don't count along a header, refuse it
(`configuration.client_address_hops_unused`).

Each trusted proxy adds the address it saw to the right of the header's
list, and whatever a caller sends sits to the left of that. So with
`hops: N`, the client is the N-th address from the right, counting every
line of the header joined in order. With one proxy,
`X-Forwarded-For: 203.0.113.9` and, with a caller's own claim in front,
`X-Forwarded-For: 10.9.9.9, 203.0.113.9`, both give `203.0.113.9`. If the
header has fewer than N addresses, or the entry chosen isn't an address,
the request counts as coming from the connection's own address, never from
a guess.

`hops:` is a whole number from 1. Write it out (`hops: 1`), or read it from
an `Int` setting, or a type built on one, that declares `min: 1` or more,
so the app can't start with a count below 1. Anything else is
`configuration.client_address_hops_invalid`.

### How a header is read

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
address by its /64), the one `jadpo dev`'s request lines show (after the
time, muted, and as `address` with `--json`), and the one the request
timeline records. A native deployable chooses an address the same way
`jadpo dev` does.

### On Workers

On Workers there is no connection address to fall back to. Cloudflare sets
`CF-Connecting-IP` on every request from its edge, replacing any value a
caller sent. So `cloudflare` is the only source accepted there, and it is
what a Workers deployable uses when there is no `client_address:`. An
address Cloudflare didn't give is unknown there, and requests with no known
address share one allowance.

Any other source is refused when the project is built for Workers. `jadpo
check` refuses it for each deployable with `target: Target.wasm`
(`deployable.target_unsupported`, see
[A Workers deployable](14-tooling.md#a-workers-deployable)). A Workers
deployable that reads one doesn't start: it logs `client_address_refused`
and answers every request with a 503.

## Checking values before the app starts
<!-- topic: checking-values-before-the-app-starts -->

Your app reads only the variables its settings declare. It checks every
value against its type and refuses to start if any is missing or invalid,
so it never opens its port with bad configuration. It reports the field and
the variable's name, never the value.

`jadpo dev` runs the same check, against the environment and then
`.env.local`. It watches `.env.local` as it watches your source, and keeps
the last good version running when a change is invalid:

```text
$ jadpo dev
jadpo dev, started 12:52:17
error: `GREETING` isn't set
  reason: `greeting` is required: it has no `default:`, so the app can't start without a value for it.
  next: Run `jadpo config set greeting` to store it in `.env.local`, or set `GREETING` in the environment (human)
  rule: configuration.value_missing
  help: https://jadpo.dev/docs/errors/configuration.value_missing

not running this version: 1 error, waiting for a save that fixes them
```

A database that is set up correctly but down isn't a configuration error:
the app starts, reports that it isn't ready, and recovers when the
database is back ([Health](07-http.md#health)).

The interpreter and compiled code on every platform run the same check, so
`jadpo config check` and the app can't disagree about what is valid.

`jadpo dev` is the one place that makes up a value. A database connection
whose address no variable gives gets a database of its own, on a
throwaway PostgreSQL started for the session, so a fresh clone of the
project runs.

## Time types
<!-- topic: time-types -->

Store a moment as an `Instant`. The other types are for what an instant
isn't: a date with no time, a moment seen in a time zone, a length of
time, a zone, or a day of the week.

| Type | What it is | Example | On the wire |
|---|---|---|---|
| `Instant` | One point on the timeline, in UTC, to the millisecond | when a todo was created | `"2026-01-15T12:00:00Z"`, `"2026-01-15T12:00:00.250Z"` |
| `CalendarDate` | A date with no time or zone | a birthday, an invoice date | `"2026-10-25"` |
| `ZonedTime` | An `Instant` seen in a `Zone`, for showing or reasoning about local time | "09:30 in London" | `{ "instant": "2026-10-25T08:30:00Z", "zone": "Europe/London" }` |
| `Duration` | Elapsed time | a timeout, a retention period | `"PT5M"` |
| `Zone` | An IANA time zone, a built-in enum | `Zone.europe_london` | `"Europe/London"` |
| `Weekday` | A built-in enum | `Weekday.monday` | `"monday"` |

Use `CalendarDate` only for facts that really are dates. These types have
no time of day on its own and no date and time without a zone: a
`ZonedTime` holds an instant and the zone it is seen in. For what has no
zone, such as a shop's opening time, use `LocalTime` and `LocalDateTime`
([Time and calendar additions](19-standard-library.md#time-and-calendar-additions)).
They are never a moment until a zone is chosen.

Time types never turn into each other or into `Text` by themselves. A
function's parameters and results use named types built on them, such as
`type DueAt = Instant {}`
([Signatures use named types](02-values-and-types.md#signatures-use-named-types)).

### Literals

- An `Instant` literal is ISO 8601 with a `Z` or an offset, to the
  millisecond: `"2026-10-25T08:30:00Z"`. The compiler checks it.
- A `CalendarDate` literal is a real date, `"2026-10-25"`, also checked by
  the compiler.
- A `Duration` is written only with a unit, `15m`. `min:` and `max:` on a
  `Duration` type are enforced.
- A `ZonedTime` has no literal.

Literals compare as times, whatever offset they are written with:

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

### Precision and range

An `Instant` keeps milliseconds, the precision JavaScript clients, SQLite
and PostgreSQL all share. A literal with microseconds is refused, and writes
to PostgreSQL's microsecond column are always whole milliseconds.

An `Instant` holds the years 0000 to 9999, up to
`9999-12-30T22:00:00.999Z`, the last instant whose local date is in year
9999 in every zone. A `CalendarDate` holds the same years. A literal past
them is refused when the code compiles, and arithmetic that goes past them
is the fault `time_out_of_range`.

### `Zone`, `Weekday` and `Locale`

These are built-in enums. `Weekday` has a variant for each day of the
week, such as `Weekday.monday`. `Zone` has a variant for each zone in the
IANA time zone database's `zone.tab` (2026d), plus `utc`, named in
lower_snake_case: `Zone.america_argentina_buenos_aires`.

`Locale` has a variant for each locale Unicode CLDR 48.2.3 covers fully,
plus, for each language without a region, the language with its likely
region (`Locale.fr` and `Locale.fr_fr`, `Locale.en_gb`). That is 533 in
all, and `Locale.en_xa`, the pseudo-locale
([Tooling for translations](#tooling-for-translations)). Each is named in
lower_snake_case from its BCP 47 tag. Both tables change only in a change
of their own.

### Comparing and storing a `ZonedTime`

Two `ZonedTime`s are equal when both their instants and their zones are.
They have no order of their own, since two in different zones have none.
Their instants do, through `temporal.instant(...)`.

A `ZonedTime` field of an entity is stored in two columns: its instant as
`timestamptz(3)` and its zone's IANA name as `text`.

### Where time can be read

`clock.now` in a function, a library function or an inline function is
`type.clock_context` ([The current time](#the-current-time-clocknow)).
`clock.advance(...)` outside a test is `type.test_clock_context`
([Controlling time](13-testing.md#controlling-time)). `config.<name>` is a
setting.

## Durations
<!-- topic: durations -->

A duration is a length of time, such as a timeout. In source it is always
a number and a unit, `500ms`, `30s`, `5m`, `2h` or `30d`. ISO 8601 text
(`"PT5M"`) is used only on the wire.

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
token lifetime means. Moving a date by calendar days is a separate
operation on `CalendarDate` ([Zones and local times](#zones-and-local-times)).

### On the wire

The runtime writes a duration as ISO 8601 in hours, minutes and seconds:
`"PT5M"`, `"PT1.5S"`, `"PT720H"` for `30d`, and `"-PT5M"` for a negative
one. A request's JSON, query string, path and headers send it the same way,
and days (`"P1DT2H"`) are accepted as 24 hours each. `5m` is the source's
spelling, and a request that sends it is refused.

A setting is the exception. It is read as source writes it, `5m`, and also
as the canonical ISO 8601 text, `PT5M`, as `text.parse` reads it, so a
value copied from a response or a log works as a setting too. Every
boundary reads a standard type the same way
([Every boundary through the contract](19-standard-library.md#every-boundary-through-the-contract)).

## The current time: `clock.now`
<!-- topic: the-current-time-clock-now -->

`clock.now` is the time an operation started. It is fixed for the whole
operation, nested actions included, so every timestamp one request writes
agrees. A later request, schedule run or retry gets a new value. There is no
way to read a fresher time, and code never reads the system clock.

Only actions read the clock. Functions and queries must give the same answer
for the same arguments, so they take the time as a parameter
(`type.clock_context` otherwise). A query's condition may compare with
`clock.now`, which is then the calling action's
([Filters](06-reads-and-writes.md#filters)):

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

A test fixes the clock with `clock:` and moves it with `clock.advance(...)`,
as above ([Controlling time](13-testing.md#controlling-time)).

Timeouts are measured on a separate clock that changes to the wall clock
can't move, and code never sees it. The SQL Jadpo generates always passes
the time as a value, and never uses the database's own `now()`. The
`random` namespace can be read only in actions in the same way
(`type.random_context`, [Randomness](19-standard-library.md#randomness)).

## Arithmetic and comparison
<!-- topic: arithmetic-and-comparison -->

Add a duration to an instant to get a later instant, and take one instant
from another to get the time between them:

- `Instant + Duration`, `Duration + Instant` and `Instant - Duration` give an
  `Instant` of the instant's own named type, so you don't build it again.
- `Instant - Instant` gives the `Duration` between them, which is negative
  when the second is later.
- `Duration + Duration` and `Duration - Duration` give a `Duration`.

As with numbers, both sides must be the same type after widening, or one
must be a literal. Two durations of the same named type give a plain
`Duration`.

Instants, dates and durations order with `<`, `<=`, `>` and `>=`, and
compare with `==` on the timeline, so `12:00Z` equals `13:00+01:00`.

A `CalendarDate` has no clock time, so a `Duration` doesn't move it. It
moves by calendar days with `temporal.add_days`. A `ZonedTime` moves on its
calendar the same way, or through its instant. `*`, `/` and `%` don't take
time values.

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
<!-- topic: zones-and-local-times -->

A zone turns an instant into the local time people see on a clock. This is
never unclear: `temporal.in_zone(clock.now, Zone.europe_london)` is a
`ZonedTime`.

Going the other way, from a date and a clock time to an instant, can be
unclear. At 01:30 on the day the clocks go back, London has two 01:30s, and
on the day they go forward 01:30 doesn't exist.

### Choosing the instant: `dst:`

By default an unclear local time fails. The option `dst:` says which instant
to take instead:

- `Dst.earlier` or `Dst.later` takes that one of the two instants when the
  time happens twice. When the time doesn't exist, it moves the clock time
  back or forward by the length of the gap.
- `Dst.compatible` is the earlier when the time happens twice and the later
  when it doesn't exist, as JavaScript's Temporal does.
- `Dst.reject`, the same as leaving `dst:` out, fails.

The failure is mapped in the call's `attempt` by the option's name, `dst:`,
the way a construction's broken rule is mapped
([Construction failures by rule](02-values-and-types.md#construction-failures-by-rule)).
The failure is `invalid_value`, and `invalid:` covers every condition that
has no arm of its own.

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

`at:` is a clock time written out, `"09:30"` or `"09:30:15"`, which the
compiler checks (`type.temporal_clock_invalid`), or a `LocalTime`
([Time and calendar additions](19-standard-library.md#time-and-calendar-additions)).
In place of a date and `at:`, the call takes a `LocalDateTime`, which
carries its own clock time: `temporal.resolve(shift.start, zone:
shift_zone, dst: Dst.compatible)`.

Only a `dst:` written out stops the call failing. A `Dst` passed in as a
value might be `reject`, so the call can still fail.

`dst:` means something only for a `ZonedTime`. On a `CalendarDate` it is
`type.temporal_option_unused`.

### A day the month doesn't have: `invalid_day:`

Moving a date by months or years can land on a day the month doesn't have,
such as 31 January plus a month. `invalid_day: InvalidDay.last_valid_day`
takes the month's last day. Left out, or `InvalidDay.reject`, the call fails
with `invalid_day:` to map.

### A day in a zone

A query for "deliveries on 25 October in London" uses the day's bounds:
from its first instant up to the first instant of the next day. These are
right whether the day has 23, 24 or 25 hours. The query compares a field
with an `InstantRange` parameter's `start` and `end`, as it would with any
parameter's fields ([Filters](06-reads-and-writes.md#filters)):

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

### The `temporal` operations

The `temporal` library is always called through its name, as
`temporal.<operation>`. In this table "value" is a value built on
`CalendarDate` or `ZonedTime`, and a result of "its type" keeps the
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

The standard library adds three more `temporal` operations, `occurrences`,
`open_at` and `period_between`
([Recurrences and opening hours](19-standard-library.md#recurrences-and-opening-hours)).

A day's bounds start at its first instant: midnight or, in a zone whose
clocks skip midnight, the first time they show that day. `instant(...)` is
the operation that orders local times: a `ZonedTime` has no order of its
own, and its instant does.

Zones come from the IANA time zone database built into Jadpo (release
2026e), never from the platform's own files. So the interpreter, a native
deployable and WebAssembly on Workers give the same answers, and every
`Zone` variant is in the database. A newer release of the database comes in
a change of its own.

## Showing dates and times to people
<!-- topic: showing-dates-and-times-to-people -->

To show a date or a time to a person, format it with
`text.format(value, zone:, locale:, format:)`. Formatting needs three things
the server must never guess: the zone, the language and the format. The
call takes all three. The format is either built in or declared once as a
named type.

The language must be one your app supports, declared in `app.jadpo`
([Locales](#locales)), so this example is a whole project:

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

Every standard type is formatted for people by this one call,
`text.format(value, locale:)`. The time types' version also takes the
`zone:` and `format:` options described here. `temporal.format`, a name
you might expect, is `semantic.standard_operation_moved`, whose suggested
edit writes `text.format`. `temporal.format_relative`, below, keeps its
name.

### Where the words come from

The words come from Unicode CLDR 48, the standard data on how each
language writes dates and numbers, compiled into Jadpo. So the
interpreter, a native deployable and WebAssembly on Workers write the same
text, and nothing is read from the platform. `Locale` was made from the
same release, so every variant has data.

The calendar is always the Gregorian one, as `calendar: "gregory"` gives in
`Intl.DateTimeFormat`, so a Thai or Arabic reader of a `CalendarDate` sees
the same year the database holds. The digits are always the ones the locale
writes (`ar_eg`, `fa`, `bn`, `mr` and `ne` write their own). CLDR's narrow
no-break space before `PM` is written as a plain space, as browsers and
Node write it, so a test is written with the space it shows.

### Built-in formats

A built-in format is written `format: TimeFormat.date_time_long`. The
built-in formats are `date_`, `time_` and `date_time_`, each with `full`,
`long`, `medium` or `short`, CLDR's four lengths. Each is the locale's own
pattern for a date, a time, or both, joined with the locale's own words. In
`en_gb`, for 14:30:05 on Sunday 27 September 2026 in London, where the
clocks are on British Summer Time:

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

`date_full` is the long date with the weekday, and `date_time_long` has
seconds, as `timeStyle: "long"` does. For a handful of locales (Japanese
and Korean `time_` formats, and the punctuation of a few `date_` formats)
the pattern isn't always the one `Intl.DateTimeFormat` writes: the two can
differ in punctuation, joining words, number widths or the padding of a
24-hour clock.

A zone's name comes from the zone and the instant: "BST" or "British Summer
Time" in summer, "GMT" or "Greenwich Mean Time" in winter. Where CLDR has no
name in the locale, it is the offset ("GMT+1", "UTC+1").

A hidden project on this page checks the table.

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

### Declared formats

A declared format, a type built on `TimeFormat` such as `ShortDate` above,
lists the parts it shows, each once, with a `ComponentStyle`. The locale
decides their order and punctuation, and whether the clock has 12 or 24
hours, as JavaScript's `Intl.DateTimeFormat` does:

| Part | Takes |
|---|---|
| `weekday` | `long`, `short`, `narrow` |
| `day`, `year` | `numeric`, `two_digit` |
| `month` | `numeric`, `two_digit`, `long`, `short`, `narrow` |
| `hour`, `minute`, `second` | `numeric`, `two_digit` |
| `zone_name` | `long` ("British Summer Time"), `short` ("BST") |

These are refused:

- a format with no parts (`type.temporal_format_components_empty`);
- a part twice (`type.temporal_format_component_duplicate`);
- a style the part doesn't take (`type.temporal_format_component_invalid`);
- a name that isn't a part (`type.unknown_setting`).

A format holds no value. `format:` names it written out, and it is never
the type of a field, a parameter or a variable, nor a value passed in
(`type.temporal_format_invalid`). The operation receives it as text, such
as `day:numeric month:short year:numeric` or `date_time_long`.

A declared format takes the locale's pattern for its parts, then sets each
part to its style:

- A month written as a name is taken from the long date pattern, or the
  medium one where that also writes the month as a name. A month written as
  a number is taken from the short pattern. `month: short` is that pattern
  with the abbreviated name.
- `numeric` leaves a number as wide as the locale writes it (`dd/MM/y`
  stays), and `two_digit` makes it at least two digits.
- A date and a time are joined with the words the locale uses for that
  month style ("at" for `long`, a comma for `short`).

So the output is the locale's own. It matches `Intl.DateTimeFormat` for
most locales and most options, but not all of them. Where `Intl` shortens
`01:59` to `1:59` for `hour: numeric minute: two_digit`, Jadpo writes the
locale's `01:59`, and a Japanese or Korean weekday beside a long date has
no brackets.

Parts no pattern holds together, such as a year with a day and no month, a
weekday and a year, a minute without an hour, or a month with a time, are
written one after another with a space: `27 2026`, `30:05`,
`September, 14:30`. `zone_name` on its own writes only the name ("British
Summer Time"), where `Intl` adds the date.

### `zone:` and `locale:`

Give `zone:` exactly when the value is an `Instant`
(`type.temporal_zone_required` otherwise). A `ZonedTime` is shown in its
own zone, and a `CalendarDate` has none, so `zone:` with either is
`type.temporal_option_unused`.

A `CalendarDate` has no clock time, so a `time_` or `date_time_` format, or
one showing `hour`, `minute`, `second` or `zone_name`, is
`type.temporal_format_requires_time`.

`locale:` is a `Locale` your app supports ([Locales](#locales);
`type.temporal_locales_required`, `type.locale_unsupported`).

### Relative times: `temporal.format_relative`

For text such as "3 minutes ago", use
`temporal.format_relative(value, now: clock.now, zone:, locale:)`, for an
`Instant` or a `ZonedTime`, with `zone:` as for `text.format`. Its rules are
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

### `PresentationText`

Formatting gives a `PresentationText`, a built-in type for text meant only
for people. It can go into a response, an email or a page, and a function's
signature may use it directly. Text written in quotes can stand for one,
which is how a test compares formatted text with what it expects.

It can't be stored. An entity field holding it, directly or through a named
type, a list, an object or a variant's data, is
`type.presentation_text_persistence`, and it can't be a setting. It can't be
turned back into a time, ordered, added to or rebuilt into `Text`. Two
compare with `==` and `!=`.

On the wire it is a JSON string. Clients that format times for themselves
receive the `Instant` instead.

## Locales
<!-- topic: locales -->

A locale is a language as a place writes it, such as British English
(`Locale.en_gb`) or the French of France (`Locale.fr_fr`). Your app lists
the locales it supports, once, in `app.jadpo`. Formatting and translations
accept only those, and writing any other is a compile error. The server's
own locale is never used.

```jadpo check project=locales file=app.jadpo
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.en_us, Locale.fr_fr]
}
```

`locales { }` is a declaration with no name, like `config { }`. It takes
`default:`, the locale used when no other is chosen, and `supported:`, the
list of locales the app supports, each once. Each entry is written
`Locale.<tag>`, the tag in lower case with `_` for `-` (`Locale.zh_hans` is
zh-Hans).

A locale the list doesn't have is `type.locale_unsupported` when it is
written out where text is formatted, or as the `default:` or `test:` of a
`Locale` setting. A project that formats text and declares no locales is
`type.temporal_locales_required`.

There is no `unsupported:` setting. A request field typed `Locale` is
decoded like any enum, so a tag the app doesn't support is an
`invalid_value` failure, and the OpenAPI document lists only the supported
tags. A setting typed `Locale` is read the same way when the app starts. To
choose a language from a browser's `Accept-Language` header, call a library
function that falls back to `default:`
([Choosing a locale from a request](#choosing-a-locale-from-a-request)).

### Locales still being translated: `in_progress:`

`in_progress:` is a third, optional option, written once: a list of the
supported locales whose translations aren't finished yet. Every other
supported locale must be complete. Only a locale listed here falls back
where wording is missing, with no error, because listing it is how you say
so ([Translations as messages](#translations-as-messages)). A locale listed
there but not in `supported:` is `type.locale_in_progress_unsupported`, and
the default can't be listed (`type.locale_in_progress_default`).

```jadpo check project=locales-in-progress file=app.jadpo
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.en_us, Locale.fr_fr, Locale.pl_pl]
    in_progress: [Locale.pl_pl]
}
```

### A locale the app doesn't support

Text for people is only ever written in a supported locale. A `Locale` the
list doesn't have can still reach `text.format`, `temporal.format_relative`
or a message while the app runs: one saved with a record before
`supported:` changed, or one written out anywhere but `locale:`. It is
written in the supported locale a browser asking for it would get
([Choosing a locale from a request](#choosing-a-locale-from-a-request)): a
supported locale of its language, the one CLDR gives as the language's
likely region first, else `default:`.

A list `list.sort_by` orders in such a locale is put in that same supported
locale's alphabetical order. So `jadpo test` and a deployable, which carries
only its supported locales' sorting rules, give the same order. Sorting by
a locale needs `locales { }`, as formatting does
(`type.temporal_locales_required`), so a deployable never carries sorting
rules for a locale the app doesn't support. Changing case isn't text for
people in a language, and takes any `Locale`
([Text](19-standard-library.md#text)).

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

A deployable carries the locale data of its supported locales only.
`jadpo build` takes from the CLDR data compiled into Jadpo the entries the
supported locales reach (the locale, then each locale it falls back to,
down to the root), their sorting rules among them, and the data that is the
same in every locale, and compiles the deployable with that. `jadpo test`
and `jadpo dev` keep every locale, and write the same text.

### Text direction

Every locale knows which way its text runs. `locale.direction(value)` gives
a value of the built-in enum `TextDirection`, `.left_to_right` or
`.right_to_left`, from the script CLDR says the locale most likely uses:
`Locale.ar_eg` and `Locale.he_il` read right to left, `Locale.en_gb` left to
right. Jadpo's wording of the two values is the HTML values, `ltr` and
`rtl`, so the email layout's `<html dir="{{ locale.direction }}">` writes
one of them.

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

A browser says which languages its reader wants in its `Accept-Language`
header. `locale.from_accept_language(header)` gives the `Locale` it asks for,
out of the locales your app supports, and `default:` when the header names
none of them. The call never fails, and it is the only way a request chooses
a locale that isn't written in its input.

`header` is `Text?`, which may hold no value. An input field read with
`header: "Accept-Language"` and `optional: true`
([Headers](07-http.md#headers)) is passed straight in, whether the request
sent the header or not:

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

The rules are fixed, so every platform gives the same locale for the same
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
  as `fr_fr`. Where several are of that language, the one CLDR gives as the
  language's likely region is chosen (`en` means `en-US`, `pt` means
  `pt-BR`), and if none of them is that one, the first in the order
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
<!-- topic: translations-as-messages -->

Text your server writes for people, such as emails, generated pages and a
failure's text for client apps, is translated in Jadpo itself. Each message
is a type, each language is a file of wordings, and both live in the domain
that uses them.

### Where translations live

A domain's translations live in `domains/<domain>/translations/`, as its
emails live in `emails/`. A message a second domain uses moves to
`shared/translations/`, as anything two domains use does
([Sharing and promotion](01-project-layout-and-modules.md#sharing-and-promotion)).
You move it by hand for now: `layout.promotion_required` doesn't cover
messages yet, so another domain's call of a message is
`visibility.private_cross_domain` until you do.

The root `translations/` folder holds only your app's wording for text it
didn't write: Jadpo's own ([Jadpo's and packages' text](#jadpos-and-packages-text))
and its library packages'. A `.json` file there is ignored, like any other
file that isn't source.

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

A message is one piece of text to translate. It is a type built on
`Message`, a type base like `Failure`, `Event` and `EmailMessage`, one per
file in the domain's `translations/` folder, the file named after the type
([Files and names](01-project-layout-and-modules.md#files-and-names)). Its
fields are the wording's placeholders. Its `///` description reaches
translators as context when you export the messages
([Tooling for translations](#tooling-for-translations)). `Message` is a
type base, so no type can be called `Message`, and message names are unique
across the project, as a failure's or an email's are.

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

`PluralCount` is a built-in whole number that picks a plural form, such as
"1 parcel" or "2 parcels". It takes a value built on `Int`, is used directly
as a `Message` field, and is written as `{count}` in each form's wording. It
has no use outside a message.

### Languages

Each language is one `translations { }` declaration, with no name, in a
file named after its locale (`de_de.jadpo`). Jadpo finds it by the file's
name, as it finds `config { }`, and code never refers to it by name.
`translations` is a reserved word, like `config` and `locales`.

A file words the messages, the emails and the enums of its own folder's
scope: a domain's, or `shared/`'s. An enum is worded in the scope that
declares it. Each member of the file is a message's, an enum's or an
email's name in lower_snake_case (`order_shipped` is `OrderShipped`), as the
message's file name and its call are
([Naming](01-project-layout-and-modules.md#naming)). When two kinds share a
name in one scope, the first keeps it, and the naming rules report the
clash.

Every wording is a plain string whose only syntax is `{field}`. Write a
literal brace twice: `{{` writes `{` and `}}` writes `}`. There is no
grouping inside a file: the domain is the group, and nesting is only for
enum values and plural forms (the root file's groups by owner, below, are
the exception). There is no message inside another. A value used again,
such as a product name, is a placeholder, because languages whose words
change form can't reuse a fragment.

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
| A field built on `Money` | A `{field, number, currency}` placeholder, written as `text.format` shows the amount in the locale; `{field}` alone is `translation.placeholder_style` |
| A field built on `Percentage` | A `{field, number, percent}` placeholder, written as `text.format` shows the percentage in the locale; `{field}` alone is `translation.placeholder_style` |
| One enum field | One member per value of the enum; the field is not a placeholder |
| One `PluralCount` field | One member per plural form the language needs, from Unicode CLDR (English `one` and `other`; Polish `one`, `few`, `many` and `other`); `{count}` is available in each |
| Any other field type (`Instant`, an object, `T?`) | `translation.field_type`: format it in code and pass the text |
| More than one enum or `PluralCount` field | `translation.too_many_selectors`: split the message, so a translator never fills a grid of combinations |

The plural forms are the locale's counting forms in CLDR, and no others.
There is no exact `=0` form: a language that says "no items" differently
words it as its own `zero` form, where CLDR has one.

### Dates, money and percentages in a message

Format dates and times in code, with `text.format`
([Showing dates and times to people](#showing-dates-and-times-to-people)),
and pass them in as `PresentationText`. An amount of money is a placeholder
of its own, written by the same formatter as `text.format`, so the
translator sees a price and each language writes it its own way
([Money operations](19-standard-library.md#money-operations)):

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
`{rate, number, percent}`
([Prices, discounts and rates](19-standard-library.md#prices-discounts-and-rates)):

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
`translations.<domain>.<message>(...)`. It takes every field by name, plus
`locale:`, one of your app's supported locales. A value given without its
name is the next field's, in the order the type declares them. The locale
is always named.

Each field takes a value of its kind:

- a field that picks the wording takes its enum;
- a `PluralCount` takes any value built on `Int`;
- a text field takes a value built on `Text` or `PresentationText`;
- a number field takes a value built on `Int` or `Decimal`.

The call returns `PresentationText`, so the result can be shown but never
stored. A message belongs to its domain: another domain may call it only
when it is `public`. So a message two domains use lives in
`shared/translations/`, and is called `translations.shared.<message>(...)`.

In an email template the same call takes the email's locale, and is written
without `locale:`: `{{ translations.shipping.parcels(count: parcel_count) }}`
([Email](16-built-in-services.md#email)).

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

The text is the language's own. The plural form comes from CLDR's rules for
the locale, and numbers are written with its digits and separators, from
the same data as `text.format`. So the interpreter, a native deployable and
WebAssembly write the same text.

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

## Email subjects
<!-- topic: email-subjects -->

An email's subject line is translated like a message. The email's member
in its domain's language files is the subject, with the email's fields as
placeholders. Its body is its `.html` and `.txt` templates
([Email](16-built-in-services.md#email)).

A subject is worded where its emails are written, checked when a language
file words it, and isn't a call. It is required in every supported locale
that isn't in progress, as a message's wording is.

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

## Enums with wording
<!-- topic: enums-with-wording -->

An enum has no label for people of its own
([Enums](02-values-and-types.md#enums)). To show one, give each of its
values a wording in its domain's language files. It is checked like a
message with one enum field. Then `{{ order.status }}` in a template writes
the wording, and code reads it with
`translations.<domain>.<enum>(value, locale: ...)`. In a template, writing
out an enum that has no wording is an error that says to add it.

A value's own fields are placeholders in that value's wording, both in the
enum's wording and in a message that has the enum as its field. Another
value's fields aren't available.

An enum is worded in every supported locale as soon as any language file
words it. One that no file words needs no wording.

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

## Failures for client apps
<!-- topic: failures-for-client-apps -->

A client app can show a failure to its user in the user's language. Word
the failure in its domain's language files under its code (its `code:`
when it has one), as you would a message. Its `client: { }` fields are the
placeholders, and `kind:` and `message:` are not. The wording is never
written on the failure type itself, so the same rule holds for all text,
Jadpo's and packages' failures included. Names are unique across the
project, so a failure and a message never share a key.

Every failure that can reach a response, which is any failure some route's
`fails` lists anywhere in the project, must have wording in every supported
locale. Any other failure may be worded, and needn't be. A client field no
wording writes isn't an error: the client gets it in `details` anyway. The
not-found failure the compiler provides for an entity
([The not-found failure](06-reads-and-writes.md#the-not-found-failure)) is
worded in the entity's domain, as one the project declares is. A project
that declares no `locales { }` words no failures.

A client field that is a date, a time or a number is sent as a value (an
`Instant` stays an instant on the wire,
[Output and status](07-http.md#output-and-status)). The client formats it
for its own language before filling the placeholder. So, unlike a
message's, a failure's date or time field can be a placeholder. The field's
type decides how the client formats it:

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

### The file for client apps

`jadpo check` writes, beside each API's OpenAPI document in
`build/openapi/`, one file per supported locale named
`<api>.failures.<locale>.json` (`main.failures.fr_fr.json`), for client
apps to show. It maps each failure code to an object of the same shape,
with its keys sorted as in every generated JSON file, so a client has one
way to read it:

- `text` is the wording in `{field}` syntax, a literal brace doubled as in
  a language file.
- `formats` lists only the placeholders the wording writes that the client
  must format, each as one of `number`, `decimal`, `date`, `date_time` or
  `duration`.

A locale in progress is sent what the server would write for it, its
regional sibling's wording and then the default's, so a client has text for
every code. The example above writes this for `fr_fr` (only `slot_taken`'s
entry is shown; the file also holds Jadpo's own failures, below):

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

The server's `message` stays fixed English, for developers and logs
([`message:`](04-failures.md#message)).

Jadpo's own failures (`invalid_input`, `rate_limited` and the rest) are
worded with Jadpo's own messages
([Jadpo's and packages' text](#jadpos-and-packages-text)), and join every
API's file. An API gets the ones its routes can meet: the request failures
for any API with routes, and the failures of signing in for one with a
route that needs a caller. A Jadpo failure's text is your app's root
wording for the locale, else what Jadpo ships in it, else the in-progress
fallback. Your app's own failure under the same code is sent in place of
Jadpo's. So an API with routes has a file for every supported locale,
whether or not the app declares failures of its own.

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

Text Jadpo writes itself, its built-in emails and its failures' wording, is
Jadpo's own messages. (Jadpo generates endpoints only, never pages.) Jadpo
ships their wording in `en_gb` and `en_us`, and later releases add locales.

An app supporting any other locale words them itself. Until it does, `jadpo
check` fails with one error for each such locale,
`translation.jadpo_missing`, pointing at the locale in `supported:` (or at
the missing file's name), whose reason lists every missing message. An
`in_progress:` locale falls back instead. The default locale is held to the
same rule: if Jadpo doesn't ship it, the app words the messages itself.
`translation.field_unused` doesn't apply to Jadpo's messages.

Only the messages of features your app has switched on count. Rewording a
message of a feature that is off, such as magic-link wording without
`magic_link:`, is an error saying the feature is off
(`translation.feature_off`). `jadpo translations builtin` lists only the
switched-on features' messages, with their fields.

Every Jadpo message may use `{app_name}`, the application's
`display_name:`. An app that needs the name translated rewords the
messages. The magic-link email's messages are listed in
[Built-in emails](16-built-in-services.md#built-in-emails).

Each of Jadpo's messages has a key, a kind (a message, a failure or an
enum), the feature that switches it on, the fields it may write besides
`{app_name}`, a description for translators and the wording Jadpo ships.
Code can't call them: `translations.jadpo` isn't a scope code can call. An
email template's `{{ locale.direction }}` writes the wording of Jadpo's
`TextDirection` enum.

A feature is switched on by what the project has. A message's name is its
key in the `jadpo: { }` group, and a failure's is its code:

| Feature | On when | Its messages |
|---|---|---|
| Requests | an API serves routes | `invalid_request`, `invalid_input`, `route_not_found`, `method_not_allowed`, `request_too_large`, `idempotency_key_reused`, `rate_limited`, `unavailable`, `internal_fault`, `deadline_exceeded` |
| Signing in | a route needs a caller | `authentication_required`, `invalid_credentials`, `ambiguous_credentials`, `not_permitted`, `authentication_unavailable` |
| Signing users in | an API accepts a browser session or an API token | `user_disabled` |
| Signing services in | an API accepts a service key | `service_disabled` |
| Browser sessions | an API accepts browser sessions and a route that needs a caller writes | `csrf_rejected` |
| Locales | the app declares `locales { }` | `text_direction`, the wording of `TextDirection` |
| Magic links | the app signs people in with [magic links](08-identity.md#magic-links) | the messages in [Built-in emails](16-built-in-services.md#built-in-emails) |
| Magic-link routes | the app writes its own magic-link routes (`generate_routes: false`, [Magic links](08-identity.md#magic-links)) | `sign_in_return_invalid` |
| Passkeys | the app signs users in with [passkeys](08-identity.md#passkeys) | `passkey_challenge_expired`, `passkey_rejected`, `passkey_already_registered`, `passkey_limit_reached`, `passkey_not_found` |
| Confirming a request | a route asks for a passkey's confirmation ([Confirm one request with a passkey](08-identity.md#confirm-one-request-with-a-passkey-confirm-confirmationpasskey)) | `confirmation_required` |
| External identity tokens | an API accepts external identity tokens ([Ways to sign in](08-identity.md#ways-to-sign-in)) | `authentication_misconfigured` |

The failures of signing in that only some methods can give
(`user_disabled`, `service_disabled`, `csrf_rejected`) are on only where an
API accepts those methods, by the same rule as the OpenAPI document's
responses. An API's file for clients carries the Jadpo failures of its own
features, and the application needs those of all its APIs together.

The wording for an idempotency key whose first request is still running
(`conflict`, 409) is your app's: `conflict` is a code an app declares too,
so Jadpo doesn't word it.

The twelve magic-link messages are on once `magic_link:` is declared
([Built-in emails](16-built-in-services.md#built-in-emails)). The messages
of privacy requests ([Data requests](15-personal-data.md#data-requests))
join the catalogue with that feature.

`TextDirection`'s wording, `ltr` and `rtl`, is the same HTML values in every
language, so Jadpo ships it in every locale and no app locale has to word
it. An email template's `{{ locale.direction }}` writes it.

### A library package's text

A library package, one that adds code to your app rather than calling an
outside service ([Kinds of package](11-integrations.md#kinds-of-package)),
carries translations as a domain does: a `translations/` folder with its
message types, its failures' wording and one `<locale>.jadpo` per language
it supports. Your app calls a package's message as
`Postcodes.translations.<message>(...)`. A provider package shows users no
text, because its failures are always mapped to the app's own, so it
carries none.

### The root wording file

The root `translations/<locale>.jadpo` holds your app's wording for text it
didn't write, grouped by whose text it is: `jadpo: { }` for Jadpo's own, and
the package's name for a package's. This is the one place a language file
has groups, and each group is checked against its owner's message and
failure types. A member outside any group is `translation.wording_shape`,
and a group that names no owner is `translation.group_unknown`.

The root folder holds language files and nothing else
(`layout.wrong_folder` for any other declaration), and a root file with no
`locales { }` in the project is `translation.locales_required`.

A wording of a Jadpo message is checked as any wording is: it may write
only `{app_name}` and the message's own fields. `text_direction`, Jadpo's
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

## Packages' text in an app
<!-- topic: packages-text-in-an-app -->

To show a package's or Jadpo's message in a locale, Jadpo looks first in
your app's root wording for that locale, then in the owner's own. For an
in-progress locale it then tries the regional sibling, in the same order,
and then the default locale, in the same order.

Your app's supported locales must be complete for every package and Jadpo
message the app can show. So an English-only package in an app that
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
  help: https://jadpo.dev/docs/errors/translation.file_missing

check failed: 1 error, 4 files
```

To show a package failure under a different code, not just in different
words, map it to your app's own failure at the call with `attempt`, as for
any package failure ([Calling](11-integrations.md#calling)).

### When a package changes

A package that changes a message's or a failure's fields makes a breaking
change to its surface, the list of what it offers
([The surface](11-integrations.md#the-surface)). So your app's rewording
fails to compile until you update it.

A package that changes its own wording marks your app's rewording stale,
through `sources.jadpo_record`
([Stale translations](#stale-translations-sourcesjadpo_record)), and the
same holds for Jadpo's wording. The root folder's
`sources.jadpo_record` records, for each wording your app gives outside the
default locale, a hash of the default wording it was made from. That is
your app's own default-locale wording of the message or, if the app gives
none, what Jadpo or the package ships. A default locale Jadpo doesn't ship
and the app doesn't word has nothing to compare with, so its translations
aren't judged.

A library package is published with complete wording in its own default
locale and a `///` description on every public message.

### Wording two versions of a package

While your app holds two versions of a library package side by side
([Where packages live](11-integrations.md#where-packages-live)), it words
each under a key carrying the version, `postcodes_v2: { }` and
`postcodes_v3: { }`, as its names carry it. With one version held, the key
is the folder's name. Here the app's own `Postcodes` is held at `v2` and
`v3` while callers move across, and the French words each:

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
  help: https://jadpo.dev/docs/errors/translation.group_unknown

check failed: 1 error, 9 files
```

### Packages' text: groups, calls and the surface

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
- **Every language a package ships is complete.** A package has no default
  locale, so the publishing bar, the checks a package must pass to be
  published, holds every language it ships to completeness. Each language
  file words every public message, and an enum once any file words it (a
  `wording` row, shown when the package carries translations). A package
  with messages and no language file fails it.
- **The surface.** The package's entry has a `locale` line for each
  language it words; each public message is an entry, `message
  translations.<message>` with its fields, after the functions. A new
  locale or message is the next minor version, a message's changed fields
  or a dropped locale are breaking, and changed wording is a patch.
- **The commands.** `missing`, `update` and `accept` take a package's
  text in, and so do `export`, `import` and `unused`, below.

### Packages' text in the exchange

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

### Two versions of a package

How your app's wording follows a library package held in two versions side
by side
([Two versions side by side, lazy loading and the package cache](11-integrations.md#two-versions-side-by-side-lazy-loading-and-the-package-cache)):

- **The key.** Exactly while names through the package carry the version,
  the app words each version's text
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
  don't rewrite its names, and `jadpo fix` is still only for type
  clashes ([Commands for records, findings and review](14-tooling.md#commands-for-records-findings-and-review)).

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

## What the compiler checks of translations
<!-- topic: what-the-compiler-checks-of-translations -->

`jadpo check` checks every language file against the messages, enums,
emails and failures it words. Each problem has its own error, listed at the
end of the page.

### Mistakes in a language file

- a member that no message, enum, email or failure declares
  (`translation.member_unknown`, with the nearest name);
- a member written twice (`translation.member_duplicate`);
- a member in the wrong shape for what it words, such as a string where a
  block of values is needed, or the reverse (`translation.wording_shape`);
- a wording that isn't a plain string with `{field}` placeholders
  (`translation.wording_invalid`);
- a variant that isn't a value of the enum, or a plural form the language
  doesn't use (`translation.variant_unknown`);
- a missing variant or plural form (`translation.variant_missing`);
- a placeholder that isn't a field (`translation.placeholder_unknown`);
- an amount of money or a percentage written without its style, or a style
  on another field (`translation.placeholder_style`);
- a field type a wording can't hold (`translation.field_type`), or more
  than one enum or `PluralCount` field (`translation.too_many_selectors`);
- a language file that isn't named for a locale
  (`translation.file_name_invalid`), or is for one the application doesn't
  support (`translation.locale_unsupported`);
- a call of no message (`translation.message_unknown`);
- a `locale:` that isn't a supported locale (`type.locale_unsupported`);
- translations in a project with no `locales { }`
  (`translation.locales_required`).

### Missing wording

Every supported locale must be complete, unless it is listed in
`in_progress:`. A message, a failure a route can answer with, or an enum
once any language words it, missing from such a locale is an error:
`translation.file_missing` for a locale with no language file,
`translation.wording_missing` for a file that lacks a member. The error's
`next:` names the file and the member to add, with the placeholders to use,
and ends `(agent)`: your coding agent can add it itself.

### A field no wording uses

A field that no wording in a language uses is `translation.field_unused`.
One variant leaving a field out is normal. Jadpo has no warnings, so this
is an error, but a judgement call: a person can answer it by writing
`decline translation.field_unused: "<reason>"` above the member
([Declining a judgement call](01-project-layout-and-modules.md#declining-a-judgement-call)).
That is right for a language that needs no such word.

### Locales in progress

`in_progress:` is how you say a locale isn't finished. A locale it lists
falls back where its language file has no wording, with no error: to the
same language's regional sibling first (`fr_ca` tries `fr_fr`, if
supported), then to the default locale. The sentence is written entirely in
the language it came from.

Wording the file does give is checked as for any locale, so a mistake in
it is still an error. Removing the locale from `in_progress:` makes the gaps
errors again. Quality comes from the review setting below, not from
waiting. `jadpo translations missing` lists what an in-progress locale falls
back on.

### Stale translations: `sources.jadpo_record`

A translation is stale when the wording it was translated from has changed
since. Jadpo knows from a file it writes in each `translations/` folder,
`sources.jadpo_record`, which you commit. It is a record
([The record format](18-records-and-manifest.md#the-record-format)) headed
`record translations 1`, written by `jadpo translations update`, listed by
`jadpo records` and merged by git's `union` merge.

It has one line for each translation outside the default locale, sorted,
with a hash of the default wording it was translated from (`from`) and of
its own wording when that was recorded (`wording`). A hash is the first 12
hex digits of BLAKE3 over the member's whole text, every value and plural
form in name order, so reordering a block changes nothing.

A translation whose `from` no longer matches today's default wording is
stale (`translation.stale`). That is an error until it is translated again
and recorded. An in-progress locale's stale translation is listed by `jadpo
translations missing` instead.

A translation with no line yet isn't stale: it was never recorded, so
nothing says what it was made from. (With review required, it is new and
not accepted.) So a project that never runs `jadpo translations update`
sees no stale error.

A line Jadpo can't read is `translation.sources_invalid`, which `jadpo
translations update` fixes. A header Jadpo didn't write, or lines out of
order, is `record.edited`, and a newer format is `record.format_newer`, as
for every record ([Checking a record](18-records-and-manifest.md#checking-a-record)).

### Review

By default a translation needs no one to accept it. To have a person accept
each one, set `translations: { review: TranslationReview.required }` in
`application { }`; the default is `TranslationReview.none`. Then every new
or changed wording outside the default locale is not accepted until a
person runs `jadpo translations accept <locale>`, which records the
acceptance in the same record. A line says a wording is accepted only while
the wording is the one the line hashes.

`jadpo translations missing` lists wording that isn't accepted. It isn't an
error, so `jadpo check` passes with it: your code review is the gate.
Jadpo can't tell who wrote a wording, so this means "changed and not yet
accepted", not "written by an agent".

A `translations:` option other than `review:` is
`translation.option_unknown`, and a `review:` that isn't a
`TranslationReview` is `translation.review_invalid`.

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

### Jadpo's and packages' wording

The root folder's wording of Jadpo's and packages' text is checked too,
each problem with its own error:

- **Jadpo's messages.** A supported locale that isn't in progress, in
  which Jadpo ships no wording and the root file words none of the messages
  of a feature the app has on, is one error for the locale, listing what is
  missing (`translation.jadpo_missing`) and pointing at `jadpo translations
  builtin`. A message of a feature that is off is
  `translation.feature_off`, and a group of the root file that names no
  owner is `translation.group_unknown`. Everything else about a group's
  wording is checked as a folder's is: `translation.member_unknown` (with
  the nearest of Jadpo's messages), `member_duplicate`, `wording_shape`,
  `wording_invalid`, `placeholder_unknown`, `variant_missing` and
  `variant_unknown`.
- **The root folder's record.** The root folder has its own
  `sources.jadpo_record`, kept by the same commands, with the owner in the
  member's name (`translation fr_fr.jadpo.invalid_input from ... wording
  ...`), as a package's is `postcodes.<message>`.
- **A library package's text.** Its own language files are checked by
  themselves, as a domain's are. Its group in the root file is checked
  against its public messages, enums and failures
  (`translation.member_unknown`, with the nearest of them). Each supported
  locale not in progress must word all of it in the root file or the
  package's own file (`translation.wording_missing` with a root file for
  the locale, `translation.file_missing` without). A call through the
  package of a message it doesn't have is `translation.message_unknown`,
  and of one it doesn't make `public`, `package.not_public`. A root record
  line is made from the app's default wording or, when the app gives none,
  the package's, and is stale when that changes (`translation.stale`).

## Tooling for translations
<!-- topic: tooling-for-translations -->

The `jadpo translations` commands list what is missing, keep the record of
what each translation was made from, and exchange wording with translation
services. Each takes `--json` wherever it prints. They succeed whatever
they list, because they answer a question: `jadpo check` is what fails.
`update` and `accept` stop on syntax errors.

### What is missing or unused

`jadpo translations missing [<locale>]` lists, under the language file each
is in:

- the wording a locale lacks and, for a locale in progress, what it falls
  back on;
- the translations made from wording that has since changed;
- with review required, the wording not yet accepted.

`jadpo translations unused` lists the messages, and the enums with
wording, that no code calls. A test doesn't count as a caller. Failures and
emails are used by being reachable and by the email's own sending, so they
are never listed. It also lists your app's wording of library packages'
text that shows nothing
([Packages' text in the exchange](#packages-text-in-the-exchange)).

### Recording and accepting: `update` and `accept`

`jadpo translations update` writes each folder's `sources.jadpo_record`. It
can't tell that a translation was redone, only that it changed. So a
translation with no line, or one reworded since its line, is recorded as
made from today's default wording, and loses its acceptance. One made from
wording that changed, and that wasn't reworded, stays stale with its line
untouched, so the error stays until someone translates it again. `update`
and `accept` say `still stale` for it.

`jadpo translations accept <locale>` does what `update` does for that
locale, then accepts everything of the locale's that isn't stale, so a
person doesn't run both. It takes a supported locale other than the default
and the pseudo-locale. Anything else is `cli.translations_arguments`, which
names the ones it takes.

### Go to definition

In the editor, go to definition goes from a
`translations.<domain>.<message>(...)` call to the message's type first,
then to its member in each language file, in path order. An enum's call
does the same.

### The pseudo-locale

The pseudo-locale, `Locale.en_xa`, writes every message with its letters
accented and the text made longer, for development and tests. It shows up
text that is written into the code instead of translated, and layouts that
are too tight. Its tag is the one Android and Chrome use, in a region
ISO 3166 leaves for private use, so it can't clash with a real locale.
`Locale` has 534 variants: 533 from CLDR and this one.

List it in `supported:` like any locale. Take it out before releasing, or
keep it for a staging build. It needs no language file, is never asked for
wording, and has no translations to list or accept.

It writes the default locale's wording, with that locale's plural rules and
number format, its letters accented (`a` is `á`), about two fifths longer,
in brackets, so a cut-off end shows as a missing `]`. The values a wording
writes aren't accented. A language file `en_xa.jadpo`, if one exists, is
written as it is. Failures' client text is exported in it the same way.

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

### Jadpo's own messages: `builtin`

`jadpo translations builtin`, with `--json`, lists Jadpo's own messages
your app can show, by feature. Each comes with the fields it may write, the
wording Jadpo ships to translate from, and `needs` and the locale for each
supported locale whose lack of it fails the check, the default included
(none for a locale in progress). It succeeds whatever it lists. Its summary
counts the messages, the ones that need wording (in the problem colour) and
the features. Its one hint is `jadpo check`:

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

`jadpo translations missing`, `update` and `accept` take the root folder in
with the rest. Its gaps are listed under `translations/<locale>.jadpo` as
`jadpo.<message>` (never for the default locale, which the error covers),
and an in-progress locale falls back to what Jadpo ships.

## Exchanging translations with a service
<!-- topic: exchanging-translations -->

Most teams send their wording to a translation service, such as Crowdin,
Lokalise or any that reads XLIFF or JSON. `jadpo translations export
[<locale>]` writes the files to upload, and `jadpo translations import
<file>` reads the translations back, both with `--json`. They exchange
every message, enum with wording, failure and email subject the default
locale words, Jadpo's own messages (those that differ by language), the
`///` description of each, and each locale's wording.

### Export

`export` writes one file for each translated locale (a supported locale
other than the default and the pseudo-locale), or for the one named, to
`build/translations/<locale>.xlf`. The file is the same each time, and the
next export overwrites it. `--format json` writes `<locale>.json` instead,
and `--out <folder>` writes to another folder, relative to the project and
inside it. Naming the default locale writes the source file to upload: the
same units with no translations.

A unit is one wording: a plain message, a value of the enum that picks a
wording, or a plural form of the locale. Its id is `<domain>.<message>`,
with `.<value>` or `.<form>` after it, which XLIFF splits into `<file id>`
and `<unit id>`. In place of a domain, `shared` stands for
`shared/translations/`, `jadpo` for Jadpo's own messages, and a library
package's folder name for its text. A domain named `jadpo` would share ids
with Jadpo's own messages, so export refuses it. Wording every locale
shares (`TextDirection`'s) isn't exchanged.

The wording is written as it is in the language file, `{field}`
placeholders and doubled braces included, not as XLIFF inline `<ph>`
codes. So the same text goes to both formats, comes back exactly, and a
service sees ordinary text (both services recognise `{x}` as a
placeholder). A plural form the default locale has no form of is
translated from the default's `other`.

```text
Wrote build/translations/de_de.xlf
    de_de  5 units, 5 translated

Wrote build/translations/pl_pl.xlf
    pl_pl  7 units, 7 to translate

Exported 2 locales: 12 units, xliff

next: jadpo translations import build/translations/pl_pl.xlf  reads the translations back
```

### The file formats

The XLIFF is version 2.0 (namespace
`urn:oasis:names:tc:xliff:document:2.0`), which every tool that reads 2.x
reads. It has a `<file>` for each domain and a `<unit>` for each wording.
Each unit has `description`, `context` (the enum value or plural form it is
for) and `placeholders` notes, and a `stale` note for wording translated
from default wording that has since changed. Its `<segment>` has the
`state` `initial` without a translation, `translated` with one, and
`reviewed` when a person accepted it.

The JSON is `{ "<id>": { "message": ..., "description": ... } }`, the shape
Chrome extensions use for `messages.json`. It has a place for a description
that Crowdin imports as it is ("Chrome JSON": message and description
fields). `message` is empty until translated. A flat `{ id: text }` object
is the shape both services take, but it carries no description, which the
exchange exists to send. Whether Lokalise reads XLIFF 2.0, and imports the
Chrome shape, hasn't been checked; each format is the other's fallback.

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

### Import

`import` tells a file's format by what it starts with: `<` for XLIFF 2.0
or 2.1, `{` for JSON. Plain strings are accepted for `message`, so a flat
object from a service that can't keep descriptions reads too. The locale is
the one the file names (XLIFF's `trgLang`), `--locale`, or a JSON file's
name (`pl_pl.json`). XLIFF 1.2 is refused, with a message saying to ask the
service for 2.x, since the two are built differently. Inline codes a
service adds on the way back (`<ph>`, `<pc>`) are read as the text they
stand for. A unit with no translation is left alone. A status in the file
(`reviewed`) is never read back: only a person accepts a translation, with
`jadpo translations accept`.

Import then does what a person editing the language files would have to:

- each changed wording goes into its language file where its member is,
  keeping the file's other members, comments and order, or at the end;
- a missing file is created;
- the files are laid out by `jadpo fmt`;
- the folder's `sources.jadpo_record` gets a line for each changed
  translation, made from today's default wording and not accepted. Only
  changed translations get one, so a round trip never creates or changes a
  line.

Before anything is written, the project is checked as `jadpo check` checks
it, by the checker's own rules with nothing special for the exchange. Any
new finding refuses the whole import, with that finding's own rule,
pointing at the unit of the imported file that caused it:
`translation.placeholder_unknown`, `translation.wording_invalid`,
`translation.variant_missing` for a plural form or enum value left out, or
the declinable `translation.field_unused` for a placeholder every wording
dropped.

Two errors belong to the exchange itself:

- A unit naming nothing the project words, or a plural form the locale
  doesn't use, is `translation.exchange_unit_unknown`, with the nearest id.
- A file that isn't XLIFF 2.x or JSON, isn't well formed, holds a unit
  twice or in several segments, or was translated from another default
  language is `translation.exchange_file_invalid`.

Both are [command errors](14-tooling.md#command-errors), so the command
exits 1: it ran, and the file was wrong. All the reasons are listed at
once, and nothing is written. An in-progress locale may be translated in
part, as its language file may. Exporting and importing with nothing
changed writes nothing, and says `Already up to date`.

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

### Wording that changed while it was being translated

XLIFF carries what each translation was made from. So a unit whose
`<source>` isn't today's default wording, because the English changed while
the service worked, is written all the same and recorded as made from that
other wording. It is stale, counted as `stale` in the summary, and `jadpo
check` fails with `translation.stale` until it is translated again, as for
wording changed by hand. This is the one case where import doesn't refuse,
because the work isn't lost and `translation.stale` already says what to
do. JSON doesn't carry its source, so its translations are taken as made
from today's wording.

Your app's own data, such as product names or articles, isn't a
translation: model translated content as ordinary fields. Library
packages' translations are in
[Jadpo's and packages' text](#jadpos-and-packages-text).

## Retention periods
<!-- topic: retention-periods -->

How long your app keeps deleted records is an ordinary setting, such as
`soft_delete_retention: Duration`. A lifecycle reads it as
`purge: { after: config.soft_delete_retention }`
([Lifecycle](05-data.md#lifecycle)). `after:` takes a `Duration` setting that
isn't secret (`data.lifecycle_purge_after` otherwise). A purge run uses the
value the app started with, and a period that isn't above zero deletes
nothing. A `retention:` on an entity or a field reads its period the same
way, and only from a setting: `retention: { after:
config.signup_ip_retention since: created_at }`
(`data.retention_period` otherwise). How retention fits the wider support
for personal data, such as marking personal fields, erasure and export,
is in [Personal data](15-personal-data.md#retention).

## What is built
<!-- topic: configuration-and-time-built -->

| Feature | Status |
|---|---|
| Time literals and the built-in `Zone`, `Weekday` and `Locale` enums | Built, with `clock.now` outside an action reported as `type.clock_context`. |
| Settings | Built: settings, secrets, signing keys, connection addresses, `jadpo config`, the check before the app starts, a test's own values, and the trusted proxies a client's address is read through. The check before the app starts covers every setting a deployable reads, its connections' included, among them the mail server's for the sign-in email. |
| Time | Built: `ZonedTime`, durations, arithmetic and comparison, the `temporal` library over the IANA database bundled into every target, time values on the wire, in OpenAPI and in queries, and a test's `clock:` and `clock.advance(...)` ([Controlling time](13-testing.md#controlling-time)). |
| Formatting | Built: `TimeFormat`, declared and built in, `ComponentStyle` and `PresentationText`, with the checks on formatting. |
| Locales | Built: `locales { }` with its errors, `locale:` checked against it, a `Locale` on the wire or in a setting limited to the supported ones, and the text formatting gives, over CLDR data compiled into every target; `locale.from_accept_language(header)`. |
| Translations as messages | Built: `Message` types and the built-in `PluralCount`, each domain's `translations { }` language files and their checks, the generated `translations.<domain>.<message>` calls, enum wording, email subjects and `in_progress:` locales; failures' client text exported per locale; stale translations and review; `locale.direction`; the `jadpo translations` commands; go to definition and the pseudo-locale. |
| Jadpo's own messages | Built, with the root file's owner groups. |
| Library packages' translations | Built: a package's `translations/`, `Postcodes.translations.<message>(...)`, the root file's package groups and their record lines, and a package's messages in its surface and the publishing bar. |
| Locale data a deployable carries | Built: its supported locales' only, with the text a `Locale` the app doesn't support is written in (see [Locales](#locales)). |
| Unaccepted wording in the approval report | Not built yet: `jadpo approval` doesn't exist yet. Use `jadpo translations missing`, which lists it. |
| Moving a message to `shared/translations/` | Not built yet as a promotion: `layout.promotion_required` doesn't cover messages, so move the message by hand. |
| Other members of `clock`, `translations`, `sessions` and `oauth` | Not built yet: a member these pages don't describe is `toolchain.not_built_yet` until its area is built. |

## Errors

Every error this page's settings, times, locales and translations can
produce. Each error's page has an example that triggers it and the
compiler's actual output.

- [`configuration.binding_empty`](../errors/configuration.binding_empty.md)
- [`configuration.binding_invalid`](../errors/configuration.binding_invalid.md)
- [`configuration.duplicate_binding`](../errors/configuration.duplicate_binding.md)
- [`configuration.duplicate_option`](../errors/configuration.duplicate_option.md)
- [`configuration.unknown_option`](../errors/configuration.unknown_option.md)
- [`configuration.expected_field`](../errors/configuration.expected_field.md)
- [`configuration.multiple_declarations`](../errors/configuration.multiple_declarations.md)
- [`configuration.type_unsupported`](../errors/configuration.type_unsupported.md)
- [`configuration.default_literal_required`](../errors/configuration.default_literal_required.md)
- [`configuration.default_type`](../errors/configuration.default_type.md)
- [`configuration.default_invalid`](../errors/configuration.default_invalid.md)
- [`configuration.secret_default`](../errors/configuration.secret_default.md)
- [`configuration.secret_invalid`](../errors/configuration.secret_invalid.md)
- [`configuration.secret_flow`](../errors/configuration.secret_flow.md)
- [`configuration.field_unknown`](../errors/configuration.field_unknown.md)
- [`configuration.url_type`](../errors/configuration.url_type.md)
- [`configuration.test_value_missing`](../errors/configuration.test_value_missing.md)
- [`configuration.client_address_duplicate`](../errors/configuration.client_address_duplicate.md)
- [`configuration.client_address_invalid`](../errors/configuration.client_address_invalid.md)
- [`configuration.client_address_source_unknown`](../errors/configuration.client_address_source_unknown.md)
- [`configuration.client_address_hops_invalid`](../errors/configuration.client_address_hops_invalid.md)
- [`configuration.client_address_hops_unused`](../errors/configuration.client_address_hops_unused.md)
- [`type.clock_context`](../errors/type.clock_context.md)
- [`type.temporal_clock_invalid`](../errors/type.temporal_clock_invalid.md)
- [`type.temporal_option_unused`](../errors/type.temporal_option_unused.md)
- [`type.temporal_zone_required`](../errors/type.temporal_zone_required.md)
- [`type.temporal_format_invalid`](../errors/type.temporal_format_invalid.md)
- [`type.temporal_format_requires_time`](../errors/type.temporal_format_requires_time.md)
- [`type.temporal_format_components_empty`](../errors/type.temporal_format_components_empty.md)
- [`type.temporal_format_component_invalid`](../errors/type.temporal_format_component_invalid.md)
- [`type.temporal_format_component_duplicate`](../errors/type.temporal_format_component_duplicate.md)
- [`type.presentation_text_persistence`](../errors/type.presentation_text_persistence.md)
- [`type.temporal_locales_required`](../errors/type.temporal_locales_required.md)
- [`syntax.locales_default_required`](../errors/syntax.locales_default_required.md)
- [`syntax.locales_supported_required`](../errors/syntax.locales_supported_required.md)
- [`syntax.locales_unknown_option`](../errors/syntax.locales_unknown_option.md)
- [`syntax.locales_duplicate_option`](../errors/syntax.locales_duplicate_option.md)
- [`type.locale_invalid`](../errors/type.locale_invalid.md)
- [`type.locale_duplicate`](../errors/type.locale_duplicate.md)
- [`type.locale_default_unsupported`](../errors/type.locale_default_unsupported.md)
- [`type.locale_unsupported`](../errors/type.locale_unsupported.md)
- [`semantic.multiple_locale_declarations`](../errors/semantic.multiple_locale_declarations.md)
- [`translation.file_name_invalid`](../errors/translation.file_name_invalid.md)
- [`translation.locale_unsupported`](../errors/translation.locale_unsupported.md)
- [`translation.file_missing`](../errors/translation.file_missing.md)
- [`translation.wording_missing`](../errors/translation.wording_missing.md)
- [`translation.locales_required`](../errors/translation.locales_required.md)
- [`translation.member_unknown`](../errors/translation.member_unknown.md)
- [`translation.member_duplicate`](../errors/translation.member_duplicate.md)
- [`translation.wording_shape`](../errors/translation.wording_shape.md)
- [`translation.wording_invalid`](../errors/translation.wording_invalid.md)
- [`translation.placeholder_unknown`](../errors/translation.placeholder_unknown.md)
- [`translation.placeholder_style`](../errors/translation.placeholder_style.md)
- [`translation.variant_unknown`](../errors/translation.variant_unknown.md)
- [`translation.variant_missing`](../errors/translation.variant_missing.md)
- [`translation.field_type`](../errors/translation.field_type.md)
- [`translation.too_many_selectors`](../errors/translation.too_many_selectors.md)
- [`translation.field_unused`](../errors/translation.field_unused.md)
- [`translation.message_unknown`](../errors/translation.message_unknown.md)
- [`translation.jadpo_missing`](../errors/translation.jadpo_missing.md)
- [`translation.feature_off`](../errors/translation.feature_off.md)
- [`translation.group_unknown`](../errors/translation.group_unknown.md)
- [`translation.stale`](../errors/translation.stale.md)
- [`translation.sources_invalid`](../errors/translation.sources_invalid.md)
- [`translation.option_unknown`](../errors/translation.option_unknown.md)
- [`translation.review_invalid`](../errors/translation.review_invalid.md)
- [`type.locale_in_progress_unsupported`](../errors/type.locale_in_progress_unsupported.md)
- [`type.locale_in_progress_default`](../errors/type.locale_in_progress_default.md)
