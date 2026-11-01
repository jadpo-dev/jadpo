---
title: Identity
area: 8
status: decided
checked-with: this checkout's jadpo on every gate run
---

# Identity
<!-- topic: identity -->

Identity is how Jadpo knows who is calling. You say which of your stored
records is a person and which is another system, and which ways in your
app accepts, such as a browser session or an API token. Jadpo checks the
caller on every request and gives your code one typed value for them,
`current_principal`. What that caller may do is decided separately, by
[Policy](09-policy.md).

You write it in three places, and read the caller in a fourth:

| Piece | Where | What it says |
|---|---|---|
| `principal { }` | `app.jadpo` | Which entity is a user, which is a service, and when each is active |
| `authentication { }` | `authentication.jadpo` | The ways a caller can sign in, and with `sign_in:` how a person is checked |
| `application { }` | `app.jadpo` | How long a revoked session may keep working, and the app's name in emails |
| `current_principal` | routes | The signed-in caller, passed to actions |

Five rules hold underneath all of it:

- Every route needs a signed-in caller unless it says otherwise.
- A request carries exactly one credential. A request with two, or with an
  invalid one beside a valid one, is refused; Jadpo never picks one.
- Jadpo checks the caller before it reads the request body or runs any of
  your code.
- Your code gets one typed caller. It never sees cookies, tokens, keys or a
  provider's claims.
- A credential never holds permissions. Policy decides them.

People sign in with a [magic link](#magic-links) sent by email, or with a
[passkey](#passkeys). An app can also accept tokens from an identity
provider it already uses ([Ways to sign in](#ways-to-sign-in)). Jadpo has
no password sign-in.

## The principal
<!-- topic: the-principal -->

The principal is whoever is calling, once Jadpo has checked them: either a
**user** (a person) or a **service** (another system, such as a partner's
server). You declare it once in `app.jadpo`, with no name. It says which
stored record each kind of caller is, and when that record may still sign
in:

```jadpo fragment
// app.jadpo
principal {
    user: {
        subject: User.authentication_subject
        active: User.status == UserStatus.active
    }
    service: {
        subject: Service.name
        active: Service.status == ServiceStatus.active
    }
}
```

### `subject:`

Jadpo needs to know which record a signed-in caller is. `subject:` names
the field it finds them by, such as `User.authentication_subject` or
`User.email`. The field must be unique. When your app has tenants (many
organisations sharing one app, see [Tenancy](09-policy.md#tenancy)), it
must be a field every tenant shares, so one person is one record. Jadpo
looks the caller up by this field whenever it needs their current record.

### `active:`

`active:` is a condition on the same entity, written with its fields as a
query's `where:` is. A caller whose record doesn't meet it is refused with
`user_disabled` or `service_disabled` (see [Failures](#failures)).
Leave it out and every record counts as active.

### What code reads from the caller

The principal has three fields of its own, and no others:

- `id`: the record's `id`, a `User.id` or a `Service.id`;
- `subject`: the value of the `subject:` field;
- `strength`: how the caller proved who they are, `primary` or
  `multi_factor` ([How strongly a person signed in](#how-strongly-a-person-signed-in)).

A role, a tenant or an email address isn't on the principal. Code that
needs one reads it from the entity, which is where [Policy](09-policy.md)
reads it too.

### Users only

`service:` is optional. Most apps only have users, and leave it out. An
app needs at least one of `user:` and `service:`.

## The application and revocation
<!-- topic: the-application-and-revocation -->

A person can be disabled, or a session revoked, while they still hold a
valid cookie or token. By default, Jadpo looks up the caller's record on
every request, so revoking a session or disabling a user takes effect on
the very next request. You don't write anything for that. Sessions and
tokens are random values Jadpo looks up in the database.

`application { }` in `app.jadpo` can change this with one setting,
`revocation_delay:`, and also holds the app's `display_name:`.

### Fewer database reads: `revocation_delay:`

Use it when looking up the caller's record on every request costs too
much, and a short window in which a disabled user can still act is
acceptable.

```jadpo check project=delay-alone
application {
    revocation_delay: 5m
}
```

With `revocation_delay: 5m`, Jadpo signs what it learned from the caller's
record into their session or token and trusts that for up to five minutes,
without reading the database. A user you disable may keep working for up
to five minutes. After that, the next request reads the record again.

Three kinds of request always read the record, whatever the delay:

- a request with a token from an outside identity provider
  (`external_token`, see [Ways to sign in](#ways-to-sign-in));
- a request that sends a service key itself, rather than a token exchanged
  for one;
- a request to a route marked `authentication: RouteAuthentication.fresh`
  ([Routes: signed in by default](#routes-signed-in-by-default)).

This is the only revocation setting. Leave it out for the strict
behaviour.

### How a delayed session is renewed

A credential is issued the same way with or without a delay. A session's
cookie and an API token are 32 random bytes, and Jadpo keeps only a
verifier of them, a one-way digest that can't be turned back into the
credential.

The first request with a new credential reads the record. Its response
renews the credential with a part Jadpo signs, holding what that read
found and when it stops counting: the delay after the read, or the
session's end if that is sooner. Every request after that signs in from
the signed part alone. When the part expires, the next request reads the
record again and renews it the same way.

- A browser session's renewed cookie comes back in `Set-Cookie`, which the
  browser keeps by itself.
- An API token's comes back in an `X-Jadpo-Token` response header. Your
  client keeps it and sends it from then on. A client that ignores it still
  signs in, but reads the record on every request.
- A token a service gets from `POST /auth/exchange` (see
  [Exchanging a service key for a token](#exchanging-a-service-key-for-a-token))
  is signed when it is given and never renewed. When it expires, the
  service exchanges its key again.

### Signing keys

With a delay, each way of signing in needs a `signing_key:`, which reads a
`SigningKey` setting from your configuration
([Signing keys](10-configuration-and-time.md#signing-keys)). New parts are
signed with the setting's current value, and parts signed with its
`_PREVIOUS` value still count, so you can rotate a key without signing
anyone out.

A signed part the key doesn't open, or one that has expired, is never
refused for that. The request reads the record, as it would with no delay.

### The app's name: `display_name:`

`display_name:` is your app's name as people see it, such as `"Notes"`.
Jadpo's own emails name your app by it, so it is required once you switch
on magic links or another
[built-in email](16-built-in-services.md).

### A whole app with a delay

The settings, the delay, a user and a service, and each way of signing in
with its key:

```jadpo check project=revocation-delay file=config.jadpo
config {
    browser_origin: Url
    session_signing_key: SigningKey
}
```

```jadpo check project=revocation-delay file=app.jadpo
use journal/entities { Member, Partner }
use journal/types { MemberStatus, PartnerStatus }

application {
    revocation_delay: 5m
}

principal {
    user: {
        subject: Member.handle
        active: Member.status == MemberStatus.active
    }
    service: {
        subject: Partner.name
        active: Partner.status == PartnerStatus.active
    }
}
```

```jadpo check project=revocation-delay file=authentication.jadpo
authentication {
    browser_session: {
        cookie: "notes_session"
        signing_key: config.session_signing_key
        origin: config.browser_origin
    }
    service_key: {
        signing_key: config.session_signing_key
    }
}
```

```jadpo check project=revocation-delay file=domains/journal/types/handle.jadpo hidden
type Handle = Text {
    min_length: 3
    max_length: 40
}
```

```jadpo check project=revocation-delay file=domains/journal/types/member_status.jadpo hidden
type MemberStatus = Enum {
    active
    suspended
}
```

```jadpo check project=revocation-delay file=domains/journal/types/partner_status.jadpo hidden
type PartnerStatus = Enum {
    active
    suspended
}
```

```jadpo check project=revocation-delay file=domains/journal/types/partner_view.jadpo hidden
use journal/entities { Partner }

type PartnerView = Output {
    pick: [Partner.name]
}
```

```jadpo check project=revocation-delay file=domains/journal/entities/member.jadpo hidden
use journal/types { Handle, MemberStatus }

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
    status: MemberStatus

    policy: {
        read: [id]
    }
}
```

```jadpo check project=revocation-delay file=domains/journal/entities/partner.jadpo hidden
use journal/entities { Member }
use journal/types { Handle, PartnerStatus }

entity Partner {
    id: Uuid
    owner_id: Member.id {
        on_delete: OnDelete.restricted
    }
    name: Handle {
        unique: true
    }
    status: PartnerStatus

    policy: {
        read: [id, owner_id]
    }
}
```

```jadpo check project=revocation-delay file=domains/journal/queries/this_partner.jadpo hidden
use journal/entities { Partner, PartnerNotFound }
use journal/types { PartnerView }

/// The partner a key or an exchanged token belongs to.
query this_partner(partner: Principal.service) fails PartnerNotFound -> PartnerView {
    from: Partner
    where: Partner.id == partner.id
    missing: PartnerNotFound
}
```

A partner signs in with its key, which always reads the record, or with
the token the exchange gave it for that key, which reads nothing until it
expires. A `fresh` route reads the record either way:

```jadpo check project=revocation-delay file=domains/journal/routes/partners.jadpo migrate=initial
use journal/types { PartnerView }
use journal/queries { this_partner }

/// What a partner reads, signed in with its key or a token the exchange gave.
routes Partners {
    prefix: /partners

    /// Signs in without a read while the token's signed part lasts.
    GET /me {
        output: PartnerView
        run: this_partner(current_principal.service)
    }

    /// Reads the partner's record now, whatever the token says.
    GET /me/fresh {
        authentication: RouteAuthentication.fresh
        output: PartnerView
        run: this_partner(current_principal.service)
    }
}
```

The API record, the file `jadpo api update` writes to list each API's
routes ([The public API record](07-http.md#the-public-api-record)), lists
the exchange Jadpo answers, as `jadpo routes` and OpenAPI do
([Generated routes](07-http.md#generated-routes)):

```jadpo check project=revocation-delay file=api/main.jadpo_record
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

api main
    authentication browser_session
    authentication service_key

POST /auth/exchange
    generated exchange
    caller service
    output ExchangedToken
    status 200

GET /openapi.json
    generated openapi
    status 200

GET /partners/me
    caller service
    output PartnerView
    status 200
    fails partner_not_found

GET /partners/me/fresh
    caller service
    output PartnerView
    status 200
    fails partner_not_found

output ExchangedToken
    expires_at: Instant
    token: ServiceToken

output PartnerView
    name: Text { min_length: 3, max_length: 40 }

failure partner_not_found
    status 404
```

## Ways to sign in
<!-- topic: ways-to-sign-in -->

`authentication { }` in `authentication.jadpo` lists the ways your app
lets a caller prove who they are. Jadpo has four, each a block with its
own name, and each decides where the caller's credential travels. A web
app starts with `browser_session`. Add `api_token` for your own mobile or
command-line clients, and `service_key` for other systems:

| Way in | Credential | Who | Use it for |
|---|---|---|---|
| `browser_session` | A cookie the browser keeps (`HttpOnly`, `Secure`, `SameSite=Strict`) | users | Your web app's pages |
| `api_token` | `Authorization: Bearer` header | users | Your own mobile or command-line clients |
| `external_token` | `Authorization: Bearer` JWT from an identity provider | users | An identity provider you already use, such as your company's single sign-on |
| `service_key` | `Authorization: Bearer` header | services | Other systems, such as a partner's server |

Every way signs in a caller of a kind the [principal](#the-principal)
declares. The principal, declared once in `app.jadpo`, says which stored
record is a user and which is a service. Its `user:` part is what
`browser_session`, `api_token` and `external_token` sign in, and its
`service:` part is what `service_key` signs in.

An app that accepts all four looks like this. Each `config.<name>` is a
setting declared in `config.jadpo`
([Settings](10-configuration-and-time.md#settings-configjadpo)), with
these types:

- `session_signing_key` is a `SigningKey`;
- `browser_origin` and `oidc_issuer` are each a `Url`;
- `oidc_audience` is a `Text`.

```jadpo fragment
// authentication.jadpo
authentication {
    browser_session: {
        cookie: "todo_session"
        signing_key: config.session_signing_key
        origin: config.browser_origin
    }
    api_token: {
        signing_key: config.session_signing_key
    }
    external_token: {
        issuer: config.oidc_issuer
        audience: config.oidc_audience
    }
    service_key: {
        signing_key: config.session_signing_key
    }
}
```

The `signing_key:` lines are only needed with a revocation delay, which
the next section explains. Without one, the smallest setup for a web app
is `browser_session` with its `cookie:`.

A bearer token is never accepted from the URL. The cookie and the
`Authorization` header are never ordinary route input, so a route can't
read them.

### `signing_key:`

By default, sessions, API tokens and service keys are random values that
Jadpo looks up in the database on every request, and nothing is signed.
An app can set a
[`revocation_delay:`](#fewer-database-reads-revocation_delay) in
`application { }` to save those reads. With a delay, Jadpo signs what it
read from the caller's record into their session or token and trusts it
for that long, so a user you disable may keep working until the delay
runs out ([How a delayed session is renewed](#how-a-delayed-session-is-renewed)).

The signing key is what Jadpo signs that part with. So once you set a
delay, `browser_session`, `api_token` and `service_key` each need a
`signing_key:`. Without a delay the key is never used, and it does no
harm to leave it written. That way you can switch the delay off by
removing its one line, and back on by adding it.

`signing_key:` reads a `SigningKey` setting, which holds a secret
([Signing keys](10-configuration-and-time.md#signing-keys)). A key written
in your source is a compile error.

### `browser_session`

A browser session is a cookie your app's pages send with every request.

- `cookie:` names the cookie: 1 to 64 letters, digits, `_` and `-`. The
  session's token travels in it, `HttpOnly`, `Secure` and
  `SameSite=Strict`, for the whole site.
- A second cookie, the same name with `_csrf` after it, holds the
  session's CSRF token. A CSRF token stops another site from making the
  browser send a write on the person's behalf. Your pages read this cookie
  and send its value back in an `X-Jadpo-CSRF` header on every write.
- A session lasts 30 days from when it starts.

`origin:` reads a `Url` setting: the scheme, host and port your app's
pages are served from, such as `https://notes.example`. A write that comes
with the cookie must carry exactly that `Origin` header. Jadpo compares it
with the setting, not with the request's `Host`, so a proxy in front of
your server changes nothing. Leave `origin:` out and a write's `Origin`
must name the server its `Host` header gives.

[Magic links](#magic-links) and [passkeys](#passkeys), the two ways a
person signs in to start a browser session, need `origin:`.

### `api_token` and `service_key`

An API token signs in a user, so the app's principal must declare
`user:`. A service key signs in a service, so the principal must declare
`service:`. Inside `authentication { }`, each block takes only
`signing_key:`. Without a revocation delay there is nothing to set, so
you write `api_token: {}` and `service_key: {}`.

Each travels as `Authorization: Bearer <value>`. An API token is
`jadpo_token_` and 64 hexadecimal digits, and a service key `jadpo_key_`
and 64. The prefix tells Jadpo which it was sent, and lets secret scanners
find one that leaks. Both are 32 random bytes. Jadpo keeps only their
BLAKE3 digests, one-way fingerprints that can't be turned back into the
credential, as it does for a session's cookie.

Your app hands out API tokens and service keys from its own actions, the
operations that read and change its data
([Functions and actions](03-logic.md#functions-and-actions)).
See [API tokens](#api-tokens) and [Service credentials](#service-credentials).

### `external_token`

Use it when people already sign in with an identity provider, and your
app should accept the JWTs it issues. A JWT is a signed token that names
the person, its subject.

`issuer:` reads a `Url` setting and `audience:` a `Text` setting. Jadpo
compares each exactly with what the token says. It accepts EdDSA, RS256
and ES256 JWTs, using the JWT support in the
[standard library](19-standard-library.md).

A token is accepted for a subject your app already has a user for, or
one [`on_first_sign_in:`](#a-person-the-app-has-not-seen) has linked to
a user. Without it, a subject no user has is refused with
`sign_in_refused` (403).

Only the subject is used. Every request reads that user's current record,
which must still meet the principal's `active:` condition and be a record
the entity's lifecycle shows. Profile and permission claims from the
provider are never stored as the user's identity, and never become
policy grants.

To check a token's signature, Jadpo fetches two documents from the
provider itself: its discovery document, a JSON file at a fixed address
under the issuer that says where its keys are, and its key set (JWKS),
the public keys it signs tokens with. An app that doesn't declare
`external_token` makes no such requests.

An invalid token is 401. Invalid provider or configuration data is
`authentication_misconfigured` (500), and a provider Jadpo can't reach is
`authentication_unavailable` (503). No response shows the provider's
content or a credential.

The limits on these fetches are at the end of this topic.

### Exchanging a service key for a token

With a revocation delay and service keys, Jadpo generates
`POST /auth/exchange`, which trades a key for a short-lived token. A
service sends its key as `Authorization: Bearer` and nothing else, and
gets `{ "token": ..., "expires_at": ... }`.

The token is `jadpo_service_token_` and a signed part. It lasts as long
as the revocation delay, or until the key expires if that is sooner. It signs the service in on every
route its key would. Jadpo reads the key together with the service's
record when it exchanges it, so a disabled service is refused at the
exchange.

You can't write a route of your own at `POST /auth/exchange`: one is the
compile error `route.path_overlap`.

### Where Jadpo fetches the provider's keys

For `external_token`, Jadpo fetches only from addresses you configured:

- The configured issuer and the key address it names must use HTTPS,
  with no redirects, and only addresses on the public internet.
- The discovery document must name exactly the configured issuer,
  including its trailing slash. Its `jwks_uri`, the address of the key
  set, may name another public HTTPS host.
- The token never supplies an address to fetch.
- Discovery adds `/.well-known/openid-configuration` after the issuer's
  path, removing a final slash for that request only
  ([OpenID Connect Discovery §§3, 4.1, 4.3](https://openid.net/specs/openid-connect-discovery-1_0.html)).
- A native build checks the addresses DNS actually returns, and refuses
  any that isn't public. On Cloudflare Workers, Jadpo sends the fetches
  with Workers' `global_fetch_strictly_public` flag, which keeps them to
  the public internet.

### Limits on the provider's keys

Each running instance of the app keeps one cache of the provider's keys:

- at most 64 keys, a 64 KiB discovery document and a 256 KiB key set;
- 3 seconds for each fetch;
- keys kept for 5 minutes, and at most one refresh every 30 seconds. A
  refresh counts even when the request that started it is cancelled.

The route's own time limit also cancels discovery and key requests.
Encryption keys in the set are ignored, and so are keys whose `key_ops`
leaves out `verify`, as keys that can't check a signature. The signing
keys must meet the standard library's strict `Jwk` rules. A key with a
private key's or a shared secret's members still refuses the whole set
as `authentication_misconfigured`.

### External tokens in tests and `jadpo dev`

Tests and `jadpo dev` never fetch from the configured issuer. Each runs a
local issuer in its place, with a key made for that run, and accepts its
tokens with the configured `issuer:` and `audience:`. In a route test,
`as: Principal.user(staff)` sends a token from it for that user's
subject, so the request goes through the same checks as a real token:
the user must exist, be `active:` and be shown by the lifecycle. In
`jadpo dev`, `jadpo dev token <user-id>` prints a bearer token for that
user's subject, for running a frontend or `curl` against the app without
the provider. Neither key is ever accepted by a deployable.

Here the `staff` API takes only the provider's tokens, and `main` takes
browser sessions. The route test signs a librarian in on `staff` with
`as:`, as it would on any other API:

```jadpo check project=staff-tokens file=app.jadpo
use desk/entities { Librarian }

application {
    display_name: "Lending desk"
    apis: {
        main: {
            prefix: /
            authentication: [AuthenticationMethod.browser_session]
        }
        staff: {
            prefix: /staff
            authentication: [AuthenticationMethod.external_token]
        }
    }
}

principal {
    user: {
        subject: Librarian.subject
        active: Librarian.active == true
    }
}
```

```jadpo check project=staff-tokens file=config.jadpo hidden
config {
    audience: Text {
        test: "lending-desk"
    }
    issuer: Url {
        default: "https://accounts.google.com"
        test: "https://id.lending.example/"
    }
}
```

```jadpo check project=staff-tokens file=authentication.jadpo
authentication {
    browser_session: {
        cookie: "desk_session"
    }
    external_token: {
        issuer: config.issuer
        audience: config.audience
    }
}
```

```jadpo check project=staff-tokens file=domains/desk/entities/librarian.jadpo hidden migrate=initial
entity Librarian {
    id: Uuid
    subject: Text {
        unique: true
    }
    name: Text
    active: Bool

    policy: {
        create: [Access.system]
        read: [id]
    }
}
```

```jadpo check project=staff-tokens file=domains/desk/failures/not_a_librarian.jadpo hidden
/// The caller isn't on the staff.
type NotALibrarian = Failure {
    kind: FailureKind.not_found
}
```

```jadpo check project=staff-tokens file=domains/desk/failures/subject_taken.jadpo hidden
/// Someone else already signs in with that subject.
type SubjectTaken = Failure {
    kind: FailureKind.conflict
}
```

```jadpo check project=staff-tokens file=domains/desk/types/librarian_card.jadpo hidden
use desk/entities { Librarian }

type LibrarianCard = Output {
    pick: [Librarian.id, Librarian.name]
}
```

```jadpo check project=staff-tokens file=domains/desk/types/new_librarian.jadpo hidden
use desk/entities { Librarian }

/// Someone to take on, under the subject their provider gives them.
type NewLibrarian = Input {
    subject: Librarian.subject
    name: Librarian.name
    active: Librarian.active
}
```

```jadpo check project=staff-tokens file=domains/desk/actions/take_on.jadpo hidden
use desk/entities { Librarian }
use desk/types { NewLibrarian }
use desk/failures { SubjectTaken }

/// Takes someone on.
action take_on(input: NewLibrarian) fails SubjectTaken -> Librarian {
    return attempt Librarian.create {
        set: {
            subject: input.subject
            name: input.name
            active: input.active
        }
        conflict: {
            Librarian.subject: SubjectTaken
        }
    }
}
```

```jadpo check project=staff-tokens file=domains/desk/queries/me.jadpo hidden
use desk/entities { Librarian }
use desk/types { LibrarianCard }
use desk/failures { NotALibrarian }

/// The signed-in member of staff.
query me(user: Principal.user) fails NotALibrarian -> LibrarianCard {
    from: Librarian
    where: Librarian.id == user.id
    missing: NotALibrarian
}
```

```jadpo check project=staff-tokens file=domains/desk/routes/staff.jadpo hidden
use desk/types { LibrarianCard }
use desk/queries { me }

/// What staff reach with their provider's token.
routes Staff {
    api: staff
    prefix: /staff
    GET /me {
        output: LibrarianCard
        run: me(current_principal.user)
    }
}
```

```jadpo check project=staff-tokens file=api/staff.jadpo_record hidden
// Generated by Jadpo from the routes of API staff; update it with `jadpo api update`, never by hand.
record api 1

api staff
    authentication external_token

GET /staff/me
    caller user
    output LibrarianCard
    status 200
    fails not_a_librarian

GET /staff/openapi.json
    generated openapi
    status 200

output LibrarianCard
    id: Uuid
    name: Text

failure not_a_librarian
    status 404
```

```jadpo check project=staff-tokens file=domains/desk/routes/staff.test.jadpo
use desk/entities { Librarian }
use desk/types { NewLibrarian }
use desk/actions { take_on }

test "staff sign in with a token from the local issuer" {
    var ann: Librarian = attempt take_on(
        NewLibrarian {
            subject: "google-ann"
            name: "Ann"
            active: true
        }
    )
    var own_card: Response = request {
        method: HttpMethod.get
        path: "/staff/me"
        as: Principal.user(ann)
    }
    assert own_card.status == 200
}

test "someone no longer active is refused" {
    var bo: Librarian = attempt take_on(
        NewLibrarian {
            subject: "google-bo"
            name: "Bo"
            active: false
        }
    )
    var own_card: Response = request {
        method: HttpMethod.get
        path: "/staff/me"
        as: Principal.user(bo)
    }
    assert own_card.status == 403
}

test "a request with no token is refused" {
    var own_card: Response = request {
        method: HttpMethod.get
        path: "/staff/me"
    }
    assert own_card.status == 401
}
```

Not built yet: with `external_token`, a principal `subject:` field made
of several columns, or of a structured value, is
`toolchain.not_built_yet`.

## Checking the person: `sign_in:`
<!-- topic: checking-the-person-sign-in -->

Signing in has two steps. First Jadpo checks the person is who they say,
for example that they can read email sent to their address. Then it gives
them a session: a cookie for a browser, or an API token for your own
client. `sign_in:` in `authentication { }` lists the ways Jadpo checks a
person for you, so your app writes no sign-in code:

```jadpo fragment
// authentication.jadpo
authentication {
    browser_session: {
        cookie: "notes_session"
        origin: config.browser_origin
    }
    api_token: {}
    sign_in: {
        magic_link: {
            address: Member.email
            link: "/sign-in/confirm"
        }
        providers: [google, github]
    }
    on_first_sign_in: accounts.create_member
}
```

`sign_in:` holds up to three ways in:

- `magic_link:`, an email with a link and a code ([Magic links](#magic-links));
- `passkey:`, a key held on the person's device ([Passkeys](#passkeys));
- `providers:`, sign-in with another service such as Google or GitHub.

Each way ends by starting a session in the same way.

`on_first_sign_in:` names an action in your app, here `create_member` in
the `accounts` domain. It decides what happens the first time someone
signs in whom your app has no user for, whichever way they came in, an
external token included. See
[A person the app has not seen](#a-person-the-app-has-not-seen).

You never write a magic-link action. `sessions.start` is only for a person
your app checks some other way, such as one accepting an invitation it
sent ([Starting and ending a session](#starting-and-ending-a-session)).

### Providers

Each sign-in provider is a connection in `app.jadpo`'s `connections:`
([Integrations](11-integrations.md)), and `providers: [...]` names those
connections, as `email:` names its connection. Google, Microsoft and
other OpenID Connect providers are each `driver: Driver.oidc`, and the
provider's discovery document gives the rest. GitHub and Apple aren't
plain OpenID Connect, so they have the built-in drivers `Driver.github`
and `Driver.apple`.

```jadpo fragment
// app.jadpo
connections: {
    google: {
        driver: Driver.oidc
        issuer: config.google_issuer
        client_id: config.google_client_id
        credential: config.google_client_secret
    }
}
```

- `issuer:` reads a `Url` setting. Jadpo compares it exactly with the
  issuer the provider's discovery document names, and with every ID
  token's. Google's is `https://accounts.google.com`. Microsoft's is
  `https://login.microsoftonline.com/<tenant-id>/v2.0` for one
  organisation's accounts, or the consumer tenant's,
  `9188040d-6c67-4c5b-b112-36a304b66dad`, for personal accounts. Its
  `common` and `organizations` addresses publish an issuer with a
  placeholder in it, which no exact comparison matches, so they are
  refused as `authentication_misconfigured`.
- `client_id:` reads a `Text` setting: the client ID the provider gave
  your app. Every ID token must name it as its audience.
- `credential:` reads a secret setting: the client secret
  ([Secrets](10-configuration-and-time.md#secrets)).
- `organisation:`, optional, reads a `Text` setting: the company whose
  accounts alone it signs in ([Company accounts](#company-accounts)).
- Register `<origin>/auth/<name>/callback` with the provider as the
  redirect address, `<origin>` being the browser session's `origin:`.

`SignInMethod` has a value for each provider, its connection's name, so
`on_first_sign_in:`'s action can tell `SignInMethod.google` from
`SignInMethod.microsoft`.

A provider signs a person in to a browser session, so `providers:` needs
`browser_session:` with its `origin:`. It also needs `on_first_sign_in:`.
A provider account finds its user only through the link a first sign-in
made ([A person the app has not seen](#a-person-the-app-has-not-seen)),
never by the principal's `subject:` field or an email address, so without
the action no one could sign in.

#### Company accounts

A company's staff sign in with their work accounts. `organisation:` keeps
a provider to that company's accounts, so an account outside it, a
personal Gmail address among them, can't sign in or be linked:

```jadpo fragment
// app.jadpo
connections: {
    google: {
        driver: Driver.oidc
        issuer: config.google_issuer
        client_id: config.google_client_id
        credential: config.google_client_secret
        organisation: config.google_domain
    }
}
```

The setting is checked on every sign-in against the claim the provider's
ID token names the account's company by:

- Google's `hd`, the Google Workspace domain (`desk.example`), for the
  issuer `https://accounts.google.com`. Google's own page then offers
  only that domain's accounts.
- Microsoft's `tid`, the tenant ID, for a tenant's issuer,
  `https://login.microsoftonline.com/<tenant-id>/v2.0`. The issuer
  already names one tenant; `tid` is checked as well.

An account whose token names another company, or none, is refused with
`sign_in_refused`. A provider whose tokens name no company can't keep to
one, so `organisation:` on a connection whose issuer setting's `default:`
or `test:` is another provider's is `connection.key_invalid`; one whose
issuer is only known when the app starts is
`authentication_misconfigured` at sign-in.

#### The routes

Jadpo generates two routes for each provider, in the first API that
accepts browser sessions:

- `POST /auth/<name>/start` needs no one signed in, but its `Origin` must
  be the browser session's origin. It takes `{ "return_to": "/books" }`,
  a path on your site, or an empty body for `/`. It answers
  `{ "address": ... }`, the provider's sign-in address, which your page
  sends the browser to. It also sets the `<cookie>_provider` cookie
  (`HttpOnly`, `Secure`, `SameSite=Lax`, ten minutes), which binds the
  sign-in to this browser.
- `GET /auth/<name>/callback` is where the provider sends the browser
  back. Jadpo checks the sign-in and starts a browser session. Then it
  answers `303` to `return_to`.

Every refusal also answers `303` to `return_to`, with `?error=<code>`
after it, so your page can say what happened:

| Code | When |
|---|---|
| `sign_in_provider_failed` | The sign-in expired, was used already, came back to another browser, or the provider refused it or sent an ID token that doesn't check |
| `sign_in_refused` | `on_first_sign_in:` refused the person, or their account is outside the connection's `organisation:` |
| `user_disabled` | The person's user isn't `active:` |
| `authentication_unavailable` | The provider or the database can't be reached |
| `authentication_misconfigured` | The provider's settings or its discovery document are invalid |

Your page then reads the session with its own requests. The session's
cookie is `SameSite=Strict`, so a browser doesn't send it on the
navigation that came back from the provider.

#### What Jadpo checks

- The authorization code flow with PKCE (`S256`), a `state` that works
  once, for ten minutes, in the browser that started it, and a `nonce`
  the ID token must carry. Jadpo keeps the `state` only as a digest, and
  works out the PKCE verifier and the nonce from the binding cookie, so
  the database holds neither.
- The code is exchanged at the provider's token endpoint with the client
  secret. The ID token's signature is checked against the provider's
  keys, fetched as an external token's are, under the same limits
  ([Where Jadpo fetches the provider's keys](#where-jadpo-fetches-the-providers-keys)).
  So are its exact issuer, its audience (`client_id:`, with `azp` naming
  it when there are several), its expiry and its nonce.
- Only the token's `sub`, `email`, `email_verified` and `name` are read,
  into `FirstSignIn`. None is ever stored as the user's identity, and no
  claim becomes a grant. `address_verified` is `true` only when the
  provider says `email_verified: true`. Microsoft doesn't say it, so an
  action can't link a Microsoft account by address.

#### In tests

A route test has `test_providers`, which signs in through the provider
routes for real, with the test run's local issuer as the provider. Only
tests can call it:

| Operation | Does |
|---|---|
| `test_providers.sign_in(method: SignInMethod, account: TestAccount) -> Response` | Starts the sign-in through `POST /auth/<name>/start` from the app's own page. Then the local issuer signs `account` in and sends the browser back to `GET /auth/<name>/callback` with the binding cookie. It gives the callback's `Response`, whose `code` is the `error` its redirect names, if any |

`TestAccount` holds what a provider says about the person: `subject:
Text`, `email: Email?`, `email_verified: Bool` and `name: Text?`, and two
fields a test may leave out: `organisation: Text?`, the company its token
names ([Company accounts](#company-accounts)), and `strength:
AuthenticationStrength?`, `multi_factor` for a token whose `amr` names a
second factor. Only a test, fixture or seed builds one. The session it
starts records the provider, as a real sign-in's does.

Here a lending desk's staff sign in with Google. A member of staff is
taken on under their address, and `on_first_sign_in:` links the Google
account whose address Google has verified to them. The route test signs
Ann in through Google twice, the second time through the link, and shows
an unverified address and a Microsoft account refused:

```jadpo check project=google-staff file=app.jadpo
use desk/entities { Librarian }

application {
    display_name: "Lending desk"
    connections: {
        google: {
            driver: Driver.oidc
            issuer: config.google_issuer
            client_id: config.google_client_id
            credential: config.google_client_secret
        }
        microsoft: {
            driver: Driver.oidc
            issuer: config.microsoft_issuer
            client_id: config.microsoft_client_id
            credential: config.microsoft_client_secret
        }
    }
}

principal {
    user: {
        subject: Librarian.email
        active: Librarian.active == true
    }
}
```

```jadpo check project=google-staff file=authentication.jadpo
use desk

authentication {
    browser_session: {
        cookie: "desk_session"
        origin: config.site
    }
    sign_in: {
        providers: [google, microsoft]
    }
    on_first_sign_in: desk.link_staff
}
```

```jadpo check project=google-staff file=domains/desk/actions/link_staff.jadpo
use desk/entities { Librarian }
use desk/failures { NotOnTheStaff }
use desk/queries { staff_member }

/// Links a Google account to the member of staff with its address, once
/// Google has verified it.
action link_staff(first: FirstSignIn) fails NotOnTheStaff -> Librarian.id {
    if (first.method != SignInMethod.google or not first.address_verified) {
        reject NotOnTheStaff {}
    }
    match first.address {
        none -> {
            reject NotOnTheStaff {}
        }
        some(address) -> {
            var librarian: Librarian = attempt staff_member(address)
            return librarian.id
        }
    }
}
```

```jadpo check project=google-staff file=domains/desk/routes/staff.test.jadpo
use desk/entities { Librarian }
use desk/types { NewLibrarian }
use desk/actions { take_on }

test "staff sign in with Google under the address they were taken on with" {
    var ann: Librarian = attempt take_on(
        NewLibrarian {
            email: "ann@desk.example"
            name: "Ann"
        }
    )
    var first: Response = test_providers.sign_in(
        SignInMethod.google,
        TestAccount {
            subject: "110248495921238986420"
            email: "ann@desk.example"
            email_verified: true
            name: "Ann Lee"
        }
    )
    assert first.status == 303
    assert first.code == none
    var second_sign_in: Response = test_providers.sign_in(
        SignInMethod.google,
        TestAccount {
            subject: "110248495921238986420"
            email: none
            email_verified: false
            name: none
        }
    )
    assert second_sign_in.code == none
    var own_card: Response = request {
        method: HttpMethod.get
        path: "/staff/me"
        as: Principal.user(ann)
    }
    assert own_card.status == 200
}

test "an address Google hasn't verified signs no one in" {
    attempt take_on(
        NewLibrarian {
            email: "bo@desk.example"
            name: "Bo"
        }
    )
    var refused: Response = test_providers.sign_in(
        SignInMethod.google,
        TestAccount {
            subject: "2001"
            email: "bo@desk.example"
            email_verified: false
            name: none
        }
    )
    assert refused.status == 303
    assert refused.code == "sign_in_refused"
}

test "a Microsoft account isn't linked by its address" {
    attempt take_on(
        NewLibrarian {
            email: "cy@desk.example"
            name: "Cy"
        }
    )
    var refused: Response = test_providers.sign_in(
        SignInMethod.microsoft,
        TestAccount {
            subject: "AAAAAAAAAAAAAAAAAAAAAPv0"
            email: "cy@desk.example"
            email_verified: true
            name: "Cy"
        }
    )
    assert refused.code == "sign_in_refused"
}
```

```jadpo check project=google-staff file=config.jadpo hidden
config {
    google_client_id: Text {
        test: "desk-google"
    }
    google_client_secret: Text {
        secret: true
    }
    google_issuer: Url {
        default: "https://accounts.google.com"
    }
    microsoft_client_id: Text {
        test: "desk-microsoft"
    }
    microsoft_client_secret: Text {
        secret: true
    }
    microsoft_issuer: Url {
        test: "https://login.microsoftonline.com/7f1d3a5e-8c2b-4f6a-9d0e-2b4c6a8e0f13/v2.0"
    }
    site: Url {
        test: "https://desk.example"
    }
}
```

```jadpo check project=google-staff file=domains/desk/entities/librarian.jadpo hidden
entity Librarian {
    id: Uuid
    email: Email {
        unique: true
    }
    name: Text
    active: Bool

    policy: {
        create: [Access.system]
        read: [id, Access.system]
    }
}
```

```jadpo check project=google-staff file=domains/desk/failures/not_on_the_staff.jadpo hidden
/// The person isn't on the staff.
type NotOnTheStaff = Failure {
    kind: FailureKind.not_found
}
```

```jadpo check project=google-staff file=domains/desk/failures/already_on_the_staff.jadpo hidden
/// Someone on the staff has that address already.
type AlreadyOnTheStaff = Failure {
    kind: FailureKind.conflict
}
```

```jadpo check project=google-staff file=domains/desk/types/new_librarian.jadpo hidden
use desk/entities { Librarian }

/// Someone to take on, by the address they sign in with.
type NewLibrarian = Input {
    email: Librarian.email
    name: Librarian.name
}
```

```jadpo check project=google-staff file=domains/desk/types/librarian_card.jadpo hidden
use desk/entities { Librarian }

type LibrarianCard = Output {
    pick: [Librarian.id, Librarian.name]
}
```

```jadpo check project=google-staff file=domains/desk/queries/staff_member.jadpo hidden
use desk/entities { Librarian }
use desk/failures { NotOnTheStaff }

/// The member of staff with an address.
query staff_member(address: Email) fails NotOnTheStaff -> Librarian {
    from: Librarian
    where: Librarian.email == address
    missing: NotOnTheStaff
}
```

```jadpo check project=google-staff file=domains/desk/queries/me.jadpo hidden
use desk/entities { Librarian }
use desk/types { LibrarianCard }
use desk/failures { NotOnTheStaff }

/// The signed-in member of staff.
query me(user: Principal.user) fails NotOnTheStaff -> LibrarianCard {
    from: Librarian
    where: Librarian.id == user.id
    missing: NotOnTheStaff
}
```

```jadpo check project=google-staff file=domains/desk/actions/take_on.jadpo hidden
use desk/entities { Librarian }
use desk/types { NewLibrarian }
use desk/failures { AlreadyOnTheStaff }

/// Takes someone on, to sign in with Google under their address.
action take_on(input: NewLibrarian) fails AlreadyOnTheStaff -> Librarian {
    return attempt Librarian.create {
        set: {
            email: input.email
            name: input.name
            active: true
        }
        conflict: {
            Librarian.email: AlreadyOnTheStaff
        }
    }
}
```

```jadpo check project=google-staff file=domains/desk/routes/staff.jadpo hidden migrate=initial
use desk/types { LibrarianCard }
use desk/queries { me }

/// What a signed-in member of staff reaches.
routes Staff {
    prefix: /staff
    GET /me {
        output: LibrarianCard
        run: me(current_principal.user)
    }
}
```

```jadpo check project=google-staff file=api/main.jadpo_record hidden
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

api main
    authentication browser_session

GET /auth/google/callback
    generated provider sign-in
    authentication none
    redirect return_to
    status 303

POST /auth/google/start
    generated provider sign-in
    authentication none
    input ProviderSignInStart (body)
    output ProviderSignInAddress
    status 200

GET /auth/microsoft/callback
    generated provider sign-in
    authentication none
    redirect return_to
    status 303

POST /auth/microsoft/start
    generated provider sign-in
    authentication none
    input ProviderSignInStart (body)
    output ProviderSignInAddress
    status 200

GET /openapi.json
    generated openapi
    status 200

GET /staff/me
    caller user
    output LibrarianCard
    status 200
    fails not_on_the_staff

output LibrarianCard
    id: Uuid
    name: Text

output ProviderSignInAddress
    address: Url

input ProviderSignInStart
    return_to: Text optional

failure not_on_the_staff
    status 404
```

#### In `jadpo dev`

`jadpo dev` never reaches the provider. Its local issuer
([External tokens in tests and `jadpo dev`](#external-tokens-in-tests-and-jadpo-dev))
stands in for every provider, so the provider's settings aren't needed:
any of `issuer:`, `client_id:` and `credential:` that nothing gives gets a
stand-in. `POST /auth/<name>/start` answers with the address of the
local issuer's account-chooser page, which `jadpo dev` serves on its own
port on your machine; the ready screen gives it on its `sign-in` line.

The page lists the provider's test accounts. Each is one of the
company's, when the connection has `organisation:`, and signs in with a
second factor, so the staff roles that need them can be tried. Choosing
one signs in as it, as the provider's own page would: the browser goes back to
`GET /auth/<name>/callback` with a code, and the route checks it as it
checks the provider's. **Add a test account** takes what a provider says
about a person, `TestAccount`'s fields: a subject (made up in Google's
shape until you change it), an email, whether the provider verified it
and a name; adding one signs in as it. **Cancel sign-in** answers as a
refusal at the provider does, so the callback's redirect names
`sign_in_provider_failed`. The accounts are kept in
`build/dev/accounts.json`, by provider, for the next session.

The page works from the keyboard and a screen reader alone: each account
is a button, each field has its label and its problem beside it, and the
first account, or the first field with a problem, has the focus. It
follows the system's light or dark setting, and runs no script.

The page takes only the sign-in the app's start route made: the
provider must be one of the app's, and the client ID and the return
address the app's own, or it says why and sends the browser nowhere. It
answers only requests addressed to this machine by name, and a choice
only from its own page, so another site can't sign you in. A deployable
never serves it nor accepts its codes.

Not built yet: `Driver.github` and `Driver.apple`.

### The old name

`sign_in_providers:` is refused, and the error renames it to `sign_in:`.

## Magic links
<!-- topic: magic-links -->

A magic link signs a person in by email, with no password. They type
their address, and Jadpo sends one email holding a link and a six-digit
code. Either signs them in, once:

- the link opens a page of your frontend, which posts the link's token to
  Jadpo and gets a browser session;
- the code is typed into your own sign-in page, which posts it the same
  way, or your own mobile or command-line client trades it for an API
  token.

Jadpo generates the endpoints, never the pages: your frontend owns every
page.

Magic links need three things besides `magic_link:` itself:

- `browser_session:` with its `origin:`, since the link starts a browser
  session at that origin;
- an email connection in `application { }`'s `email:`, which sends the
  email, unless `magic_link:` names its own `send:` action
  ([Your own email](#your-own-email));
- `application { display_name: }`, your app's name, which the email
  uses.

`link:` is the one setting you must write: the path of the frontend page
the email's link opens, such as `"/sign-in/confirm"`.

A whole app with magic links: the settings, the email connection and the
app's name, the member, and the action that signs a new member up the
first time they sign in.

```jadpo check project=magic-links file=config.jadpo
config {
    browser_origin: Url
    mail_sender: Email
    smtp_host: Hostname
}
```

```jadpo check project=magic-links file=app.jadpo
use accounts/entities { Member }

application {
    display_name: "Notes"
    connections: {
        mail: {
            driver: Driver.smtp
            host: config.smtp_host
        }
    }
    email: {
        connection: mail
        from: config.mail_sender
    }
}

principal {
    user: {
        subject: Member.email
    }
}
```

```jadpo check project=magic-links file=authentication.jadpo
use accounts

authentication {
    browser_session: {
        cookie: "notes_session"
        origin: config.browser_origin
    }
    api_token: {}
    sign_in: {
        magic_link: {
            link: "/sign-in/confirm"
            same_browser: true
        }
    }
    on_first_sign_in: accounts.create_member
}
```

```jadpo check project=magic-links file=domains/accounts/entities/member.jadpo
entity Member {
    id: Uuid
    email: Email {
        unique: true
    }

    policy: {
        create: [Access.system]
        read: [id]
    }
}
```

```jadpo check project=magic-links file=domains/accounts/failures/sign_up_closed.jadpo hidden
/// Signing up is closed for this address.
type SignUpClosed = Failure {
    kind: FailureKind.conflict
}
```

`on_first_sign_in:` names `create_member`, which runs for an address no
member has ([A person the app has not seen](#a-person-the-app-has-not-seen)).
It runs as the system, the identity Jadpo uses for work no person asked
for (`Access.system` grants it), in its own transaction, and reaches no
tenant's records:

```jadpo check project=magic-links file=domains/accounts/actions/create_member.jadpo migrate=initial
use accounts/entities { Member }
use accounts/failures { SignUpClosed }

/// Signs a new member up the first time they sign in with a proved address.
action create_member(first: FirstSignIn) fails SignUpClosed -> Member.id {
    if (not first.address_verified) {
        reject SignUpClosed {}
    }
    match first.address {
        none -> {
            reject SignUpClosed {}
        }
        some(address) -> {
            var member: Member = attempt Member.create {
                set: {
                    email: address
                }
                conflict: {
                    Member.email: SignUpClosed
                }
            }
            return member.id
        }
    }
}
```

The API record lists the routes Jadpo answers, each with a fixed body and
answer. The route that trades a code for a token comes with `api_token:`:

```jadpo check project=magic-links file=api/main.jadpo_record
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

api main
    authentication api_token
    authentication browser_session

POST /auth/magic-link
    generated magic link
    authentication none
    input MagicLinkAsk (body)
    status 202

POST /auth/magic-link/confirm
    generated magic link
    authentication none
    input MagicLinkConfirmation (body)
    output MagicLinkConfirmed
    status 200

POST /auth/magic-link/redeem
    generated magic link
    authentication none
    input MagicLinkCode (body)
    output MagicLinkToken
    status 200

GET /openapi.json
    generated openapi
    status 200

output MagicLinkConfirmed
    return_to: Text

output MagicLinkToken
    token: ApiToken
```

### The routes

Jadpo generates three routes under `path:`, `/auth/magic-link` unless you
change it. Anyone may call them:

```text
POST /auth/magic-link            ask for a link and a code
POST /auth/magic-link/confirm    a link's token or a code: starts a browser session
POST /auth/magic-link/redeem     a code for an API token (needs api_token:)
```

Each route's response shape is fixed
([Generated routes](07-http.md#generated-routes)). A failure comes back in
the usual failure body, and your frontend shows its wording, which the
per-locale failure export carries ([Configuration and
time](10-configuration-and-time.md)).

### Asking for a link

```text
POST /auth/magic-link
{ "address": "ada@example.com", "return_to": "/notes", "locale": "fr_fr" }

202 Accepted                      (no body; the same for every address)
```

- `address` is required. `return_to` and `locale` are optional.
- `return_to` is where the person goes once signed in. It must be a path
  on your app's own site: it starts with one `/`, and anything else, such
  as `//evil.example` or `https://...`, is a 422 `invalid_input`. Without
  it the person goes to `/`.
- The email's language is `locale` when it is a supported locale, else the
  best supported match for the request's `Accept-Language`, else your
  default locale.
- The answer is `202` with no body whether or not an account has the
  address, so the route never tells anyone which addresses have an
  account. The email is sent after the request commits when an account
  has the address, or when you declare `on_first_sign_in:`. Otherwise
  nothing is sent.
- Sends to one address are limited, counted the same whether or not an
  account has it: one every `resend_after:`, and `codes_per_hour:` an
  hour. Over either limit the answer is the built-in `rate_limited` (429,
  with `Retry-After`), the same for unknown addresses.
- Each send makes the earlier link and code for that address stop
  working. Only the latest is ever valid.

### Confirming a link or a code

The link is `<origin><link>?token=<64 hexadecimal digits>`: the
`origin:` of `browser_session:` and the path `link:` names, for example
`https://notes.example/sign-in/confirm?token=3f9a...c1`. Your page reads
the token and posts it. Loading the page uses up nothing, so a mail
scanner that opens the link signs no one in. Only the `POST` does:

```text
POST /auth/magic-link/confirm
{ "token": "3f9a...c1" }

200 OK
{ "return_to": "/notes" }          (the request's return_to)
Set-Cookie: notes_session=...; notes_session_csrf=...
```

Your own sign-in page posts the code instead, and gets the same answer:

```text
POST /auth/magic-link/confirm
{ "address": "ada@example.com", "code": "042317" }
```

Your own client trades the code for an API token:

```text
POST /auth/magic-link/redeem
{ "address": "ada@example.com", "code": "042317" }

200 OK
{ "token": "jadpo_token_9c1e..." }
```

- A confirm `POST` needs an `Origin` header naming your app's own site,
  as any write with the session cookie does.
- After signing in, your frontend is given only the `return_to` path
  stored when the person asked, never an address the link carries.
- A person signed in by link or code has proved they control the address,
  so it reaches `on_first_sign_in:` as verified.

### Settings

Every setting but `link:` has a default. Tightening one is always
allowed. Loosening one past its limit needs an acceptance: a recorded,
reviewed sign-off on a change that weakens security
([Acceptances](18-records-and-manifest.md#acceptances)). In the change
that takes a setting past its limit, or further past it,
`jadpo check` fails until an acceptance lists
`magic_link.expires_after: PolicyChange.sign_in_loosened`, naming the
setting. `access.jadpo_record` keeps each setting that has a limit, so
the change shows in its diff.

| Setting | Default | Needs an acceptance above |
|---|---|---|
| `address:` | the `email` field of the principal's user entity | |
| `expires_after:` | `15m`, how long a link and a code last | `1h` |
| `code_attempts:` | `5` wrong guesses end a code | `10` |
| `codes_per_hour:` | `10` sends per address | |
| `failed_codes_per_day:` | `20` wrong guesses per address | `50` |
| `resend_after:` | `1m` between sends to one address | |
| `same_browser:` | `false` | |
| `link:` | none: required, the frontend path the email's link opens, such as `"/sign-in/confirm"` | |
| `path:` | `"/auth/magic-link"`, where the generated routes go | |
| `generate_routes:` | `true`; `false` generates none | |
| `email:` | Jadpo's own email | |
| `send:` | none | |

`address:` names a field of the principal's user entity that is built on
`Email` and is `unique: true`. Leave it out and the entity's `email` field
is used. With no such field, `address:` is required.

### Wrong codes

`code_attempts:` wrong guesses end that code. Every wrong guess gets the
same answer, `sign_in_code_invalid`, never a count of guesses left. So
does a code that was used, ended, expired or replaced, and an address with
no code.

When an address reaches `failed_codes_per_day:` wrong guesses in a day
(UTC), codes stop working for it until the next day
(`sign_in_codes_disabled`), while links still work. Nothing locks an
account, so an attacker can't lock a person out.

### Only in the browser that asked: `same_browser:`

`same_browser: true` makes a link work only in the browser that asked for
it. `POST /auth/magic-link` sets a `<cookie>_sign_in` cookie (`HttpOnly`,
`Secure`, `SameSite=Lax`, lasting `expires_after:`), and the confirm
refuses a link from any other browser (`sign_in_link_other_browser`). The
code still works anywhere.

### What is kept

Link tokens are 32 random bytes. Codes are six random digits, kept as
text so a leading `0` stays. Jadpo keeps both only as verifiers, with a
digest of the address rather than the address itself, in the
`jadpo_sign_in_requests` system table. That table is in the store of the
principal's user entity, beside `jadpo_sessions`
([System tables](05-data.md#system-tables)). Each link and code works
once, and using either ends the other.

### Writing the routes yourself

Use it when your sign-in needs a step of its own around the generated
behaviour. `path:` only moves the generated routes. `generate_routes:
false` generates none, and your app writes its own routes over the
operations below. They keep every protection above: the limits, only the
latest being valid, the wrong-code rules, `same_browser:` and
`on_first_sign_in:`.

| Operation | Does |
|---|---|
| `magic_links.request(address: Email, locale: Locale, return_to: Text?)` | Sends the link and code after commit, as `POST /auth/magic-link` does; fails with `rate_limited`, and with `sign_in_return_invalid` (`invalid_value`, 422) for a `return_to` that is not a path on the app's site |
| `magic_links.redeem(token: SignInToken)` | Uses a link's token; gives a `MagicLinkRedeemed` (`user_id`, `return_to: Text`) or fails with a `sign_in_` failure |
| `magic_links.redeem_code(address: Email, code: SignInCode)` | The same for a code |

Your action then calls `sessions.start(redeemed.user_id)` or
`sessions.issue_token(redeemed.user_id)`:

```jadpo fragment
// domains/accounts/actions/confirm_sign_in.jadpo
action confirm_sign_in(input: ConfirmSignIn) -> SignedInView {
    var redeemed: MagicLinkRedeemed = magic_links.redeem(input.token)
    sessions.start(redeemed.user_id)
    ...
}
```

- Only actions call these operations, as with `sessions.start`.
- `same_browser:`'s cookie is set on the response of the route whose
  action calls `magic_links.request`.
- The `sign_in_` failures are built in and raised by Jadpo. Like every
  sign-in failure, they pass through an action without being listed in
  its `fails` ([Failures](04-failures.md)).

### Your own email

Jadpo sends its own magic-link email, worded for you, through your
`email:` connection. Two settings change that
([Built-in emails](16-built-in-services.md)):

- `email:` names an email type of your own, built from the fields of the
  built-in `MagicLinkEmail`. Jadpo still makes the link and the code and
  sends it.
- `send:` names an action of yours that sends the email itself, given a
  `MagicLinkRequest`.

You can write one of them, not both.

## Passkeys
<!-- topic: passkeys -->

A passkey lets a person sign in with their fingerprint, their face or
their device's PIN, with no password and no email to wait for. The device
holds a key, and the person unlocks it. The fingerprint or face never
leaves the device, and your app stores only a public key, so there is no
secret on the server to leak. A passkey works only on the site it was
made for, so a fake site can't use it. Passkeys are the WebAuthn and FIDO2
standards.

Jadpo does all of it for you:

- **Registering and signing in.** The exchanges between the browser and
  your server, called ceremonies, are built in: the one-time challenge,
  its expiry, the check of the site's origin, and the signature counter.
  Jadpo checks each as the `webauthn-rs` library does, and its tests hold
  it to that library's answers. It doesn't ask the device to prove its
  make (attestation).
- **Stored with the user** as the standard `WebAuthnCredential` type, never
  a biometric. The [personal-data report](15-personal-data.md) lists only
  each passkey's identifier and when it was last used.
- **Recovery is a magic link.** A person who has lost every device with a
  passkey signs in by email and adds a new one, so passkeys need magic
  links in the same app.
- **More than one passkey per person.** Each has a name the person gives
  it and can be removed, so a phone and a laptop each have their own.

`passkey:` goes in `sign_in:` beside `magic_link:`:

```jadpo fragment
// authentication.jadpo
authentication {
    browser_session: {
        cookie: "notes_session"
        origin: config.browser_origin
    }
    sign_in: {
        magic_link: {
            address: Member.email
            link: "/sign-in/confirm"
        }
        passkey: {}
    }
}
```

| Setting | Default | Allowed |
|---|---|---|
| `expires_after:` | `5m`, how long a ceremony's challenge lasts | `1m` to `10m` |
| `path:` | `"/auth/passkey"`, where the generated routes go | a path of fixed segments |

- `passkey:` needs `browser_session:` with its `origin:`. Every answer
  from a device is checked against that origin. Its host is the
  relying-party ID, the site name a passkey is bound to. Jadpo reads it
  from your configuration, never from the request, and never widens it to
  a parent domain or another port. A passkey made for
  `https://notes.example` works only there.
- `passkey:` needs `magic_link:` in the same `sign_in:`. It is how a
  person with no passkey left gets back in.
- Every passkey checks the person, not just the device
  (`userVerification: "required"`), so it is two factors in one: the
  device, and the fingerprint, face or PIN that unlocks it.
- Every passkey is discoverable (`residentKey: "required"`): the device
  can find it without being told whose it is. So signing in asks for no
  address first, and tells no one which addresses have an account.
- The device lists a passkey under the person's address (the field
  `magic_link:`'s `address:` names) and your app's host.
- A person has at most 20 passkeys.

### The routes

Jadpo generates seven routes under `path:`, `/auth/passkey` unless you
change it. Anyone may call the two sign-in routes. Adding and managing
passkeys needs the person signed in with a browser session. Any other
credential, an API token among them, is 403 `not_permitted`, since a
passkey it added would outlive it. Every write needs the session's CSRF
token and an `Origin` naming your app's site, as any write with the
session cookie does:

```text
POST   /auth/passkey/sign-in/options     options for signing in
POST   /auth/passkey/sign-in             the device's answer: starts a browser session
POST   /auth/passkey/register/options    options for adding a passkey (signed in)
POST   /auth/passkey/register            the new passkey and its name: keeps it (signed in)
GET    /auth/passkey                     the person's passkeys (signed in)
PATCH  /auth/passkey/{passkey_id}        renames one (signed in)
DELETE /auth/passkey/{passkey_id}        removes one (signed in)
```

### Signing in

Signing in is two requests around the browser's own prompt. The first,
with no body, gives the options for `navigator.credentials.get()` (which
`PublicKeyCredential.parseRequestOptionsFromJSON` reads) and a cookie
binding them to this browser:

```text
POST /auth/passkey/sign-in/options

200 OK
{ "publicKey": { "challenge": "Vh3q...", "rpId": "notes.example", "userVerification": "required", "timeout": 300000, ... }, "mediation": "conditional" }
Set-Cookie: notes_session_passkey=...; HttpOnly; Secure; SameSite=Strict
```

The frontend posts what the device answers, as WebAuthn's `toJSON()`
writes it:

```text
POST /auth/passkey/sign-in
{ "credential": { "id": "AQID...", "rawId": "AQID...", "type": "public-key", "response": { "clientDataJSON": "...", "authenticatorData": "...", "signature": "...", "userHandle": "..." } } }

204 No Content
Set-Cookie: notes_session=...; notes_session_csrf=...
```

- Each options request makes a new challenge of 32 random bytes, kept in
  the `jadpo_passkey_challenges` system table with what checking the
  answer needs: server-side, never in the cookie. The cookie,
  `<cookie>_passkey` (`HttpOnly`, `Secure`, `SameSite=Strict`, lasting
  `expires_after:`), holds 32 more random bytes kept only as a verifier,
  and binds the challenge to the browser that asked; a new request
  replaces that browser's earlier challenge.
- `mediation: "conditional"` lets the browser offer the passkey in the
  address field's autofill; a frontend with its own "Sign in with a
  passkey" button drops it.
- The sign-in `POST` needs an `Origin` naming the app's site, as a magic
  link's confirm does. Its challenge is used up when the answer arrives,
  whether or not the answer checks: a replayed answer, one whose challenge
  expired, and one from another browser are `passkey_challenge_expired`.
- The answer is checked: the challenge, the origin and the
  relying-party ID, that the person was present and verified, the
  signature against the stored public key, and the signature counter. Any
  failure is `passkey_rejected`, never saying which, and so is an answer
  from a passkey that was removed or never registered.
- **The counter.** A device that keeps a signature counter adds to it each
  time it signs. When the stored counter or the answer's is above zero,
  the answer's must be higher than the stored one (WebAuthn Level 3,
  §7.2, the signature counter's step). One that isn't means the key may
  have been copied, so the sign-in is refused (`passkey_rejected`) and the passkey left as it was,
  so the person signs in another way. A passkey that keeps no counter,
  which most synced ones don't, sends `0` every time and is never refused
  for it.
- A person whose `active:` doesn't hold is `user_disabled`, as on any
  request.
- On success the passkey's counter, backup state and last use are
  updated, a browser session starts as `sessions.start` starts one, and
  `<cookie>_passkey` is cleared. A passkey signs in a browser session
  only. Your own mobile and command-line clients sign in with a magic
  link's code.

### Adding a passkey

Adding a passkey is the same two steps, signed in:

```text
POST /auth/passkey/register/options

200 OK
{ "publicKey": { "challenge": "o7Wd...", "rp": { "id": "notes.example", "name": "notes.example" }, "user": { "id": "...", "name": "ada@example.com", "displayName": "ada@example.com" }, "excludeCredentials": [...], "authenticatorSelection": { "residentKey": "required", "requireResidentKey": true, "userVerification": "required" }, "attestation": "none", ... } }
```

```text
POST /auth/passkey/register
{ "name": "Work phone", "credential": { "id": "AQID...", "rawId": "AQID...", "type": "public-key", "response": { "clientDataJSON": "...", "attestationObject": "..." } } }

201 Created
{ "id": "0192d3c4-...", "name": "Work phone", "registered_at": "2026-10-18T09:00:00Z", "last_used_at": null }
```

- The challenge is bound to the session that asked for it, used up when
  the answer arrives whether or not it checks, and refused from any other
  session (`passkey_challenge_expired`); a new options request replaces
  that session's earlier challenge.
- `excludeCredentials` lists the person's passkeys, so a device that holds
  one already says so rather than making a second.
- `name` is required: the person's own name for the device, one line. A
  passkey registered already, to anyone, is `passkey_already_registered`;
  a 21st is `passkey_limit_reached`.
- The user handle the device keeps is 16 random bytes per passkey, never
  the user's `id`, so it says nothing about the account.

### Listing, renaming and removing passkeys

```text
GET /auth/passkey

200 OK
[{ "id": "0192d3c4-...", "name": "Work phone", "registered_at": "2026-10-18T09:00:00Z", "last_used_at": "2026-10-18T09:05:00Z" }]
```

```text
PATCH /auth/passkey/0192d3c4-...
{ "name": "Old phone" }

200 OK
{ "id": "0192d3c4-...", "name": "Old phone", "registered_at": "2026-10-18T09:00:00Z", "last_used_at": "2026-10-18T09:05:00Z" }
```

```text
DELETE /auth/passkey/0192d3c4-...

204 No Content
```

- Each reaches only the signed-in person's own passkeys: any other `id`
  is `passkey_not_found`. Removing the last one is allowed; the person
  signs in with a magic link and adds another. Removing a passkey ends no
  session it started.
- Each route's response shape is fixed
  ([Generated routes](07-http.md#generated-routes)). The routes belong to
  the first API that accepts browser sessions, and are listed in its
  record, its OpenAPI document and `jadpo routes`, marked generated.
- Your app writes no passkey routes of its own: one at any of these paths
  is `route.path_overlap`. Unlike magic links, there is no
  `generate_routes: false`. Every body is WebAuthn's rather than your
  app's, so your own route would have nothing to add.

### What is kept

`jadpo_passkeys`, a system table in the store of the principal's user
entity beside `jadpo_sessions`, holds each passkey as a
`WebAuthnCredential` from the [standard library](19-standard-library.md):

- its credential ID and its public key (a COSE key);
- its signature counter and its transports;
- whether it may be backed up, and whether it is;
- its name, and when it was registered and last used;
- the user's `id` and the user handle.

None of it is a secret: a copy of the table signs no one in. The
[personal-data report](15-personal-data.md) lists only each passkey's
credential ID and its last use.

### Recovery

A person with no passkey left signs in with a magic link or its code, then
adds a new passkey through the routes above. Nothing else recovers an
account, so a lost phone never needs an operator.

### A whole app with passkeys

The origin setting, the user, `passkey:` beside the browser session, and
magic links beside it, with the email connection and the app's name their
email needs:

```jadpo check project=passkeys file=config.jadpo
config {
    browser_origin: Url {
        test: "https://notes.example"
    }
    mail_sender: Email
    smtp_host: Hostname
}
```

```jadpo check project=passkeys file=app.jadpo
use journal/entities { Member }

application {
    display_name: "Notes"
    connections: {
        mail: {
            driver: Driver.smtp
            host: config.smtp_host
        }
    }
    email: {
        connection: mail
        from: config.mail_sender
    }
}

principal {
    user: {
        subject: Member.email
    }
}
```

```jadpo check project=passkeys file=authentication.jadpo
authentication {
    browser_session: {
        cookie: "notes_session"
        origin: config.browser_origin
    }
    sign_in: {
        magic_link: {
            link: "/sign-in/confirm"
        }
        passkey: {
            expires_after: 2m
        }
    }
}
```

```jadpo check project=passkeys file=domains/journal/entities/member.jadpo migrate=initial
entity Member {
    id: Uuid
    email: Email {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
    }
}
```

```jadpo check project=passkeys file=domains/journal/failures/email_taken.jadpo hidden
/// The address has an account already.
type EmailTaken = Failure {
    kind: FailureKind.conflict
}
```

```jadpo check project=passkeys file=domains/journal/types/enrolment.jadpo hidden
type Enrolment = Input {
    email: Email
}
```

```jadpo check project=passkeys file=domains/journal/actions/enrol_member.jadpo hidden
use journal/entities { Member }
use journal/types { Enrolment }
use journal/failures { EmailTaken }

/// Adds a member with an address.
action enrol_member(input: Enrolment) fails EmailTaken -> Member {
    return attempt Member.create {
        set: {
            email: input.email
        }
        conflict: {
            Member.email: EmailTaken
        }
    }
}
```

```jadpo check project=passkeys file=policy.jadpo hidden
use journal

policy {
    actions: {
        journal.enrol_member: [Access.public]
    }
}
```

```jadpo check project=passkeys file=domains/journal/fixtures/member_called.jadpo hidden
use journal/entities { Member }
use journal/types { Enrolment }
use journal/actions { enrol_member }

/// A member with `address`, signed up with no passkey.
public fixture member_called(address: Email) -> Principal.user {
    var member: Member = attempt enrol_member(
        Enrolment {
            email: address
        }
    )
    return Principal.user(member)
}
```

A route test drives the generated routes with `test_passkeys`
([In development and tests](#in-development-and-tests)). Ada adds a passkey
and signs in with it:

```jadpo check project=passkeys file=tests/passkeys.test.jadpo
use journal

test "a passkey signs Ada in" {
    var ada: Principal.user = journal.member_called("ada@example.com")
    var phone: TestPasskey = test_passkeys.register(ada, "Phone")
    var response: Response = test_passkeys.sign_in(phone)
    assert response.status == 204
}
```

The API record lists the routes Jadpo answers, as `jadpo routes` and
OpenAPI do. WebAuthn's own JSON goes in and out of the ceremonies, and the
record names each body for what it is:

```jadpo check project=passkeys file=api/main.jadpo_record
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

api main
    authentication browser_session

POST /auth/magic-link
    generated magic link
    authentication none
    input MagicLinkAsk (body)
    status 202

POST /auth/magic-link/confirm
    generated magic link
    authentication none
    input MagicLinkConfirmation (body)
    output MagicLinkConfirmed
    status 200

GET /auth/passkey
    generated passkey
    caller user
    output List<PasskeyView>
    status 200

POST /auth/passkey/register
    generated passkey
    caller user
    input PasskeyRegistration (body)
    output PasskeyView
    status 201

POST /auth/passkey/register/options
    generated passkey
    caller user
    output PasskeyCreationOptions
    status 200

POST /auth/passkey/sign-in
    generated passkey
    authentication none
    input PasskeyAnswer (body)
    status 204

POST /auth/passkey/sign-in/options
    generated passkey
    authentication none
    output PasskeyRequestOptions
    status 200

PATCH /auth/passkey/{passkey_id: Uuid}
    generated passkey
    caller user
    input PasskeyName (body)
    output PasskeyView
    status 200

DELETE /auth/passkey/{passkey_id: Uuid}
    generated passkey
    caller user
    status 204

GET /openapi.json
    generated openapi
    status 200

output MagicLinkConfirmed
    return_to: Text

output PasskeyView
    id: Uuid
    last_used_at: Instant?
    name: Text
    registered_at: Instant
```

### When the setup is incomplete

Without an origin there is nothing to bind a passkey to:

```jadpo check project=e-passkey-origin expect=semantic.auth_setting_required
principal {
    user: {
        subject: Member.email
    }
}

authentication {
    browser_session: {
        cookie: "session"
    }
    sign_in: {
        passkey: {}
    }
}

entity Member {
    id: Uuid
    email: Email {
        unique: true
    }
}
```

```text output
error: `passkey:` needs the browser session's `origin:`
 --> main.jadpo:12:9
   |
12 |         passkey: {}
   |         ^^^^^^^ no origin to bind to
  reason: A passkey works only on the site it was made for: every answer from a device is checked against the origin `browser_session:`'s `origin:` reads from configuration (area 10), never one the request names.
  next: Add `origin: config.<name>` to `browser_session:`, reading a `Url` setting (agent)
  rule: semantic.auth_setting_required
  help: https://jadpo.dev/docs/errors/semantic.auth_setting_required

check failed: 1 error, 1 file
```

Without magic links, a person who loses every device has no way back in:

```jadpo check project=e-passkey-recovery expect=semantic.auth_setting_required
config {
    browser_origin: Url
}

principal {
    user: {
        subject: Member.email
    }
}

authentication {
    browser_session: {
        cookie: "session"
        origin: config.browser_origin
    }
    sign_in: {
        passkey: {}
    }
}

entity Member {
    id: Uuid
    email: Email {
        unique: true
    }
}
```

```text output
error: `passkey:` needs `magic_link:` beside it
 --> main.jadpo:17:9
   |
17 |         passkey: {}
   |         ^^^^^^^ no way back in
  reason: A person who loses every device holding a passkey signs in with a magic link or its code and adds a new one; with no `magic_link:` in the same `sign_in:`, nothing gets them back in.
  next: Add `magic_link: { link: "<path>" }` to `sign_in:`, beside `passkey:` (agent)
  rule: semantic.auth_setting_required
  help: https://jadpo.dev/docs/errors/semantic.auth_setting_required

check failed: 1 error, 1 file
```

## A person the app has not seen
<!-- topic: a-person-the-app-has-not-seen -->

The first time someone signs in whom your app has no user for, Jadpo
asks your app what to do. `on_first_sign_in:` in `authentication { }`
names the action that decides: create a user, link one that already
exists, or refuse. One action decides for every way in.

It runs when:

- a magic link or code is used for an address no user has;
- a provider returns an account not yet linked to a user;
- an external token names a subject not yet linked to a user.

Here the action refuses an address Google hasn't verified, and creates a
member otherwise:

```jadpo fragment
// domains/accounts/actions/create_member.jadpo
action create_member(first: FirstSignIn) fails SignUpClosed -> Member.id {
    if (first.method == SignInMethod.google and not first.address_verified) {
        reject SignUpClosed
    }
    var member: Member = attempt Member.create(...)
    return member.id
}
```

| `FirstSignIn` field | Type | Holds |
|---|---|---|
| `method` | `SignInMethod` | The way in: `magic_link`, each provider named under `sign_in:`, or `external_token`; a built-in enum with one value per way the app declares |
| `address` | `Email?` | For a magic link, the address, verified; for a provider, its email claim; for an external token, none |
| `address_verified` | `Bool` | `true` for a magic link; the provider's own verified claim for a provider; `false` for an external token |
| `subject` | `Text?` | The provider's or token's subject; none for a magic link |
| `name` | `Text?` | The display name a provider's profile gives, when it gives one |

- The action takes exactly one `FirstSignIn` and returns the `id` of the
  principal's user entity: a user it created, or one that already exists.
  It runs in its own transaction before the session starts.
- To refuse, it fails with any failure it declares. The person always
  sees the same built-in `sign_in_refused`, whatever the reason. The log
  and the `SignInRefused` event keep the real one.
- For a provider or an external token, Jadpo then records the link, from
  the method and subject to the user's `id`, in the `jadpo_sign_in_links`
  system table, so the next sign-in skips the action. A magic link needs
  no link: Jadpo finds the user by `address:`.
- Linking a user who already exists, by their address, is the action's
  choice. Do it only when `address_verified` is `true`, so no one can
  take over an account by claiming its address at a provider that doesn't
  check it.
- Without `on_first_sign_in:`, only users who already exist sign in. An
  unknown address gets no email, and an unknown provider account or token
  subject is `sign_in_refused`.
- A passkey never runs it. Only a person already signed in can add a
  passkey, so every passkey that signs in has its user.

An external token is signed in on every request, so its first sign-in
happens on the first request it comes with:

- Jadpo looks for the user whose `subject:` field holds the token's
  subject, then for one a first sign-in linked it to, in one read. A
  subject that isn't of the field's type is looked for among the links
  only.
- The action runs once for each subject. Requests that arrive together
  with a new subject, as a page's first load sends them, wait for the
  first, then sign in as the user it gave.
- The user the action gives is linked even when they aren't `active:`,
  and is refused with `user_disabled` until they are. So an action can
  take someone on to wait for approval without being asked again.

Here the `staff` API takes the company provider's tokens, as in
[External tokens in tests and `jadpo dev`](#external-tokens-in-tests-and-jadpo-dev),
and someone's first token takes them on as a librarian who waits for a
manager's approval:

```jadpo check project=first-token file=authentication.jadpo
use desk

authentication {
    browser_session: {
        cookie: "desk_session"
    }
    external_token: {
        issuer: config.issuer
        audience: config.audience
    }
    on_first_sign_in: desk.take_on_newcomer
}
```

```jadpo check project=first-token file=domains/desk/actions/take_on_newcomer.jadpo
use desk/entities { Librarian }
use desk/failures { SubjectTaken }

/// Takes on someone the company's provider vouches for, to wait for a
/// manager's approval.
action take_on_newcomer(first: FirstSignIn) fails SubjectTaken -> Librarian.id {
    match first.subject {
        none -> {
            reject SubjectTaken
        }
        some(subject) -> {
            var newcomer: Librarian = attempt Librarian.create {
                set: {
                    subject: subject
                    name: "New starter"
                    active: false
                }
                conflict: {
                    Librarian.subject: SubjectTaken
                }
            }
            return newcomer.id
        }
    }
}
```

```jadpo check project=first-token file=app.jadpo hidden
use desk/entities { Librarian }

application {
    display_name: "Lending desk"
    apis: {
        main: {
            prefix: /
            authentication: [AuthenticationMethod.browser_session]
        }
        staff: {
            prefix: /staff
            authentication: [AuthenticationMethod.external_token]
        }
    }
}

principal {
    user: {
        subject: Librarian.subject
        active: Librarian.active == true
    }
}
```

```jadpo check project=first-token file=config.jadpo hidden
config {
    audience: Text {
        test: "lending-desk"
    }
    issuer: Url {
        default: "https://accounts.google.com"
        test: "https://id.lending.example/"
    }
}
```

```jadpo check project=first-token file=domains/desk/entities/librarian.jadpo hidden
entity Librarian {
    id: Uuid
    subject: Text {
        unique: true
    }
    name: Text
    active: Bool

    policy: {
        create: [Access.system]
        read: [id]
        delete: [Access.system]
    }
}
```

```jadpo check project=first-token file=domains/desk/failures/not_a_librarian.jadpo hidden
/// The caller isn't on the staff.
type NotALibrarian = Failure {
    kind: FailureKind.not_found
}
```

```jadpo check project=first-token file=domains/desk/failures/subject_taken.jadpo hidden
/// Someone else already signs in with that subject.
type SubjectTaken = Failure {
    kind: FailureKind.conflict
}
```

```jadpo check project=first-token file=domains/desk/types/librarian_card.jadpo hidden
use desk/entities { Librarian }

type LibrarianCard = Output {
    pick: [Librarian.id, Librarian.name, Librarian.active]
}
```

```jadpo check project=first-token file=domains/desk/types/new_librarian.jadpo hidden
use desk/entities { Librarian }

/// Someone to take on, under the subject their provider gives them.
type NewLibrarian = Input {
    subject: Librarian.subject
    name: Librarian.name
    active: Librarian.active
}
```

```jadpo check project=first-token file=domains/desk/actions/take_on.jadpo hidden
use desk/entities { Librarian }
use desk/types { NewLibrarian }
use desk/failures { SubjectTaken }

/// Takes someone on.
action take_on(input: NewLibrarian) fails SubjectTaken -> Librarian {
    return attempt Librarian.create {
        set: {
            subject: input.subject
            name: input.name
            active: input.active
        }
        conflict: {
            Librarian.subject: SubjectTaken
        }
    }
}
```

```jadpo check project=first-token file=domains/desk/actions/let_go.jadpo hidden
use desk/entities { Librarian }

/// Lets someone go, record and all.
action let_go(librarian: Librarian) {
    Librarian.delete(librarian)
}
```

```jadpo check project=first-token file=domains/desk/queries/me.jadpo hidden
use desk/entities { Librarian }
use desk/types { LibrarianCard }
use desk/failures { NotALibrarian }

/// The signed-in member of staff.
query me(user: Principal.user) fails NotALibrarian -> LibrarianCard {
    from: Librarian
    where: Librarian.id == user.id
    missing: NotALibrarian
}
```

```jadpo check project=first-token file=domains/desk/routes/staff.jadpo hidden migrate=initial
use desk/types { LibrarianCard }
use desk/queries { me }

/// What staff reach with their provider's token.
routes Staff {
    api: staff
    prefix: /staff
    GET /me {
        output: LibrarianCard
        run: me(current_principal.user)
    }
}
```

```jadpo check project=first-token file=api/staff.jadpo_record hidden
// Generated by Jadpo from the routes of API staff; update it with `jadpo api update`, never by hand.
record api 1

api staff
    authentication external_token

GET /staff/me
    caller user
    output LibrarianCard
    status 200
    fails not_a_librarian

GET /staff/openapi.json
    generated openapi
    status 200

output LibrarianCard
    active: Bool
    id: Uuid
    name: Text

failure not_a_librarian
    status 404
```

```jadpo check project=first-token file=domains/desk/routes/staff.test.jadpo hidden
use desk/entities { Librarian }
use desk/types { NewLibrarian }
use desk/actions { let_go, take_on }

test "a newcomer's first token takes them on to wait for approval" {
    // `as:` sends a token for the subject the caller was made with, so
    // once Cy's record is gone no librarian holds it.
    var cy: Librarian = attempt take_on(
        NewLibrarian {
            subject: "google-cy"
            name: "Cy"
            active: true
        }
    )
    var newcomer: Principal.user = Principal.user(cy)
    let_go(cy)
    var first: Response = request {
        method: HttpMethod.get
        path: "/staff/me"
        as: newcomer
    }
    assert first.code == "user_disabled"
    var returning_card: Response = request {
        method: HttpMethod.get
        path: "/staff/me"
        as: newcomer
    }
    assert returning_card.code == "user_disabled"
}
```


## Signing in from code the app writes
<!-- topic: signing-in-from-code-the-app-writes -->

Most apps let `sign_in:` check people. When your app checks a person
itself, such as someone accepting an invitation it sent, its action calls
`sessions.start(user_id)` for a browser session or
`sessions.issue_token(user_id)` for an API token. See
[Starting and ending a session](#starting-and-ending-a-session).

## Events
<!-- topic: events -->

Every sign-in publishes a built-in event, which your monitoring and your
app's own [subscribers](12-jobs-and-events.md#subscribers) can use:

| Event | Fields | When |
|---|---|---|
| `SignedIn` | `user_id` (the user entity's `id`), `method: SignInMethod`, `first_time: Bool` | A session or API token was started by `sign_in:` or an external token's first use |
| `SignInRefused` | `method: SignInMethod`, `reason: SignInRefusal`, `user_id` (when known) | A link, code or first sign-in was refused |
| `SignInCodesDisabled` | `user_id` (when the address has an account), `until: Instant` | An address reached `failed_codes_per_day:` |
| `PasskeyAdded` | `user_id`, `passkey_id: Uuid` | A person registered a passkey |
| `PasskeyRemoved` | `user_id`, `passkey_id: Uuid` | A person removed one of their passkeys |

`SignInRefusal` is a built-in enum: `link_expired`, `link_other_browser`,
`code_invalid`, `codes_disabled`, `refused_by_app`, `user_disabled`,
`passkey_challenge_expired` and `passkey_rejected`. `SignInMethod` has
`passkey` when `passkey:` is declared, and a value for each sign-in
provider, its connection's name.

No event carries an address, a link, a code, a challenge, or a passkey's
credential ID or key.

## In development and tests
<!-- topic: in-development-and-tests -->

### Magic links in `jadpo dev`

In `jadpo dev`, no email leaves your machine. It lands in the local inbox
([The dev inbox](16-built-in-services.md)), and its link and code work
there. The link opens your frontend's page. An agent reads the token or
code with `jadpo email links <id>` or `jadpo email show <id>`, and posts
it to `POST /auth/magic-link/confirm` itself.

### Magic links in tests

Most route tests don't sign in through a magic link. A request's `as:`
signs the test in as a user directly
([Acting as a user](13-testing.md)).

To test the sign-in itself, a route test asks for a link with
`POST /auth/magic-link`, reads the email from `email.sent`, and follows
its link or posts its code. It calls the generated routes with
`request { }`, as it calls your own. Each takes a built-in input, named
as the API record names it, which only a test, a fixture or a seed
builds:

| Route | `input:` | Its fields | `output` |
|---|---|---|---|
| `POST /auth/magic-link` | `MagicLinkAsk` | `address: Email`, `return_to: Text?`, `locale: Locale?` | none |
| `POST /auth/magic-link/confirm` | `MagicLinkConfirmation` | `token: SignInToken?`, `address: Email?`, `code: SignInCode?` | `MagicLinkConfirmed`, its `return_to: Text` |
| `POST /auth/magic-link/redeem` | `MagicLinkCode` | `address: Email`, `code: SignInCode` | `MagicLinkToken`, its `token: ApiToken` |

- The redeem route is there only when you declare `api_token:`.
- Leave out any field that may be `none`, as a request's body may.
- Every request a test sends carries your app's own `Origin`, as a
  browser's does, so the confirm route takes it.
- The sign-in email lands in `email.sent` with its link's token in
  `sign_in_token` and its code in `sign_in_code`. Both are `none` on
  every other email, so a test never reads them out of the text.
- Built anywhere but a test, a fixture or a seed, an input is
  `type.not_record`.

```jadpo check project=magic-link-tests file=app.jadpo hidden
use notes/entities { Member }

application {
    display_name: "Notes"
    connections: {
        mail: {
            driver: Driver.smtp
            host: config.smtp_host
        }
    }
    email: {
        connection: mail
        from: config.mail_sender
    }
}

principal {
    user: {
        subject: Member.email
    }
}
```

```jadpo check project=magic-link-tests file=config.jadpo hidden
config {
    mail_sender: Email {
        default: "hello@notes.example"
    }
    site: Url {
        default: "https://notes.example"
    }
    smtp_host: Hostname {
        default: "localhost"
    }
}
```

```jadpo check project=magic-link-tests file=authentication.jadpo hidden
use notes

authentication {
    browser_session: {
        cookie: "notes_session"
        origin: config.site
    }
    sign_in: {
        magic_link: {
            link: "/sign-in"
        }
    }
    on_first_sign_in: notes.welcome_member
}
```

```jadpo check project=magic-link-tests file=domains/notes/entities/member.jadpo hidden
entity Member {
    id: Uuid
    email: Email {
        unique: true
    }

    policy: {
        create: [Access.system]
        read: [id]
    }
}
```

```jadpo check project=magic-link-tests file=domains/notes/failures/already_a_member.jadpo hidden
type AlreadyAMember = Failure {
    kind: FailureKind.conflict
}
```

```jadpo check project=magic-link-tests file=domains/notes/actions/welcome_member.jadpo hidden migrate=initial
use notes/entities { Member }
use notes/failures { AlreadyAMember }

/// Makes a member of someone signing in for the first time.
action welcome_member(first: FirstSignIn) fails AlreadyAMember -> Member.id {
    match first.address {
        none -> {
            reject AlreadyAMember
        }
        some(address) -> {
            var member: Member = attempt Member.create {
                set: {
                    email: address
                }
                conflict: {
                    Member.email: AlreadyAMember
                }
            }
            return member.id
        }
    }
}
```

```jadpo check project=magic-link-tests file=api/main.jadpo_record hidden
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

api main
    authentication browser_session

POST /auth/magic-link
    generated magic link
    authentication none
    input MagicLinkAsk (body)
    status 202

POST /auth/magic-link/confirm
    generated magic link
    authentication none
    input MagicLinkConfirmation (body)
    output MagicLinkConfirmed
    status 200

GET /openapi.json
    generated openapi
    status 200

output MagicLinkConfirmed
    return_to: Text
```

```jadpo check project=magic-link-tests file=tests/sign_in.test.jadpo
test "an emailed link signs a new person in" {
    var asked: Response = request {
        method: HttpMethod.post
        path: "/auth/magic-link"
        input: MagicLinkAsk {
            address: "ada@notes.example"
        }
    }
    assert asked.status == 202
    match list.first(email.sent) {
        none -> {
            assert list.count(email.sent) == 1
        }
        some(sent) -> {
            assert sent.to == "ada@notes.example"
            var came_back: Response = request {
                method: HttpMethod.post
                path: "/auth/magic-link/confirm"
                input: MagicLinkConfirmation {
                    token: sent.sign_in_token
                }
            }
            assert came_back.status == 200
        }
    }
}

test "the emailed code signs a new person in" {
    var asked: Response = request {
        method: HttpMethod.post
        path: "/auth/magic-link"
        input: MagicLinkAsk {
            address: "ada@notes.example"
        }
    }
    assert asked.status == 202
    match list.first(email.sent) {
        none -> {
            assert list.count(email.sent) == 1
        }
        some(sent) -> {
            var came_back: Response = request {
                method: HttpMethod.post
                path: "/auth/magic-link/confirm"
                input: MagicLinkConfirmation {
                    address: "ada@notes.example"
                    code: sent.sign_in_code
                }
            }
            assert came_back.status == 200
        }
    }
}
```

To test `on_first_sign_in:`'s action, call it directly with a
`FirstSignIn` you build, giving every field. A subscriber's action takes a
sign-in event built the same way:

```jadpo fragment
test "an unproved address is refused" {
    assert create_member(FirstSignIn {
        method: SignInMethod.magic_link
        address: "ada@example.com"
        address_verified: false
        subject: none
        name: none
    }) fails SignUpClosed
}
```

### Passkeys in `jadpo dev`

Browsers allow passkeys on `http://localhost` without TLS. So in
`jadpo dev`, a browser's own passkeys work against a `browser_origin` of
`http://localhost:<port>`. An agent uses the browser's virtual
authenticator, Chrome DevTools' WebAuthn panel or WebDriver's virtual
authenticator commands, rather than a real device.

### Passkeys in tests

A route test has `test_passkeys`, a software authenticator that drives
the generated routes for real. Only tests can call it:

| Operation | Does |
|---|---|
| `test_passkeys.register(user: Principal.user, name: Text) -> TestPasskey` | Adds a passkey for `user` through `POST /auth/passkey/register/options` and `POST /auth/passkey/register`, signed in as `as:` signs in, and gives the device holding it |
| `test_passkeys.sign_in(passkey: TestPasskey) -> Response` | Signs in with it through the two sign-in routes and gives the second's `Response` |
| `test_passkeys.confirm(passkey: TestPasskey)` | Confirms the next request the test sends to a route with `confirm: Confirmation.passkey` with it ([Confirm one request with a passkey](#confirm-one-request-with-a-passkey-confirm-confirmationpasskey)) |

Each `TestPasskey` is a device of its own holding one passkey, made by
the program running the tests, so only a test ever holds one. Called
anywhere else, `test_passkeys` is `semantic.request_invalid`.

If a route refuses `register`, the first step of `sign_in` or a
confirmation's options, the test ends as a fault naming the route and its
code, since the test meant to have a passkey. A refusal of the sign-in or
of the confirmed request itself comes back in its `Response`, for the test
to check.

`test_passkeys.confirm` sends the next request to the route's
`…/confirmation` first, with the same caller and input, and has the device
answer. The
device answers with its own passkey whatever the options list, as an
attacker's device would, so a test can show another person's passkey being
refused. A request to a route with `confirm:` the test didn't confirm goes
without an answer.

```jadpo fragment
test "a passkey signs Ada in" {
    var ada: Principal.user = member_called("ada")
    var phone: TestPasskey = test_passkeys.register(ada, "Phone")
    var response: Response = test_passkeys.sign_in(phone)
    assert response.status == 204
}
```

## What the compiler checks of sign-in
<!-- topic: what-the-compiler-checks-of-sign-in -->

Jadpo checks your sign-in setup when it compiles, so a mistake is an
error with a fix rather than a sign-in that fails in production. Each row
says what you wrote and the error you get.

### Magic link settings

| What you wrote | Error |
|---|---|
| `sign_in_providers:`, the old name | `semantic.auth_method_unknown`, whose edit renames it `sign_in:` |
| `magic_link:` without `browser_session:`, or without its `origin:` | `semantic.auth_setting_required` |
| `magic_link:` without the app's `email:` connection, and no `send:` | `email.connection_required` |
| `magic_link:` without `application { display_name: }`, which Jadpo's email names your app by | `semantic.auth_setting_required` |
| `magic_link:` without `link:` | `semantic.auth_setting_required` |
| `address:` naming anything but a field of the principal's user entity built on `Email` and `unique: true` | `semantic.auth_setting_invalid` |
| `address:` naming a field the entity doesn't have | `type.unknown_field` |
| No `address:`, and no `email` field on the entity | `semantic.auth_setting_required` |
| A setting `magic_link:` doesn't take | `semantic.auth_setting_unknown` |
| A `link:` that isn't a path on your app's site, a `path:` that isn't fixed segments, `expires_after:` under `1m`, `resend_after:` not above zero, a count under 1, or `same_browser:` that isn't `true` or `false` | `semantic.auth_setting_invalid` |
| A setting loosened past its limit, with no acceptance this change adds | `acceptance.missing` ([Acceptances](18-records-and-manifest.md#acceptances)) |
| `generate_routes:` that isn't `true` or `false`, or `false` beside `path:` | `semantic.auth_setting_invalid` |
| A route your app writes at a generated magic-link path | `route.path_overlap`; with `generate_routes: false` there are none |
| `MagicLinkAsk`, `MagicLinkConfirmation` or `MagicLinkCode` built with `{ ... }` anywhere but a test, fixture or seed | `type.not_record` |
| A request to `/auth/magic-link/redeem` without `api_token:`, which generates no such route | `semantic.request_route_unknown` |

### The first sign-in action

`on_first_sign_in:` naming anything but an action that takes one
`FirstSignIn` and returns the user entity's `id` is
`semantic.auth_setting_invalid`.

### Your own magic-link email

| What you wrote | Error |
|---|---|
| `email:` and `send:` together | `semantic.auth_setting_invalid` |
| `email:` naming anything but an `EmailMessage` of your app's | `semantic.auth_setting_invalid` |
| `email:` naming an email with a field, besides `to:` and `locale:`, that isn't picked from `MagicLinkEmail` | `semantic.auth_setting_invalid` |
| `send:` naming anything but an action that takes one `MagicLinkRequest` and returns `Unit` | `semantic.auth_setting_invalid` |
| `emit` of the email `email:` names | `event.emit_invalid` |
| Copying from `MagicLinkEmail` into anything but an email | `type.not_record` |
| A `SignInCode` kept in an entity | `type.auth_credential_stored` |
| A `SignInCode` in an `Output`, a failure or an event | `type.secret_exposed` |

See [Built-in emails](16-built-in-services.md) for both settings.

### Writing the magic-link routes yourself

| What you wrote | Error |
|---|---|
| `magic_links.request`, `.redeem` or `.redeem_code` with no `magic_link:`, or with its routes generated | `semantic.auth_setting_required` |
| One of them called outside an action | `type.auth_sessions_context` |
| One of them as a route's `run:` itself | `route.run_invalid` |
| A `SignInToken` kept in an entity | `type.auth_credential_stored` |
| A `SignInToken` in an `Output`, a failure, an event, an email, a message or a prompt | `type.secret_exposed` |

### Sign-in events

| What you wrote | Error |
|---|---|
| A subscriber of `SignedIn`, `SignInRefused` or `SignInCodesDisabled` with no `magic_link:` | `subscriber.event_invalid` |
| A field one of them doesn't have | `type.unknown_field` |
| `emit` of one of them | `event.emit_invalid` |
| `FirstSignIn` or a sign-in event built with `{ ... }` anywhere but a test, fixture or seed | `type.not_record` |

### Passkeys

| What you wrote | Error |
|---|---|
| `passkey:` without `browser_session:`, or without its `origin:` | `semantic.auth_setting_required` |
| `passkey:` without `magic_link:` in the same `sign_in:` | `semantic.auth_setting_required` |
| A setting `passkey:` doesn't take | `semantic.auth_setting_unknown` |
| `expires_after:` outside `1m` to `10m`, or a `path:` that isn't fixed segments | `semantic.auth_setting_invalid` |
| Anything in `sign_in:` but `magic_link:`, `passkey:` and `providers:` | `semantic.auth_method_unknown` |
| A route your app writes at a generated passkey path | `route.path_overlap` |
| A subscriber of `PasskeyAdded` or `PasskeyRemoved` with no `passkey:` | `subscriber.event_invalid` |
| `emit` of `PasskeyAdded` or `PasskeyRemoved` | `event.emit_invalid` |
| `test_passkeys.register`, `.sign_in` or `.confirm` anywhere but a test | `semantic.request_invalid` |
| `test_passkeys.register`, `.sign_in` or `.confirm` with no `passkey:` | `semantic.auth_setting_required` |
| A route's `confirm:` holding anything but `Confirmation.passkey`, on a route open to anyone or for a service | `route.confirm_invalid` |
| A route's `confirm:` with no `passkey:` | `semantic.auth_setting_required` |
| A route your app writes at a confirmed route's `…/confirmation`, or a confirmed `GET` beside a confirmed `POST` at one path | `route.path_overlap` |

### Sign-in providers

| What you wrote | Error |
|---|---|
| A name in `providers:` that isn't a connection on `Driver.oidc` | `semantic.auth_setting_invalid` |
| A provider named `magic_link`, `passkey` or `external_token`, or named twice | `semantic.auth_setting_invalid` |
| `providers:` without `browser_session:`, or without its `origin:` | `semantic.auth_setting_required` |
| `providers:` without `on_first_sign_in:` | `semantic.auth_setting_required` |
| A `Driver.oidc` connection without `issuer:`, `client_id:` or `credential:` | `connection.key_missing` |
| `issuer:` not reading a `Url` setting, `client_id:` not a `Text` one, or `credential:` not a secret one | `connection.value_invalid` |
| Any other setting on a `Driver.oidc` connection | `connection.key_invalid` |
| A route your app writes at `/auth/<name>/start` or `/auth/<name>/callback` | `route.path_overlap` |
| `SignInMethod.<name>` for a provider the app doesn't name | `type.unknown_enum_variant` |
| `test_providers.sign_in` anywhere but a test | `semantic.request_invalid` |
| `test_providers.sign_in` with no `providers:` | `semantic.auth_setting_required` |
| `TestAccount` built with `{ ... }` anywhere but a test, fixture or seed | `type.not_record` |

## Who is calling: `current_principal`
<!-- topic: who-is-calling-current-principal -->

A route reads the signed-in caller as `current_principal`, and passes it
to the action it runs like any other argument. `current_principal.user`
says the route is for users only: it is a `Principal.user`, and a service
calling the route is refused with 403 before the action runs.
`current_principal.service` works the same way for services.

```jadpo fragment
// domains/planning/actions/create_todo.jadpo
action create_todo(input: CreateTodo, caller: Principal.user) fails DueDateInPast -> TodoView {
    var owner: User.id = caller.id
    ...
}

// domains/planning/routes/todos.jadpo
POST / {
    input: CreateTodo
    output: TodoView
    run: create_todo(input, current_principal.user)
}
```

The route is written as [HTTP](07-http.md) describes.

- `current_principal` exists only in routes that need a signed-in caller.
  Actions, queries and functions get the caller as a parameter, so their
  signatures say they need one, and tests pass one in.
- An action that only needs policy's checks takes no caller.
  [Policy](09-policy.md) reads the caller from the request itself.
- A schedule or a subscriber has no caller, so an action it runs can't ask
  for one ([Jobs and events](12-jobs-and-events.md)).
- A caller can never be returned to a client, read from request input or
  put in a failure's fields (`type.auth_principal_*`), however deeply a
  field holds it: in a list, a map or an option, or in an object's field
  or a variant's data.

## Routes: signed in by default
<!-- topic: routes-signed-in-by-default -->

Every route needs a signed-in caller unless it says otherwise. You don't
write anything for that. A request without a valid session or token is
refused with a 401 before your code runs.

A route changes this with an `authentication:` line, which takes one of two
values.

### Open to anyone: `RouteAuthentication.none`

Use it for routes people call before they have an account, such as a health
check or a sign-up form.

```jadpo fragment
GET /health/live {
    authentication: RouteAuthentication.none
    output: Health
    run: live()
}
```

The route doesn't look for a session or token, so it never knows who is
calling. The compiler stops anything the route runs from reading the caller
(`type.auth_public_principal`).

Only the sign-in check is skipped. Input is still validated and limits still
count. [Policy](09-policy.md) still decides whether the route's action may
run for someone who isn't signed in. A route can be open to anyone and still
be refused by policy.

### Check the caller again: `RouteAuthentication.fresh`

Use it where acting for someone who was signed out a moment ago would do
harm, such as disabling an account, changing permissions, billing or exports.

```jadpo fragment
DELETE /users/{user_id} {
    authentication: RouteAuthentication.fresh
    run: disable_user(user_id, current_principal.user)
}
```

`fresh` only changes anything if your app sets a `revocation_delay`. By
default, Jadpo looks up the caller's record on every request, so a session
you revoke stops working at once. With a delay, for example
`revocation_delay: 5m`, sessions are trusted for up to five minutes without
that lookup, so a disabled user may keep working until then (see
[The application and revocation](#the-application-and-revocation)). A
`fresh` route always does the lookup, whatever the delay.

### Any other value

Any other `authentication:` value is a compile error. The error offers both
fixes rather than guessing, because one keeps the route protected and the
other opens it to anyone.

### Confirm one request with a passkey: `confirm: Confirmation.passkey`

Use it where a stolen session or token must not be enough, such as
releasing a package, moving money or deleting an account. The signed-in
person approves this one request with one of their own passkeys.

```jadpo check project=confirmation file=domains/payouts/routes/payouts.jadpo
use payouts/entities { Payout }
use payouts/actions { send_payout }

routes Payouts {
    prefix: /payouts

    POST /{payout_id: Payout.id}/send {
        confirm: Confirmation.passkey
        status: SuccessStatus.no_content
        run: send_payout(payout_id)
    }
}
```

A client confirms a request in three steps, around the browser's own
prompt. It first sends the same request to the route's generated
`<path>/confirmation`, with the same method, placeholders, query or body
and headers (a `GET`'s goes as a `POST`, since asking keeps a challenge),
for the options to give `navigator.credentials.get()`
(`PublicKeyCredential.parseRequestOptionsFromJSON` reads what `publicKey`
holds):

```text
POST /payouts/0192d3c4-.../send/confirmation

200 OK
{ "publicKey": { "challenge": "Vh3q...", "rpId": "payouts.example", "allowCredentials": [{ "type": "public-key", "id": "AQID..." }], "userVerification": "required", "timeout": 120000 } }
```

The browser has the person present a passkey, and the request itself
carries the device's answer in `X-Jadpo-Confirmation`, the JSON WebAuthn's
`toJSON()` writes, in base64 (`btoa(JSON.stringify(credential))`):

```text
POST /payouts/0192d3c4-.../send
X-Jadpo-Confirmation: eyJpZCI6IkFRSUQi...

204 No Content
```

- **The challenge is bound to the request.** The runtime reads the
  options request as the route itself would, so input the route would
  refuse is refused here, before anyone is asked for a passkey. It binds
  the challenge to the signed-in person and a digest of the route and
  that input, the one an `Idempotency-Key` is checked against. So the
  same input written differently, with its members in another order, is
  the same request.
- **The options list only the person's passkeys.** A person with none is
  refused with 403 `confirmation_required` rather than given options any
  passkey could answer, so a frontend sends them to add one.
- **It is checked before anything runs**: the passkey must be one the
  signed-in person registered, the answer must show the person was
  verified (a fingerprint, a face or a PIN), and the challenge must be
  unused, under two minutes old and match this request's input. Anything
  else is refused with 403 `confirmation_required`, and nothing runs. The
  answer is checked as a passkey's sign-in is
  ([Signing in](#signing-in)): the origin, the relying party, the
  signature and the counter. On success the passkey's counter, backup
  state and last use are written.
- **Each challenge works once**, so an answer can't be replayed, and one
  made for one request can't approve another. It is used up when the
  answer arrives, whether or not the answer checks. A new options request
  for the same person, route and input replaces the earlier one.
- **The two minutes are fixed.** `passkey:`'s `expires_after:` doesn't
  change them.
- **A retry confirms again.** An `Idempotency-Key` is checked after the
  confirmation, so a client repeating a request with its key presents a
  passkey again and gets the first response. A confirmation can't be
  skipped by repeating a key.
- **It needs a person signed in.** `confirm:` on a route with
  `authentication: RouteAuthentication.none`, or one that passes on
  `current_principal.service`, is `route.confirm_invalid`. Any credential
  of a signed-in person can carry an answer, since the answer is what
  proves it's them; a route that also needs their record read now adds
  `authentication: RouteAuthentication.fresh`.
- **It needs `passkey:`** in `sign_in:` (`semantic.auth_setting_required`),
  and a route that needs it says so in its API's record (`confirm passkey`)
  and OpenAPI (`x-jadpo-confirm`, with the header and the 403). The
  generated route is listed beside it in both and in `jadpo routes`, marked
  generated. A route your app writes at that path is `route.path_overlap`,
  as is a confirmed `GET` beside a confirmed `POST`, which would share one.
- **Taking it off needs a person.** A route that stops asking for a
  confirmation needs an acceptance, `PolicyChange.confirmation_removed`
  ([Records](18-records-and-manifest.md)), since a stolen session is then
  enough again.
- **In route tests**, `test_passkeys.confirm(passkey)` answers the next
  confirmation with that test passkey
  ([Passkeys in tests](#passkeys-in-tests)).

```jadpo check project=confirmation file=config.jadpo hidden
config {
    browser_origin: Url {
        test: "https://payouts.example"
    }
    mail_sender: Email
    smtp_host: Hostname
}
```

```jadpo check project=confirmation file=app.jadpo hidden
use payouts/entities { Treasurer }

application {
    display_name: "Payouts"
    connections: {
        mail: {
            driver: Driver.smtp
            host: config.smtp_host
        }
    }
    email: {
        connection: mail
        from: config.mail_sender
    }
}

principal {
    user: {
        subject: Treasurer.email
    }
}
```

```jadpo check project=confirmation file=authentication.jadpo hidden
authentication {
    browser_session: {
        cookie: "payouts_session"
        origin: config.browser_origin
    }
    sign_in: {
        magic_link: {
            link: "/sign-in/confirm"
        }
        passkey: {}
    }
}
```

```jadpo check project=confirmation file=domains/payouts/entities/treasurer.jadpo hidden
entity Treasurer {
    id: Uuid
    email: Email {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
    }
}
```

```jadpo check project=confirmation file=domains/payouts/entities/payout.jadpo hidden
use payouts/entities { Treasurer }

entity Payout {
    id: Uuid
    treasurer_id: Treasurer.id {
        on_delete: OnDelete.restricted
    }
    paid: Bool

    policy: {
        create: [treasurer_id]
        read: [treasurer_id]
        update: [treasurer_id]
    }
}
```

```jadpo check project=confirmation file=domains/payouts/failures/email_taken.jadpo hidden
/// The address has an account already.
type EmailTaken = Failure {
    kind: FailureKind.conflict
}
```

```jadpo check project=confirmation file=domains/payouts/failures/payout_not_found.jadpo hidden
/// No such payout.
type PayoutNotFound = Failure {
    kind: FailureKind.not_found
}
```

```jadpo check project=confirmation file=domains/payouts/types/enrolment.jadpo hidden
type Enrolment = Input {
    email: Email
}
```

```jadpo check project=confirmation file=domains/payouts/actions/enrol_treasurer.jadpo hidden
use payouts/entities { Treasurer }
use payouts/types { Enrolment }
use payouts/failures { EmailTaken }

/// Adds a treasurer with an address.
action enrol_treasurer(input: Enrolment) fails EmailTaken -> Treasurer {
    return attempt Treasurer.create {
        set: {
            email: input.email
        }
        conflict: {
            Treasurer.email: EmailTaken
        }
    }
}
```

```jadpo check project=confirmation file=domains/payouts/actions/draft_payout.jadpo hidden
use payouts/entities { Payout }

/// Starts a payout nobody has sent.
action draft_payout(treasurer: Principal.user) -> Payout {
    return Payout.create {
        set: {
            treasurer_id: treasurer.id
            paid: false
        }
    }
}
```

```jadpo check project=confirmation file=domains/payouts/actions/send_payout.jadpo hidden
use payouts/entities { Payout }
use payouts/failures { PayoutNotFound }

/// Sends a payout out.
action send_payout(payout_id: Payout.id) fails PayoutNotFound -> Unit {
    var sent: Payout = attempt Payout.update(payout_id) {
        set: {
            paid: true
        }
    }
    assert sent.paid
}
```

```jadpo check project=confirmation file=policy.jadpo hidden
use payouts

policy {
    actions: {
        payouts.enrol_treasurer: [Access.public]
    }
}
```

```jadpo check project=confirmation file=domains/payouts/fixtures/treasurer_called.jadpo hidden
use payouts/entities { Treasurer }
use payouts/types { Enrolment }
use payouts/actions { enrol_treasurer }

/// A treasurer with `address`, with no passkey.
fixture treasurer_called(address: Email) -> Principal.user {
    var treasurer: Treasurer = attempt enrol_treasurer(
        Enrolment {
            email: address
        }
    )
    return Principal.user(treasurer)
}
```

```jadpo check project=confirmation file=domains/payouts/fixtures/drafted_by.jadpo hidden migrate=initial
use payouts/entities { Payout }
use payouts/actions { draft_payout }

/// A payout of `treasurer`'s that nobody has sent.
fixture drafted_by(treasurer: Principal.user) -> Payout {
    return draft_payout(treasurer)
}
```

A route test confirms the next request with a test passkey. Without it,
or with another person's, nothing runs:

```jadpo check project=confirmation file=domains/payouts/routes/payouts.test.jadpo
use payouts/entities { Payout }
use payouts/fixtures { drafted_by, treasurer_called }

test "a payout goes out once its treasurer confirms it with a passkey" {
    var ada: Principal.user = treasurer_called("ada@example.com")
    var phone: TestPasskey = test_passkeys.register(ada, "Phone")
    var payout: Payout = drafted_by(ada)
    test_passkeys.confirm(phone)
    var response: Response = request {
        method: HttpMethod.post
        path: "/payouts/{payout_id}/send"
        params: {
            payout_id: payout.id
        }
        as: ada
    }
    assert response.status == 204
}

test "a stolen session alone sends nothing" {
    var ada: Principal.user = treasurer_called("ada@example.com")
    var phone: TestPasskey = test_passkeys.register(ada, "Phone")
    var payout: Payout = drafted_by(ada)
    var response: Response = request {
        method: HttpMethod.post
        path: "/payouts/{payout_id}/send"
        params: {
            payout_id: payout.id
        }
        as: ada
    }
    assert response.status == 403
    assert response.code == "confirmation_required"
    var signed_in: Response = test_passkeys.sign_in(phone)
    assert signed_in.status == 204
}

test "another person's passkey confirms nothing" {
    var ada: Principal.user = treasurer_called("ada@example.com")
    var mallory: Principal.user = treasurer_called("mallory@example.com")
    var ada_phone: TestPasskey = test_passkeys.register(ada, "Phone")
    var mallory_phone: TestPasskey = test_passkeys.register(mallory, "Phone")
    var payout: Payout = drafted_by(ada)
    test_passkeys.confirm(mallory_phone)
    var response: Response = request {
        method: HttpMethod.post
        path: "/payouts/{payout_id}/send"
        params: {
            payout_id: payout.id
        }
        as: ada
    }
    assert response.status == 403
    assert response.code == "confirmation_required"
    var signed_in: Response = test_passkeys.sign_in(ada_phone)
    assert signed_in.status == 204
}
```

The API record says the route asks for a passkey, and lists the route Jadpo
answers beside it:

```text
POST /payouts/{payout_id: Uuid}/send
    confirm passkey
    status 204
    fails payout_not_found

POST /payouts/{payout_id: Uuid}/send/confirmation
    generated confirmation
    caller user
    output PasskeyRequestOptions
    status 200
```

### Apps with more than one API

An app can serve several APIs, for example `main` for its own clients and
`partners` for other companies' servers, each under its own path prefix
([APIs](07-http.md)). Each API lists the ways a caller may prove who they
are, and every route in it accepts those:

```jadpo fragment
// app.jadpo
apis: {
    main: {
        prefix: /
        authentication: [AuthenticationMethod.browser_session, AuthenticationMethod.api_token]
    }
    partners: {
        prefix: /partners
        authentication: [AuthenticationMethod.service_key]
    }
}
```

Here a browser session or an API token works on any route under `/`, and
only a service key works under `/partners`. A route can narrow its API's
list but never add to it. An app with a single API accepts every method.

Webhooks from a provider such as Stripe aren't routes you write, so there's
nothing here to set. Jadpo generates them, checks each one's signature and
passes it to your app as an event ([Integrations](11-integrations.md)).

## Service credentials
<!-- topic: service-credentials -->

A service, such as a partner's system or a worker, signs in with a key.
Each service has an owner, a status, and any number of keys. Each key
expires, can be revoked on its own, and can overlap with the next, so you
can rotate a key without downtime. A key is shown once when it is issued,
and Jadpo keeps only a verifier of it.

You declare only the service's entity:

```jadpo fragment
// domains/identity/entities/service.jadpo
entity Service {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.restricted
    }
    name: Text {
        unique: true
    }
    status: ServiceStatus
}
```

Jadpo stores the keys, as it stores browser sessions, and generates three
operations on the entity to issue, list and revoke them:
`Service.issue_key(service_id, expires_at)`, `Service.keys(service_id)` and
`Service.revoke_key(key_id)`.

- The operations belong to the entity the principal's `service:` names,
  once `authentication { }` declares `service_key:`.
- Only an action calls them (`type.auth_sessions_context`), as it calls
  `sessions.start`. A `GET` route can also run `keys` itself
  ([Listing from a `GET` route](#listing-from-a-get-route)).
- `Service.issue_key(service_id, expires_at)` takes the service's `id`
  and an `Instant`, keeps the new key in the action's transaction and
  gives it once as a `ServiceKey`, text an `Output` can send; the
  response that carries it is never kept for an idempotent repeat, and a
  `ServiceKey` can't be stored in an entity.
- `Service.keys(service_id)` lists the service's keys that haven't
  expired, oldest first, as `IssuedKey`s, which code reads `id`,
  `started_at` and `ends_at` from, never the key itself. A `GET` route
  runs it too, and an `Output` can send the list
  ([Listing from a `GET` route](#listing-from-a-get-route)).
- `Service.revoke_key(key_id)` deletes one by its `id`; an `id` no key
  has revokes nothing.
- Keys live in the `jadpo_service_keys` system table, in the store the
  service entity lives in ([System tables](05-data.md#system-tables)).
  Jadpo looks a key up with its service's record, `active:` and all, in
  one statement, so a disabled service is refused (`service_disabled`) on
  its next request.
- A service key signs in only a service. A route that selects
  `current_principal.user` refuses it with 403 `not_permitted`, and one
  that selects `current_principal.service` refuses a user the same way.

## Starting and ending a session
<!-- topic: starting-and-ending-a-session -->

Magic links, passkeys and providers start the session themselves
([Checking the person](#checking-the-person-sign_in)). Jadpo has no
password sign-in.

When your app checks a person some other way, such as someone accepting
an invitation it sent, the action that checks them starts the session
with `sessions.start`. `sessions.end` signs someone out. Jadpo sets and
clears the cookie and its CSRF token on the route's response:

```jadpo fragment
action accept_invitation(token: InvitationToken) fails InvitationExpired -> Welcome {
    var invitation: Invitation = attempt Invitation.redeem(token)
    sessions.start(invitation.member_id)
    ...
}

action sign_out() -> Unit {
    sessions.end()
}
```

`sessions.start(invitation.member_id)` starts a session for that member,
and the response of the route that ran the action sets its cookie.
`sessions.end()` ends the session the request came with.

- `sessions.start` keeps the session in the action's own transaction, in
  the `jadpo_sessions` system table of the store the principal's user
  entity lives in ([System tables](05-data.md#system-tables)). So an action
  that fails starts no session. The route's response sets the cookies once
  the action commits.
- `sessions.end` deletes the session the request came with, and the
  response clears the cookies.
- A session or token past its end, which a lookup ignores, and a service
  key past its expiry, are deleted on the tick, Jadpo's regular background
  sweep, in the store each is kept in
  ([Jobs and events](12-jobs-and-events.md)).
- Only an action calls them (`type.auth_sessions_context`). A test reaches
  them through the route, or calls the action, which keeps the session
  without a response to set it on.
- Only verifiers are kept: a session's token and its CSRF token are 32
  random bytes each, stored as their digests, so a copy of the database
  signs no one in.

### How a session started

Each session and API token records the way in that started it, its
`SignInMethod`, and how strongly the person signed in. A role can need
one ([Roles that need a way in](09-policy.md#roles-that-need-a-way-in)),
and the check runs on every request against what the session recorded.

- A magic link records `magic_link`, a passkey `passkey`, and a provider
  its connection's name, such as `google`. A request with an external
  token signs in through `external_token` each time.
- `sessions.start` and `sessions.issue_token` record no way in, since
  your app checked the person itself. A role that needs a way in never
  counts for such a session.
- A session started through a provider that any role's requirement
  names lasts 24 hours, not 30 days, so someone the company switches off
  at the provider loses the role within a day. After that its requests
  are 401 `invalid_credentials`, and the person signs in again.
- Sessions kept before ways in were recorded have none, so a role that
  needs one returns at the person's next sign-in through the provider.

### How strongly a person signed in

`current_principal.strength` is `multi_factor` when the sign-in proved a
second factor, and `primary` otherwise:

- a passkey is two factors, the device and what unlocks it;
- a provider's or an external token's sign-in is two when its token's
  `amr` (RFC 8176) says so: `mfa`, or something the person knows
  (`pwd`, `pin`, `kba`) beside something they hold or are (`hwk`, `swk`,
  `sc`, `pop`, `otp`, `sms`, `tel`, `fpt`, `face`, `iris`, `retina`,
  `vbm`). When a role's requirement needs a second factor, the sign-in
  asks the provider for `amr` through the OpenID Connect `claims`
  parameter. Microsoft sends it; Google sends it to verified apps that
  ask;
- a magic link is one.

### API tokens

Your own mobile and command-line clients sign in with an API token. An
action issues one once it has checked the person, and returns it in its
output. That is the one time anyone sees it:

```jadpo fragment
action accept_invitation_in_app(token: InvitationToken) fails InvitationExpired -> AppSignedIn {
    var invitation: Invitation = attempt Invitation.redeem(token)
    return AppSignedIn {
        token: sessions.issue_token(invitation.member_id)
    }
}
```

`sessions.issue_token` gives an `ApiToken`, which the client keeps and
sends as `Authorization: Bearer`. With magic links, the generated
`POST /auth/magic-link/redeem` trades a code for a token instead
([Magic links](#magic-links)).

- `sessions.issue_token(user_id)` takes the user entity's `id` and gives
  an `ApiToken`, text an `Output` can send. The token is a session without
  a cookie: kept as a verifier in `jadpo_sessions` in the action's
  transaction, lasting 30 days from when it is issued, with no CSRF token,
  since a browser never sends it by itself. The response that carries it
  is never kept for an idempotent repeat, and an `ApiToken` can't be
  stored in an entity.
- `sessions.end()` ends the token a request came with, as it ends a
  browser session.
- `sessions.tokens(user_id)` lists the user's tokens that haven't ended,
  oldest first, as `IssuedToken`s, which code reads `id`, `started_at`
  and `ends_at` from; `sessions.revoke_token(token_id)` ends one by its
  `id`, and an `id` no token has ends nothing. Together they let an
  application end a user's other devices.
- Like `sessions.start`, only an action calls them, except that a `GET`
  route can run `sessions.tokens` itself (below). Each needs `api_token:`
  in `authentication { }`.

#### Listing from a `GET` route

An application's "signed-in devices" screen is one route. A `GET` runs
`sessions.tokens(user_id)`, and `Service.keys(service_id)` for a service's
keys, directly, as generated queries that read what Jadpo keeps:

```jadpo fragment
routes Devices {
    prefix: /devices

    GET / {
        output: List<IssuedToken>
        run: sessions.tokens(current_principal.user.id)
    }
}
```

- `run:` passes the holder's `id`, from a placeholder or from
  `current_principal`, and nothing else; the route's `output:` is
  `List<IssuedToken>` (`List<IssuedKey>` for keys), exactly what the
  listing returns. Any other method is `route.run_kind`, and the writes
  (`sessions.issue_token`, `.revoke_token`, `issue_key`, `revoke_key`) are
  `route.run_invalid`: an action calls those.
- `IssuedToken` and `IssuedKey` are built-in values an `Output` can hold
  (`tokens: List<IssuedToken>`) and a route can send. On the wire each is
  an object of its `id`, `started_at` and `ends_at`, never the credential;
  the API record and OpenAPI give it a section of its own.
- The list goes through [Policy](09-policy.md) like any query's. It holds
  the tokens or keys of a holder whose record the caller may `read`, and is
  empty for any other.
- A route that lists a holder nobody may read is
  `policy.effect_ungranted`. One anyone may call needs `Access.public` on
  the holder's `read:`.

### Signing in, end to end

A whole app that signs members up, keeps their notes and refuses a
suspended member. To show sessions on their own, signing up here starts a
session for a handle without checking the person. A real app checks the
person with [`sign_in:`](#checking-the-person-sign_in). The principal and
its one way in:

```jadpo check project=sign-in file=app.jadpo
use journal/entities { Member }
use journal/types { MemberStatus }

application {}

principal {
    user: {
        subject: Member.handle
        active: Member.status == MemberStatus.active
    }
}
```

```jadpo check project=sign-in file=authentication.jadpo
authentication {
    browser_session: {
        cookie: "notes_session"
    }
}
```

```jadpo check project=sign-in file=domains/journal/types/handle.jadpo hidden
type Handle = Text {
    min_length: 3
    max_length: 40
}
```

```jadpo check project=sign-in file=domains/journal/types/member_status.jadpo hidden
type MemberStatus = Enum {
    active
    suspended
}
```

```jadpo check project=sign-in file=domains/journal/types/note_text.jadpo hidden
type NoteText = Text {
    min_length: 1
    max_length: 500
}
```

```jadpo check project=sign-in file=domains/journal/types/note_view.jadpo hidden
use journal/types { NoteText }

type NoteView = Output {
    text: NoteText
}
```

```jadpo check project=sign-in file=domains/journal/entities/member.jadpo hidden
use journal/types { Handle, MemberStatus }

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
    status: MemberStatus

    policy: {
        create: [Access.public]
        read: [id]
    }
}
```

```jadpo check project=sign-in file=domains/journal/entities/note.jadpo hidden
use journal/entities { Member }
use journal/types { NoteText }

entity Note {
    id: Uuid
    author_id: Member.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText

    policy: {
        create: [author_id]
        read: [author_id]
    }
}
```

```jadpo check project=sign-in file=domains/journal/failures/handle_taken.jadpo hidden
type HandleTaken = Failure {
    kind: FailureKind.conflict
    message: "That handle is taken."
}
```

```jadpo check project=sign-in file=domains/journal/actions/register.jadpo hidden
use journal/entities { Member }
use journal/types { Handle, MemberStatus }
use journal/failures { HandleTaken }

action register(handle: Handle) fails HandleTaken -> Member {
    return attempt Member.create {
        set: {
            handle: handle
            status: MemberStatus.active
        }
        conflict: {
            Member.handle: HandleTaken
        }
    }
}
```

```jadpo check project=sign-in file=domains/journal/actions/suspend.jadpo hidden
use journal/entities { Member, MemberNotFound }
use journal/types { MemberStatus }

action suspend(member_id: Member.id) fails MemberNotFound -> Member {
    return attempt Member.update(member_id) {
        set: {
            status: MemberStatus.suspended
        }
    }
}
```

Signing up checks the handle, starts a session and answers with no content;
its response sets the cookies. Writing a note takes the signed-in member as
a parameter:

```jadpo check project=sign-in file=domains/journal/actions/sign_up.jadpo
use journal/entities { Member }
use journal/types { Handle }
use journal/failures { HandleTaken }
use journal/actions { register }

/// What signing up takes.
type SignUp = Input {
    handle: Handle
}

/// Registers a member and signs them in.
action sign_up(input: SignUp) fails HandleTaken -> Unit {
    var member: Member = attempt register(input.handle)
    sessions.start(member.id)
}
```

```jadpo check project=sign-in file=domains/journal/actions/sign_out.jadpo
/// Ends the session the request came with.
action sign_out() -> Unit {
    sessions.end()
}
```

```jadpo check project=sign-in file=domains/journal/actions/write_note.jadpo
use journal/entities { Note }
use journal/types { NoteText, NoteView }

/// What writing a note takes.
type WriteNote = Input {
    text: NoteText
}

/// Writes a note as the signed-in member.
action write_note(input: WriteNote, author: Principal.user) -> NoteView {
    var note: Note = Note.create {
        set: {
            author_id: author.id
            text: input.text
        }
    }
    return NoteView {
        text: note.text
    }
}
```

Signing up is the one route anyone may call; every other needs the session:

```jadpo check project=sign-in file=domains/journal/routes/account.jadpo
use journal/actions { SignUp, sign_out, sign_up }

/// Signing up and out.
routes Account {
    prefix: /account

    /// Anyone may sign up; the response sets the session's cookies.
    POST /sign-up {
        authentication: RouteAuthentication.none
        input: SignUp
        status: SuccessStatus.no_content
        run: sign_up(input)
    }

    /// Ends the session and clears its cookies.
    POST /sign-out {
        status: SuccessStatus.no_content
        run: sign_out()
    }
}
```

```jadpo check project=sign-in file=domains/journal/routes/notes.jadpo migrate=initial
use journal/types { NoteView }
use journal/actions { WriteNote, write_note }

/// The signed-in member's notes.
routes Notes {
    prefix: /notes

    POST / {
        input: WriteNote
        output: NoteView
        status: SuccessStatus.created
        run: write_note(input, current_principal.user)
    }
}
```

```jadpo check project=sign-in file=api/main.jadpo_record hidden
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

api main
    authentication browser_session

POST /account/sign-out
    status 204

POST /account/sign-up
    authentication none
    input SignUp (body)
    status 204
    fails handle_taken

POST /notes
    caller user
    input WriteNote (body)
    output NoteView
    status 201

GET /openapi.json
    generated openapi
    status 200

output NoteView
    text: Text { min_length: 1, max_length: 500 }

input SignUp
    handle: Text { min_length: 3, max_length: 40 }

input WriteNote
    text: Text { min_length: 1, max_length: 500 }

failure handle_taken
    status 409
    message "That handle is taken."
```

A route test signs in with `as:` through the real browser session (a session
kept for the member, its cookie, its CSRF token and an `Origin`), so a
suspended member is refused exactly as a browser would be:

```jadpo check project=sign-in file=domains/journal/routes/notes.test.jadpo
use journal/entities { Member }
use journal/actions { WriteNote, register, suspend }

test "a note needs someone signed in" {
    var response: Response = request {
        method: HttpMethod.post
        path: "/notes"
        input: WriteNote {
            text: "Buy milk"
        }
    }
    assert response.status == 401
    assert response.code == "authentication_required"
}

test "a signed-in member writes a note" {
    var member: Member = attempt register("ann")
    var response: Response = request {
        method: HttpMethod.post
        path: "/notes"
        as: Principal.user(member)
        input: WriteNote {
            text: "Buy milk"
        }
    }
    assert response.status == 201
}

test "a suspended member is refused" {
    var member: Member = attempt register("bob")
    var suspended: Member = attempt suspend(member.id)
    var response: Response = request {
        method: HttpMethod.post
        path: "/notes"
        as: Principal.user(suspended)
        input: WriteNote {
            text: "Buy milk"
        }
    }
    assert response.status == 403
    assert response.code == "user_disabled"
}
```

### Tokens and keys, end to end

The same notes, written from the members' app with an API token, and a
partner system that reads itself with a service key. Both kinds of caller
are declared, and both bearer methods:

```jadpo check project=tokens-and-keys file=app.jadpo
use journal/entities { Member, Partner }
use journal/types { MemberStatus, PartnerStatus }

application {}

principal {
    user: {
        subject: Member.handle
        active: Member.status == MemberStatus.active
    }
    service: {
        subject: Partner.name
        active: Partner.status == PartnerStatus.active
    }
}
```

```jadpo check project=tokens-and-keys file=authentication.jadpo
authentication {
    api_token: {}
    service_key: {}
}
```

```jadpo check project=tokens-and-keys file=domains/journal/types/handle.jadpo hidden
type Handle = Text {
    min_length: 3
    max_length: 40
}
```

```jadpo check project=tokens-and-keys file=domains/journal/types/member_status.jadpo hidden
type MemberStatus = Enum {
    active
    suspended
}
```

```jadpo check project=tokens-and-keys file=domains/journal/types/note_text.jadpo hidden
type NoteText = Text {
    min_length: 1
    max_length: 500
}
```

```jadpo check project=tokens-and-keys file=domains/journal/types/note_view.jadpo hidden
use journal/types { NoteText }

type NoteView = Output {
    text: NoteText
}
```

```jadpo check project=tokens-and-keys file=domains/journal/types/partner_name.jadpo hidden
type PartnerName = Text {
    min_length: 1
    max_length: 60
}
```

```jadpo check project=tokens-and-keys file=domains/journal/types/partner_status.jadpo hidden
type PartnerStatus = Enum {
    active
    suspended
}
```

```jadpo check project=tokens-and-keys file=domains/journal/types/partner_view.jadpo hidden
use journal/types { PartnerName }

type PartnerView = Output {
    name: PartnerName
}
```

```jadpo check project=tokens-and-keys file=domains/journal/entities/member.jadpo hidden
use journal/types { Handle, MemberStatus }

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
    status: MemberStatus

    policy: {
        create: [Access.public]
        read: [id]
        update: [id]
    }
}
```

```jadpo check project=tokens-and-keys file=domains/journal/entities/note.jadpo hidden
use journal/entities { Member }
use journal/types { NoteText }

entity Note {
    id: Uuid
    author_id: Member.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText

    policy: {
        create: [author_id]
        read: [author_id]
    }
}
```

```jadpo check project=tokens-and-keys file=domains/journal/entities/partner.jadpo
use journal/entities { Member }
use journal/types { PartnerName, PartnerStatus }

/// Another system that reads the journal with its keys, managed by the
/// member who added it.
entity Partner {
    id: Uuid
    owner_id: Member.id {
        on_delete: OnDelete.restricted
    }
    name: PartnerName {
        unique: true
    }
    status: PartnerStatus

    policy: {
        create: [owner_id]
        read: [id, owner_id]
        update: [owner_id]
    }
}
```

```jadpo check project=tokens-and-keys file=domains/journal/failures/handle_taken.jadpo hidden
type HandleTaken = Failure {
    kind: FailureKind.conflict
    message: "That handle is taken."
}
```

```jadpo check project=tokens-and-keys file=domains/journal/failures/partner_name_taken.jadpo hidden
type PartnerNameTaken = Failure {
    kind: FailureKind.conflict
    message: "That partner already exists."
}
```

```jadpo check project=tokens-and-keys file=domains/journal/actions/register.jadpo hidden
use journal/entities { Member }
use journal/types { Handle, MemberStatus }
use journal/failures { HandleTaken }

action register(handle: Handle) fails HandleTaken -> Member {
    return attempt Member.create {
        set: {
            handle: handle
            status: MemberStatus.active
        }
        conflict: {
            Member.handle: HandleTaken
        }
    }
}
```

```jadpo check project=tokens-and-keys file=domains/journal/actions/create_partner.jadpo hidden
use journal/entities { Partner }
use journal/types { PartnerName, PartnerStatus }
use journal/failures { PartnerNameTaken }

action create_partner(name: PartnerName, owner: Principal.user) fails PartnerNameTaken -> Partner {
    return attempt Partner.create {
        set: {
            owner_id: owner.id
            name: name
            status: PartnerStatus.active
        }
        conflict: {
            Partner.name: PartnerNameTaken
        }
    }
}
```

```jadpo check project=tokens-and-keys file=domains/journal/actions/suspend_partner.jadpo hidden
use journal/entities { Partner, PartnerNotFound }
use journal/types { PartnerStatus }

action suspend_partner(partner_id: Partner.id) fails PartnerNotFound -> Partner {
    return attempt Partner.update(partner_id) {
        set: {
            status: PartnerStatus.suspended
        }
    }
}
```

```jadpo check project=tokens-and-keys file=domains/journal/actions/sign_out.jadpo hidden
/// Ends the token the request came with.
action sign_out() -> Unit {
    sessions.end()
}
```

```jadpo check project=tokens-and-keys file=domains/journal/actions/write_note.jadpo hidden
use journal/entities { Note }
use journal/types { NoteText, NoteView }

/// What writing a note takes.
type WriteNote = Input {
    text: NoteText
}

/// Writes a note as the signed-in member.
action write_note(input: WriteNote, author: Principal.user) -> NoteView {
    var note: Note = Note.create {
        set: {
            author_id: author.id
            text: input.text
        }
    }
    return NoteView {
        text: note.text
    }
}
```

Signing up issues the app's token and answers with it, once; ending
every device lists the member's tokens and revokes each:

```jadpo check project=tokens-and-keys file=domains/journal/actions/sign_up.jadpo
use journal/entities { Member }
use journal/types { Handle }
use journal/failures { HandleTaken }
use journal/actions { register }

/// What signing up takes.
type SignUp = Input {
    handle: Handle
}

/// The app's token, which it sends back as `Authorization: Bearer`.
type SignedUp = Output {
    token: ApiToken
}

/// Registers a member and gives their app a token, shown once.
action sign_up(input: SignUp) fails HandleTaken -> SignedUp {
    var member: Member = attempt register(input.handle)
    return SignedUp {
        token: sessions.issue_token(member.id)
    }
}
```

```jadpo check project=tokens-and-keys file=domains/journal/actions/sign_out_everywhere.jadpo
/// Ends every token the member's apps hold, this one's too.
action sign_out_everywhere(member: Principal.user) -> Unit {
    for token in sessions.tokens(member.id) {
        sessions.revoke_token(token.id)
    }
}
```

Adding a partner issues its first key, which expires when the input
says; the partner reads itself with it. The member who adds a partner owns
it, so its grants let them issue and revoke its keys
([Grants](09-policy.md#grants)):

```jadpo check project=tokens-and-keys file=domains/journal/actions/add_partner.jadpo
use journal/entities { Partner }
use journal/types { PartnerName }
use journal/failures { PartnerNameTaken }
use journal/actions { create_partner }

/// What adding a partner takes.
type AddPartner = Input {
    name: PartnerName
    key_expires_at: Instant
}

/// The partner's key, which it sends as `Authorization: Bearer`.
type PartnerAdded = Output {
    key: ServiceKey
}

/// Adds a partner and gives its first key, shown once.
action add_partner(
    input: AddPartner,
    owner: Principal.user
) fails PartnerNameTaken -> PartnerAdded {
    var partner: Partner = attempt create_partner(input.name, owner)
    return PartnerAdded {
        key: Partner.issue_key(partner.id, input.key_expires_at)
    }
}
```

```jadpo check project=tokens-and-keys file=domains/journal/queries/this_partner.jadpo
use journal/entities { Partner, PartnerNotFound }
use journal/types { PartnerView }

/// The partner a key belongs to.
query this_partner(partner: Principal.service) fails PartnerNotFound -> PartnerView {
    from: Partner
    where: Partner.id == partner.id
    missing: PartnerNotFound
}
```

```jadpo check project=tokens-and-keys file=domains/journal/routes/account.jadpo
use journal/actions { SignUp, SignedUp, sign_out, sign_out_everywhere, sign_up }

/// Signing up and out, from the app.
routes Account {
    prefix: /account

    /// Anyone may sign up; the response holds the app's token.
    POST /sign-up {
        authentication: RouteAuthentication.none
        input: SignUp
        output: SignedUp
        status: SuccessStatus.created
        run: sign_up(input)
    }

    /// Ends the token the request came with.
    POST /sign-out {
        status: SuccessStatus.no_content
        run: sign_out()
    }

    /// Ends every token the member's apps hold.
    POST /sign-out-everywhere {
        status: SuccessStatus.no_content
        run: sign_out_everywhere(current_principal.user)
    }
}
```

A member's own devices are one `GET` route, and so are another member's,
for whoever may read their record. The list is limited the way a query's
records are ([Policy](09-policy.md)), so it is empty for a member the
caller may not read. `Member` grants a read of itself (`read: [id]`):

```jadpo check project=tokens-and-keys file=domains/journal/routes/devices.jadpo
use journal/entities { Member }

/// The devices that hold a token for a member.
routes Devices {
    prefix: /devices

    /// The signed-in member's own, oldest first.
    GET / {
        output: List<IssuedToken>
        run: sessions.tokens(current_principal.user.id)
    }

    /// Another member's: a list holds only the tokens of members the
    /// caller may read, so it is empty for anyone else's.
    GET /{member_id: Member.id} {
        output: List<IssuedToken>
        run: sessions.tokens(member_id)
    }
}
```

```jadpo check project=tokens-and-keys file=domains/journal/routes/notes.jadpo hidden
use journal/types { NoteView }
use journal/actions { WriteNote, write_note }

/// The signed-in member's notes.
routes Notes {
    prefix: /notes

    POST / {
        input: WriteNote
        output: NoteView
        status: SuccessStatus.created
        run: write_note(input, current_principal.user)
    }
}
```

```jadpo check project=tokens-and-keys file=domains/journal/routes/partners.jadpo migrate=initial
use journal/entities { Partner }
use journal/types { PartnerView }
use journal/actions { AddPartner, PartnerAdded, add_partner }
use journal/queries { this_partner }

/// Partners and their keys.
routes Partners {
    prefix: /partners

    /// A signed-in member adds a partner; the response holds its key.
    POST / {
        input: AddPartner
        output: PartnerAdded
        status: SuccessStatus.created
        run: add_partner(input, current_principal.user)
    }

    /// A partner reads itself, signed in with its key.
    GET /me {
        output: PartnerView
        run: this_partner(current_principal.service)
    }

    /// The keys a partner holds that haven't expired, for the member who
    /// owns it.
    GET /{partner_id: Partner.id}/keys {
        output: List<IssuedKey>
        run: Partner.keys(partner_id)
    }
}
```

```jadpo check project=tokens-and-keys file=api/main.jadpo_record hidden
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

api main
    authentication api_token
    authentication service_key

POST /account/sign-out
    status 204

POST /account/sign-out-everywhere
    caller user
    status 204

POST /account/sign-up
    authentication none
    input SignUp (body)
    output SignedUp
    status 201
    fails handle_taken

GET /devices
    caller user
    output List<IssuedToken>
    status 200

GET /devices/{member_id: Uuid}
    output List<IssuedToken>
    status 200

POST /notes
    caller user
    input WriteNote (body)
    output NoteView
    status 201

GET /openapi.json
    generated openapi
    status 200

POST /partners
    caller user
    input AddPartner (body)
    output PartnerAdded
    status 201
    fails partner_name_taken

GET /partners/me
    caller service
    output PartnerView
    status 200
    fails partner_not_found

GET /partners/{partner_id: Uuid}/keys
    output List<IssuedKey>
    status 200

input AddPartner
    key_expires_at: Instant
    name: Text { min_length: 1, max_length: 60 }

output IssuedKey
    ends_at: Instant
    id: Uuid
    started_at: Instant

output IssuedToken
    ends_at: Instant
    id: Uuid
    started_at: Instant

output NoteView
    text: Text { min_length: 1, max_length: 500 }

output PartnerAdded
    key: ServiceKey

output PartnerView
    name: Text { min_length: 1, max_length: 60 }

input SignUp
    handle: Text { min_length: 3, max_length: 40 }

output SignedUp
    token: ApiToken

input WriteNote
    text: Text { min_length: 1, max_length: 500 }

failure handle_taken
    status 409
    message "That handle is taken."

failure partner_name_taken
    status 409
    message "That partner already exists."

failure partner_not_found
    status 404
```

A route test signs in with `as:` through the method the route's API
accepts: a member with an API token here, since nothing declares browser
sessions, and a partner with a key from `Partner.issue_key`, each kept for
real and sent as `Authorization: Bearer`:

```jadpo check project=tokens-and-keys file=domains/journal/routes/partners.test.jadpo
use journal/entities { Member, Partner }
use journal/types { PartnerView }
use journal/actions { WriteNote, create_partner, register, suspend_partner }

test "a member writes a note with the app's token" {
    var member: Member = attempt register("ann")
    var response: Response = request {
        method: HttpMethod.post
        path: "/notes"
        as: Principal.user(member)
        input: WriteNote {
            text: "Buy milk"
        }
    }
    assert response.status == 201
}

test "a partner reads itself with its key" {
    var ann: Member = attempt register("ann")
    var partner: Partner = attempt create_partner("Acme", Principal.user(ann))
    var response: Response = request {
        method: HttpMethod.get
        path: "/partners/me"
        as: Principal.service(partner)
    }
    assert response.status == 200
    assert response.output == PartnerView {
        name: "Acme"
    }
}

test "a partner can't write a member's note" {
    var ann: Member = attempt register("ann")
    var partner: Partner = attempt create_partner("Acme", Principal.user(ann))
    var response: Response = request {
        method: HttpMethod.post
        path: "/notes"
        as: Principal.service(partner)
        input: WriteNote {
            text: "Buy milk"
        }
    }
    assert response.status == 403
    assert response.code == "not_permitted"
}

test "a suspended partner is refused" {
    var ann: Member = attempt register("ann")
    var partner: Partner = attempt create_partner("Acme", Principal.user(ann))
    var suspended: Partner = attempt suspend_partner(partner.id)
    var response: Response = request {
        method: HttpMethod.get
        path: "/partners/me"
        as: Principal.service(suspended)
    }
    assert response.status == 403
    assert response.code == "service_disabled"
}
```

A route test lists as each member and partner would: a signed-in caller's
request keeps a real token or key, so there is one to list, and policy
decides whose:

```jadpo check project=tokens-and-keys file=domains/journal/routes/devices.test.jadpo
use journal/entities { Member, Partner }
use journal/actions { create_partner, register }

test "a member lists the devices that hold a token" {
    var ann: Member = attempt register("ann")
    var response: Response = request {
        method: HttpMethod.get
        path: "/devices"
        as: Principal.user(ann)
    }
    assert response.status == 200
    assert response.output != []
}

test "a member's tokens are listed to no one else" {
    var ann: Member = attempt register("ann")
    var bob: Member = attempt register("bob")
    var by_bob: Response = request {
        method: HttpMethod.get
        path: "/devices"
        as: Principal.user(bob)
    }
    assert by_bob.output != []
    var by_ann: Response = request {
        method: HttpMethod.get
        path: "/devices/{member_id}"
        params: {
            member_id: ann.id
        }
        as: Principal.user(ann)
    }
    assert by_ann.output != []
    var ann_on_bob: Response = request {
        method: HttpMethod.get
        path: "/devices/{member_id}"
        params: {
            member_id: bob.id
        }
        as: Principal.user(ann)
    }
    assert ann_on_bob.status == 200
    assert ann_on_bob.output == []
}

test "a partner's keys are listed to the partner and its owner" {
    var ann: Member = attempt register("ann")
    var bob: Member = attempt register("bob")
    var partner: Partner = attempt create_partner("Acme", Principal.user(ann))
    var by_bob: Response = request {
        method: HttpMethod.get
        path: "/devices"
        as: Principal.user(bob)
    }
    assert by_bob.output != []
    var by_partner: Response = request {
        method: HttpMethod.get
        path: "/partners/{partner_id}/keys"
        params: {
            partner_id: partner.id
        }
        as: Principal.service(partner)
    }
    assert by_partner.output != []
    var by_owner: Response = request {
        method: HttpMethod.get
        path: "/partners/{partner_id}/keys"
        params: {
            partner_id: partner.id
        }
        as: Principal.user(ann)
    }
    assert by_owner.output != []
    var by_member: Response = request {
        method: HttpMethod.get
        path: "/partners/{partner_id}/keys"
        params: {
            partner_id: partner.id
        }
        as: Principal.user(bob)
    }
    assert by_member.output == []
}
```

## Acting for a user at another service: OAuth grants
<!-- topic: acting-for-a-user-at-another-service-oauth-grants -->

Signing in proves to your app who a person is. An OAuth grant goes the
other way: your app acts at another service, such as a user's Google
Calendar, an organisation's Xero or a Slack workspace, with that owner's
consent. It isn't a way to sign in. Your app acting as an OAuth provider,
for other apps to sign in with, is a third, separate thing.

The connection and its calls are in
[Integrations](11-integrations.md#acting-for-each-user-oauth-grants). This
topic covers what touches identity. A connection's credential is an
`OAuthGrant`:

```text
// app.jadpo
calendar: {
    package: GoogleCalendar.v3
    domains: [scheduling]
    credential: OAuthGrant {
        owner: User
        client_id: config.google_client_id
        client_secret: config.google_client_secret
        scopes: [...]
        return_to: "/settings/integrations"
    }
}
```

- **A grant is owned by one of the app's records**: `owner:` is the
  principal's user entity or any other entity (`Organisation`). The grant
  belongs to that record, not to a session, so it outlives every session
  and is used from subscribers and schedules, which run with no caller.
- **Connecting is endpoints, not pages.** `POST /connections/<name>/start`
  is a generated route that needs a signed-in caller (`authentication:
  RouteAuthentication.none` is not available on it) and answers with the
  provider's consent address. It keeps the `state` on the server against
  the caller's browser session or API token. The provider sends the browser
  back to the generated `GET /connections/<name>/callback`, which finds the
  `state` (a consent can complete only once, for the caller who started
  it), exchanges the code with PKCE, stores the grant and redirects to the
  frontend's `return_to`, a path on the app's own site, with success or
  failure. The callback needs no cookie, so your own API-token clients
  connect exactly as browsers do. Both appear in `jadpo routes`, OpenAPI
  and the API record, marked generated
  ([Generated routes](07-http.md#generated-routes)).
- **Who may connect for an owner is [Policy](09-policy.md)'s.** Connecting
  and disconnecting need the `update` grant on the owner's record. So a
  user connects their own account, and an organisation's grant needs
  whatever lets the caller update the organisation. There is no new kind
  of permission for it. The start route checks it before answering.
- **Grants are held by Jadpo**, like sessions, in a `jadpo_grants`
  system table in the owner entity's store, so grants need a database.
  Their tokens are encrypted with `application { grants: { encryption_key:
  config.grant_encryption_key } }`, a `SigningKey` setting. It rotates as
  a `signing_key:` does: the current value encrypts, and its `_PREVIOUS`
  value is still read ([Signing keys](10-configuration-and-time.md#signing-keys)).
  Code can never read the tokens, and Jadpo refreshes them before they
  expire. `encryption_key:` is one of the places in `application { }` a
  secret may be read ([Secrets](10-configuration-and-time.md#secrets)).
- **Every call names whose grant it uses**, `on_behalf_of: owner_id`, an id
  of the owner's entity that the action has loaded or been given.
  `grant_missing` and `grant_revoked` are failures your app maps to its
  own ([Integrations](11-integrations.md)).
  The caller of the route need not be the owner: a scheduled job acting for
  an organisation names the organisation.
- **Disconnecting** is the generated `POST /connections/<name>/disconnect`
  and its operation, revoking at the provider when the package declares
  how. Erasing a person or a record ([Personal data](15-personal-data.md))
  deletes and revokes its grants. A revoked grant gives the `GrantRevoked`
  event, so your app can ask the owner to reconnect.
- **In development and tests**, `jadpo dev grants add <connection> --owner
  <id>` makes a fake grant, and tests declare grants as fixtures.

Not built yet: OAuth grants, their routes and `on_behalf_of:`. Acting as
an OAuth provider isn't built either.

## Failures
<!-- topic: identity-failures -->

Jadpo raises the sign-in failures itself. You never declare them or list
them in an action's `fails`
([Declaring a failure](04-failures.md#declaring-a-failure)). A client gets
them in the usual failure body, with the code and status below.

| Condition | Code | Kind | HTTP |
|---|---|---|---|
| No credential | `authentication_required` | `unauthenticated` | 401 |
| Malformed, expired, tampered or wrong-audience credential | `invalid_credentials` | `unauthenticated` | 401 |
| Two credentials, or a cookie and a bearer together | `ambiguous_credentials` | `unauthenticated` | 401 |
| Cookie write without the right `Origin` or CSRF header | `csrf_rejected` | `not_permitted` | 403 |
| The caller's record is not active | `user_disabled` / `service_disabled` | `not_permitted` | 403 |
| A service calls a route that selects `current_principal.user` | `not_permitted` | `not_permitted` | 403 |
| The database or OIDC provider cannot be reached | `authentication_unavailable` | `unavailable` | 503 |
| Keys or issuer missing or invalid | `authentication_misconfigured` | `misconfigured` | 500 |
| A magic link expired, used, replaced or unknown | `sign_in_link_expired` | `unauthenticated` | 401 |
| A magic link opened in another browser with `same_browser: true` | `sign_in_link_other_browser` | `unauthenticated` | 401 |
| A code wrong, used, ended, expired or replaced, or no code for the address | `sign_in_code_invalid` | `unauthenticated` | 401 |
| An address reached `failed_codes_per_day:` | `sign_in_codes_disabled` | `rate_limited` | 429 |
| `on_first_sign_in` refused, or no user and no `on_first_sign_in:` for a token | `sign_in_refused` | `not_permitted` | 403 |
| A provider's sign-in expired, was used, came back to another browser, was refused by the provider, or sent an ID token that doesn't check | `sign_in_provider_failed` | `unauthenticated` | 401 |
| Too many sends to one address | `rate_limited` | `rate_limited` | 429 |
| `magic_links.request` given a path to go back to that isn't on the app's site | `sign_in_return_invalid` | `invalid_value` | 422 |
| A passkey ceremony's challenge expired, was used, was replaced or came from another browser or session | `passkey_challenge_expired` | `unauthenticated` | 401 |
| A passkey's answer doesn't check (origin, relying party, presence, verification, signature or counter), or its passkey isn't registered | `passkey_rejected` | `unauthenticated` | 401 |
| The passkey is registered already | `passkey_already_registered` | `conflict` | 409 |
| The person has 20 passkeys already | `passkey_limit_reached` | `conflict` | 409 |
| No passkey of the signed-in person has that `id` | `passkey_not_found` | `not_found` | 404 |
| A route's `confirm:` isn't met: no answer, one that doesn't check or isn't from one of the person's own passkeys, one for another request, used already or too old, or a person with no passkey | `confirmation_required` | `not_permitted` | 403 |

`user_disabled` and `service_disabled` are built in and never declared,
as are the `sign_in_` and `passkey_` failures and
`confirmation_required`. Their wording for people is
Jadpo's own ([Built-in services](16-built-in-services.md)).

With a [revocation delay](#fewer-database-reads-revocation_delay), a
disabled caller is noticed within that delay. The log records the way in,
the credential's record id and the reason, never the credential.

## What is built
<!-- topic: identity-built -->

A feature not built yet is `toolchain.not_built_yet` where it is written.

| Feature | Status |
|---|---|
| Everything else on this page | Built. |
| `Driver.github`, `Driver.apple` and `oauth_provider:` | Not built yet. |
| Scope declarations and policy's scope restrictions | Not built yet. As [Policy](09-policy.md) specifies, an external method narrows scopes only when it declares the scopes it reads; undeclared provider scope or profile claims never replace application authority. |
| OAuth grants: `OAuthGrant` credentials, the connecting and disconnecting routes and `on_behalf_of:` ([Integrations](11-integrations.md)) | Not built yet. |

## Errors

Every error this area's checks produce, with an example that triggers it.
The examples are one file each, so the application, principal and
sign-in methods sit beside the entity they name; a project keeps them in
`app.jadpo` and `authentication.jadpo`.

- [`route.confirm_invalid`](../errors/route.confirm_invalid.md)
- [`semantic.multiple_applications`](../errors/semantic.multiple_applications.md)
- [`semantic.multiple_principals`](../errors/semantic.multiple_principals.md)
- [`semantic.multiple_authentication`](../errors/semantic.multiple_authentication.md)
- [`semantic.principal_variant_required`](../errors/semantic.principal_variant_required.md)
- [`semantic.principal_variant_unknown`](../errors/semantic.principal_variant_unknown.md)
- [`semantic.principal_variant_duplicate`](../errors/semantic.principal_variant_duplicate.md)
- [`semantic.principal_variant_invalid`](../errors/semantic.principal_variant_invalid.md)
- [`type.principal_subject_invalid`](../errors/type.principal_subject_invalid.md)
- [`type.principal_subject_not_unique`](../errors/type.principal_subject_not_unique.md)
- [`type.principal_active_invalid`](../errors/type.principal_active_invalid.md)
- [`semantic.revocation_delay_invalid`](../errors/semantic.revocation_delay_invalid.md)
- [`semantic.auth_method_unknown`](../errors/semantic.auth_method_unknown.md)
- [`semantic.auth_method_duplicate`](../errors/semantic.auth_method_duplicate.md)
- [`semantic.auth_setting_unknown`](../errors/semantic.auth_setting_unknown.md)
- [`semantic.auth_setting_invalid`](../errors/semantic.auth_setting_invalid.md)
- [`semantic.auth_setting_required`](../errors/semantic.auth_setting_required.md)
- [`semantic.auth_principal_required`](../errors/semantic.auth_principal_required.md)
- [`semantic.auth_not_set_up`](../errors/semantic.auth_not_set_up.md)
- [`type.auth_principal_output`](../errors/type.auth_principal_output.md)
- [`type.auth_principal_input`](../errors/type.auth_principal_input.md)
- [`type.auth_principal_failure_context`](../errors/type.auth_principal_failure_context.md)
- [`type.auth_principal_variant_unknown`](../errors/type.auth_principal_variant_unknown.md)
- [`type.auth_public_principal`](../errors/type.auth_public_principal.md)
- [`type.auth_sessions_context`](../errors/type.auth_sessions_context.md)
- [`type.auth_principal_construction`](../errors/type.auth_principal_construction.md)
- [`type.auth_credential_stored`](../errors/type.auth_credential_stored.md)
