---
title: Identity
area: 8
status: decided
checked-with: jadpo chunk-5.1b (2026-10-16)
rules:
  - semantic.auth_*
  - semantic.multiple_*
  - semantic.principal_variant_*
  - semantic.revocation_delay_*
  - type.auth_*
  - type.principal_*
---

# Identity

Who is calling: the `application { }` and `principal { }` declarations, the
ways a caller proves who they are, how code reads the caller, service
credentials, revocation, and the failures authentication produces. Who may do
what is policy, area 9.

> Decided on 2026-10-12. Code on this page applies the decisions from areas 1 to 8. Built in the new compiler in roadmap chunk 4.3: the principal, browser sessions, routes signed in by default, `current_principal`, starting and ending a session, and signing in from a route test; in chunk 4.3b: API tokens, service keys and service callers; in chunk 4.4c: a `GET` route listing tokens and keys; in chunk 5.1b: the revocation delay, with signed sessions and tokens, `POST /auth/exchange` and a browser session's `origin:` read from configuration. "Status" below lists the rest.

The rules underneath all of it were accepted in the AUTH-001 contract and are
not reopened here:

- every route needs a signed-in caller unless it says otherwise;
- a request carries exactly one credential; two, or an invalid one beside a
  valid one, is refused rather than picked from;
- authentication finishes before the request body is read or any business
  work starts;
- code receives one typed caller, never cookies, tokens, keys or provider
  claims;
- permissions are never part of a credential; policy decides them.

| Piece | Where | What it says |
|---|---|---|
| `application { }` | `app.jadpo` | How long a revoked session may keep working |
| `principal { }` | `app.jadpo` | Which entity is a user, which is a service, and when each is active |
| `authentication { }` | `authentication.jadpo` | The ways a caller can sign in, and with `sign_in:` how a person is checked |
| `current_principal` | routes | The signed-in caller, passed to actions |

## The principal

The principal is the caller after authentication: either a **user** (a
person) or a **service** (another system). It is declared once, with no name
(decided in area 1), and says which entity field identifies each kind and
when one counts as active.

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

- `subject:` names a unique field of a stored entity. Authentication looks
  the caller up by it on every check that needs current data.
- `active:` is a condition on that entity, naming its fields as a query
  does. A caller whose record is not active is refused (see Failures
  below); left out, every record is active.
- The principal's fields are built in (question 2): `id` (a `User.id` or
  `Service.id`), `subject` and `strength` (`primary` or `multi_factor`). Code
  that needs a role, tenant or email reads it from the entity, which is where
  policy (area 9) reads it too.
- `service:` is optional; most applications only have users (question 3).

## The application and revocation

A signed-in caller can be disabled, or a session revoked, while they still hold
a valid credential. The application says how long that may take to bite.

```jadpo check project=delay-alone
application {
    revocation_delay: 5m
}
```

- **No `revocation_delay`** (the default): every request checks the caller's
  record. Revoking or disabling takes effect on the next request. Sessions and
  tokens are opaque values looked up in the database.
- **`revocation_delay: 5m`**: sessions and tokens are signed and checked
  without a database read, and expire within the delay; renewing one checks
  the record. A disabled user may keep working for up to five minutes.

This is the only revocation setting (question 4), and leaving it out gives
the strict behaviour. `application { }` also holds `display_name:`, the
application's name as people see it, required once magic links or another
built-in email is switched on (area 16, "Built-in emails"). Three paths always
check the record whatever the setting: external OIDC tokens, service keys used
directly, and routes marked `authentication: RouteAuthentication.fresh` (below).

With a delay, a credential is still issued as it is without one: a session's
cookie and an API token are 32 random bytes the runtime keeps only as a
verifier. Its first request reads the record, and the response renews it
with a part the runtime signs, holding what that read found and when it
stops counting: the delay after the read, or the session's end if that is
sooner. Every request after it signs in from the signed part alone, until it
expires and the next request renews it the same way (chosen in chunk 5.1b):

- a browser session's renewed cookie comes back in `Set-Cookie`, which the
  browser keeps by itself;
- an API token's comes back in an `X-Jadpo-Token` header, which a client
  keeps and sends from then on; one that ignores it still signs in, reading
  the record each time;
- a token `POST /auth/exchange` gives a service is signed when it is given
  and is never renewed: when it expires, the service exchanges its key again.

Each method signs with its `signing_key:`, a `SigningKey` setting (area 10):
new parts are signed with the current value and parts signed with the
`_PREVIOUS` one still count, so a key rotates without signing anyone out. A
signed part the key doesn't open, or one that has expired, is never a
refusal: the request reads the record, as it would with no delay.

A whole application with a delay: the settings, the delay, and each method's
key.

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
expires; a `fresh` route reads the record either way:

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

The API record lists the exchange the runtime answers, as `jadpo routes`
and OpenAPI do (area 7, question 12):

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

`authentication.jadpo` lists the methods the application accepts. Each is one
named block from a fixed list; the method decides where the credential travels
(question 1, with the [research and options](research/08-identity-proposals.md)).

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

| Method | Credential | Who | Notes |
|---|---|---|---|
| `browser_session` | `HttpOnly`, `Secure`, `SameSite=Strict` cookie | users | Writes need the exact `Origin` and a session-bound `X-Jadpo-CSRF` header |
| `api_token` | `Authorization: Bearer` | users | For the application's own mobile or command-line clients |
| `external_token` | `Authorization: Bearer` JWT from a provider | users | Only the subject is used; the user is looked up on every request (renamed from `oidc`, question 11) |
| `service_key` | `Authorization: Bearer` key | services | Shown once, stored as a verifier; can be exchanged for a short token |

- Keys come only from secret configuration (area 10): `signing_key:` reads
  a `SigningKey` setting, and a key in source is an error. Each method needs
  one once `revocation_delay:` is set; without a delay nothing is signed, so
  a key may stay written and the delay is switched off in one line.
- `origin:` reads a `Url` setting: the scheme, host and port the
  application's pages are served from. A cookie write's `Origin` must be
  exactly it, so a proxy in front of the server changes nothing. Left out, a
  write's `Origin` must name the server its `Host` header gives.
- `browser_session:` names its cookie (`cookie:`, 1 to 64 letters, digits,
  `_` and `-`). The session's token travels in it, `HttpOnly`, `Secure`,
  `SameSite=Strict`, for the whole site; its CSRF token in a second cookie
  of the same name with `_csrf` after it, which the application's pages
  read and send back in `X-Jadpo-CSRF` on every write. A session lasts 30
  days from when it starts.
- `api_token:` and `service_key:` take only `signing_key:`, needed with a
  revocation delay, so without one they are written `api_token: {}` and
  `service_key: {}`. An API token signs in a user, so it needs the
  principal's `user:`; a service key signs in a service and needs its
  `service:`. Each travels as `Authorization: Bearer <value>`: an API
  token is `jadpo_token_` and 64 hexadecimal digits, a service key
  `jadpo_key_` and 64, so the runtime knows which it was sent and secret
  scanners can find one that leaks. Both are 32 random bytes kept only as
  their BLAKE3 digests, like a session's.
- A bearer token is never accepted from the URL, and the cookie name and
  `Authorization` header can never be ordinary route input.
- `external_token` uses the compiler's pinned JWT cryptography (area 19).
  Its algorithms, limits and key refresh are fixed by the compiler;
  applications without the method make no discovery or key requests.
- **Provider reachability (Richard, 2026-10-22):** the configured issuer
  and its discovered key endpoint use HTTPS, with no redirects and only
  public resolved addresses. The discovery document must name exactly
  the configured issuer, including its trailing slash. Its `jwks_uri`
  may name another public HTTPS authority; the token never supplies an
  address to fetch. Discovery appends `/.well-known/openid-configuration`
  after the issuer's path, removing a final slash for that request only
  ([OpenID Connect Discovery §§3, 4.1, 4.3](https://openid.net/specs/openid-connect-discovery-1_0.html)).
- With a revocation delay and service keys, the compiler generates
  `POST /auth/exchange`, which trades a key for a short-lived token (question
  1's proposal fixes the path). A service sends its key as `Authorization:
  Bearer` and nothing else, and gets `{ "token": ..., "expires_at": ... }`:
  the token is `jadpo_service_token_` and a signed part, lasting the delay
  or until the key expires if that is sooner, and signs the service in
  wherever its key would. The key is read with the service's record, so a
  disabled service is refused there. A route the application writes at
  that path is `route.path_overlap`.

## Checking the person: `sign_in:`

> Decided on 2026-10-16 (Richard), revising question 11. `sign_in:` is
> read from chunk 5.5z, with `passkey:` built there; `magic_link:` and
> `on_first_sign_in:` are built in chunk 7.3b, and the app's own email and
> sending (`email:`, `send:`) in chunk 7.3b2; writing the routes yourself
> comes in chunk 7.3b3 and the events in chunk 7.3b4 (a passkey's in
> chunk 5.5z2); `providers:` is
> `toolchain.not_built_yet` until after the first build.

Signing in has two layers. The first checks the person; the second gives
them a session (`sessions.start` for a browser, `sessions.issue_token` for
the app's own clients). `sign_in:` in `authentication { }` lists every way
the language checks a person: built-in magic links, and providers such as
Google (question 11). Each one ends by starting a session the same way, and
every way in, external tokens included, shares one decision point for a
person the app has not seen before, `on_first_sign_in:`.

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
        providers: [google, github]             // connections in app.jadpo (question 11)
    }
    on_first_sign_in: accounts.create_member
}
```

`sign_in:` replaces the earlier `sign_in_providers:`, which is refused with
its rename. Checking the person stays out of application code: an app no
longer writes a magic-link action, and `sessions.start` remains for a
person the app checks some other way (an invitation it issued, below).

> Revised 2026-10-16 (Richard, area 11's "No presets"): sign-in providers
> are connections. Google, Microsoft and other OpenID Connect providers are
> each `google: { driver: Driver.oidc  issuer:  client_id:  credential: }`
> in `app.jadpo`'s `connections:`, whose discovery document gives the
> rest; GitHub and Apple, which are not plain OpenID Connect, are the built-in
> `Driver.github` and `Driver.apple`. `sign_in:` names them in `providers: [...]`, as
> `email:` names its connection.

### Magic links

A person gives their email address and receives one email holding a link
and a six-digit code. Either signs them in, once: the link through a page
of the app's frontend, the code typed into the app's own sign-in page or
posted by the app's own client for an API token. Jadpo generates endpoints
only, never pages (decided 2026-10-16): the frontend owns every page.

| Setting | Default | Loosening past this needs an acceptance |
|---|---|---|
| `address:` | the `email` field of the principal's user entity | |
| `expires_after:` | `15m` | more than `1h` |
| `code_attempts:` | `5` wrong guesses end a code | more than `10` |
| `codes_per_hour:` | `10` sends per address | |
| `failed_codes_per_day:` | `20` wrong guesses per address | more than `50` |
| `resend_after:` | `1m` between sends to one address | |
| `same_browser:` | `false` | |
| `link:` | required: the frontend path the email's link opens, such as `"/sign-in/confirm"` | |
| `path:` | `"/auth/magic-link"`, where the generated routes go | |
| `generate_routes:` | `true`; `false` generates none | |
| `email:` | Jadpo's built-in email | (area 16, "Built-in emails", level 4) |
| `send:` | none | (area 16, "Built-in emails", level 5) |

- `address:` names a field of the principal's user entity whose type is
  built on `Email` and which is `unique: true`. Left out, the entity's
  `email` field is used; with no such field, `address:` is required.
- Tightening a setting is free. Loosening one past the limit in the table
  shows in the approval report and needs an acceptance (area 7's form),
  like any change that weakens security.
- `magic_link:` needs `browser_session:` with its `origin:`, which is
  otherwise optional (chunk 5.1b), since the link starts a browser session
  at that origin; and the app's `email:` connection unless it names
  `send:` (area 16).

**The routes.** The compiler generates three routes anyone may call, under
`path:`:

```text
POST /auth/magic-link            ask for a link and a code
POST /auth/magic-link/confirm    a link's token or a code: starts a browser session
POST /auth/magic-link/redeem     a code for an API token (needs api_token:)
```

```text
POST /auth/magic-link
{ "address": "ada@example.com", "return_to": "/notes", "locale": "fr_fr" }

202 Accepted                      (no body; the same for every address)
```

- `address` is required; `return_to` and `locale` are optional.
  `return_to` must be a path on the app's own site: it starts with one
  `/`, and anything else (`//evil.example`, `https://...`) is a 422
  `invalid_input`. Without it the person goes to `/`.
- The email's language is `locale` when it is a supported locale, else the
  best supported match for the request's `Accept-Language`, else the
  default locale.
- The answer is `202` with no body whether or not an account has the
  address. With an account, or with `on_first_sign_in:` declared, the
  email is sent after the request commits; without either, nothing is
  sent.
- Sends are limited per address, counted the same whether or not an
  account has it: one per `resend_after:` and `codes_per_hour:` an hour.
  Over either limit the answer is the built-in `rate_limited` (429, with
  `Retry-After`), identical for unknown addresses.
- Each send makes the earlier link and code for that address invalid: only
  the latest is ever valid.

The link is `<origin><link>?token=<64 hexadecimal digits>`: the
`origin:` of `browser_session:` and the frontend path `link:` names, for
example `https://notes.example/sign-in/confirm?token=3f9a...c1`. The
frontend page reads the token and posts it. Loading the page consumes
nothing, so a mail scanner that opens the link signs no one in; only the
`POST` does:

```text
POST /auth/magic-link/confirm
{ "token": "3f9a...c1" }

200 OK
{ "return_to": "/notes" }          (the request's return_to)
Set-Cookie: notes_session=...; notes_session_csrf=...
```

The app's own sign-in page posts the code instead and gets the same
answer:

```text
POST /auth/magic-link/confirm
{ "address": "ada@example.com", "code": "042317" }
```

The app's own client trades the code for an API token:

```text
POST /auth/magic-link/redeem
{ "address": "ada@example.com", "code": "042317" }

200 OK
{ "token": "jadpo_token_9c1e..." }
```

- A confirm `POST` needs an `Origin` naming the app's own site, as a
  cookie write does. A failure is the usual envelope; the frontend shows
  its wording, which the per-locale failure export carries (area 10).
- Each route's response shape is fixed (area 7, "Generated routes").
- Link tokens are 32 random bytes; codes six random digits, kept as text
  so a leading `0` stays. Both are kept only as verifiers, with a digest of
  the address rather than the address, in the `jadpo_sign_in_requests`
  system table in the store of the principal's user entity, beside
  `jadpo_sessions`. Each is single-use, and using either ends the other.
- **Wrong codes.** `code_attempts:` wrong guesses end that code. Every
  wrong guess gets the same answer, `sign_in_code_invalid`, never a count,
  and so does a used, ended, expired or replaced code, or an address with
  no code. When an address reaches `failed_codes_per_day:` wrong guesses in
  a day (UTC), codes stop working for it until the next day
  (`sign_in_codes_disabled`), while links still work. Nothing locks an
  account, so an attacker cannot lock a person out.
- **`same_browser: true`** makes a link work only in the browser that asked
  for it: `POST /auth/magic-link` sets a `<cookie>_sign_in` cookie
  (`HttpOnly`, `Secure`, `SameSite=Lax`, lasting `expires_after:`), and the
  confirm refuses a link from any other browser
  (`sign_in_link_other_browser`). The code still works anywhere.
- After signing in the frontend is given only the `return_to` path stored
  when the person asked, never an address the link carries.
- A person signed in by link or code has proved they control the address:
  it reaches `on_first_sign_in` as verified.

**Writing the routes yourself.** `path:` moves the generated routes;
`generate_routes: false` generates none, and the app writes its own routes over the
generated operations, which keep every protection above (limits, only the
latest valid, wrong-code rules, `same_browser:`, `on_first_sign_in`):

| Operation | Does |
|---|---|
| `magic_links.request(address: Email, locale: Locale, return_to: Text?)` | Sends the link and code after commit, as `POST /auth/magic-link` does; fails with `rate_limited`, and with `sign_in_return_invalid` (`invalid_value`, 422) for a `return_to` that is not a path on the app's site |
| `magic_links.redeem(token: SignInToken)` | Uses a link's token; gives a `MagicLinkRedeemed` (`user_id`, `return_to: Text`) or fails with a `sign_in_` failure |
| `magic_links.redeem_code(address: Email, code: SignInCode)` | The same for a code |

The app's action then calls `sessions.start(redeemed.user_id)` or
`sessions.issue_token(redeemed.user_id)`. Only actions call them, as with
`sessions.start`; `same_browser:`'s cookie is set on the response of the
route whose action calls `magic_links.request`.

```jadpo fragment
// domains/accounts/actions/confirm_sign_in.jadpo
action confirm_sign_in(input: ConfirmSignIn) -> SignedInView {
    var redeemed: MagicLinkRedeemed = magic_links.redeem(input.token)
    sessions.start(redeemed.user_id)
    ...
}
```

The `sign_in_` failures are built in and raised by the runtime, so, like
every authentication failure, they pass through an action without being
listed in its `fails` (area 4).

A whole application with magic links (built in chunk 7.3b): the settings,
the email connection and the application's name its email needs, the
member, and the action that signs a new member up the first time they
sign in.

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

The action runs as the system (`Access.system`), reaching no tenant's
records (chunk 7.9b), in its own transaction, for an address no member
has:

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

The API record lists the routes the runtime answers, each body and answer
fixed; the redemption comes with `api_token:`:

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

### Passkeys

> Decided 2026-10-18 (Richard). Registering, signing in and managing
> passkeys built in chunk 5.5z; recovery through a magic link, the check
> that magic links are on, the route-test helpers and the events built in
> chunk 5.5z2; the ceremonies on Workers built in chunk 8.2l.

A passkey (WebAuthn, FIDO2) is the third built-in way to check a person,
beside magic links and providers. The person unlocks a key held on their
device with a fingerprint, their face or the device's PIN; the biometric
never leaves the device, and the app stores only a public key, so there is
no secret on the server to leak and a passkey works only on the site it
was made for. The language owns it, as it owns magic links, because the
ceremonies are where hand-written sign-in goes wrong:

- **Registering and signing in** are built in: the one-time challenge, its
  expiry, the origin and relying-party check, and the signature counter,
  on the well-established `webauthn-rs` crate rather than written by hand.
  Attestation is not requested.
- **Stored with the user** as the standard `WebAuthnCredential` type (area
  19), never a biometric: area 15 has nothing to report beyond the
  credential's identifier and when it was last used.
- **Recovery is a magic link.** A person who has lost every device with a
  passkey signs in by email and adds a new one, so passkeys need magic
  links on in the same application.
- **More than one passkey per person**, each named by the person and
  removable, so a phone and a laptop each have their own.

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

- `passkey:` needs `browser_session:` with its `origin:`. That origin is
  the one every answer from a device is checked against, and its host is
  the relying-party ID a passkey is bound to: read from configuration
  (area 10), never from the request, and never widened to a parent domain
  or another port. A passkey made for `https://notes.example` works only
  there.
- `passkey:` needs `magic_link:` in the same `sign_in:`: it is how a person
  with no passkey left gets back in.
- Every passkey verifies the person (`userVerification: "required"`), so it
  is two factors in one: the device, and the fingerprint, face or PIN that
  unlocks it. Every passkey is discoverable (`residentKey: "required"`),
  so signing in asks for no address first and tells no one which
  addresses have an account.
- The device lists a passkey under the person's address (the field
  `magic_link:`'s `address:` names) and the application's host.
- A person has at most 20 passkeys.

**The routes.** The compiler generates seven routes under `path:`. Signing
in is open to anyone; adding and managing passkeys need the person signed
in with a browser session, and every write needs the session's CSRF token
and an `Origin` naming the app's site, as any cookie write does:

```text
POST   /auth/passkey/sign-in/options     options for signing in
POST   /auth/passkey/sign-in             the device's answer: starts a browser session
POST   /auth/passkey/register/options    options for adding a passkey (signed in)
POST   /auth/passkey/register            the new passkey and its name: keeps it (signed in)
GET    /auth/passkey                     the person's passkeys (signed in)
PATCH  /auth/passkey/{passkey_id}        renames one (signed in)
DELETE /auth/passkey/{passkey_id}        removes one (signed in)
```

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
  the `jadpo_passkey_challenges` system table with what `webauthn-rs`
  needs to check the answer: server-side, never in the cookie. The cookie,
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
- `webauthn-rs` checks the answer: the challenge, the origin and the
  relying-party ID, that the person was present and verified, the
  signature against the stored public key, and the signature counter. Any
  failure is `passkey_rejected`, never saying which, and so is an answer
  from a passkey that was removed or never registered.
- **The counter.** When the stored counter or the answer's is above zero,
  the answer's must be higher than the stored one (WebAuthn Level 3,
  §7.2, the signature counter's step). One that isn't means the key may have been copied: the
  sign-in is refused (`passkey_rejected`) and the passkey left as it was,
  so the person signs in another way. A passkey that keeps no counter,
  which most synced ones don't, sends `0` every time and is never refused
  for it.
- A person whose `active:` doesn't hold is `user_disabled`, as on any
  request.
- On success the passkey's counter, backup state and last use are
  updated, a browser session starts as `sessions.start` starts one, and
  `<cookie>_passkey` is cleared. A passkey signs in a browser session
  only; the app's own clients sign in with a magic link's code.

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

Listing, renaming and removing:

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
- Each route's response shape is fixed (area 7, "Generated routes"). They
  belong to the first API that accepts browser sessions and are listed in
  its record, its OpenAPI document and `jadpo routes`, marked generated.
- The app writes no passkey routes of its own: one at any of these paths
  is `route.path_overlap`. Unlike magic links there is no `generate_routes: false`,
  since every body is WebAuthn's rather than the app's, so there is
  nothing for the app's own route to add.

**What is kept.** `jadpo_passkeys`, a system table in the store of the
principal's user entity beside `jadpo_sessions`, holds each passkey as a
`WebAuthnCredential` (area 19): its credential ID, its public key (a COSE
key), its signature counter, its transports, whether it may be and is
backed up, its name, and when it was registered and last used, with the
user's `id` and the user handle. None of it is a secret: a copy of the
table signs no one in. Area 15's report lists only each passkey's
credential ID and its last use.

**Recovery.** A person with no passkey left signs in with a magic link or
its code, then adds a new passkey through the routes above. Nothing else
recovers an account, so a lost phone never needs an operator.

A whole application with passkeys: the origin setting, the user,
`passkey:` beside the browser session, and magic links beside it, with
the email connection and the application's name their email needs.

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

A route test drives the generated routes with `test_passkeys` (see "In
development and tests"): Ada adds a passkey and signs in with it.

```jadpo check project=passkeys file=tests/passkeys.test.jadpo
use journal

test "a passkey signs Ada in" {
    var ada: Principal.user = journal.member_called("ada@example.com")
    var phone: TestPasskey = test_passkeys.register(ada, "Phone")
    var response: Response = test_passkeys.sign_in(phone)
    assert response.status == 204
}
```

The API record lists the routes the runtime answers, as `jadpo routes`
and OpenAPI do; WebAuthn's own JSON goes in and out of the ceremonies,
named here for what it is:

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
  help: docs/language/08-identity.html#semantic-auth-setting-required

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
  help: docs/language/08-identity.html#semantic-auth-setting-required

check failed: 1 error, 1 file
```

### A person the app has not seen

`on_first_sign_in:` names an action that decides, for every way in, what a
first sign-in does: create a user, link an existing one, or refuse. It
runs when a magic link or code is used for an address no user has, when a
provider returns an account not yet linked, and when an external token
names a subject not yet linked.

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

- The action takes exactly one `FirstSignIn` and returns the principal's
  user entity's `id`, of a user it created or one that already exists. It
  runs in its own transaction before the session starts.
- To refuse, it fails with any failure it declares. The person always
  sees the same built-in `sign_in_refused`, whatever the reason; the log
  and the `SignInRefused` event keep the real one.
- For a provider or external token the runtime then records the link
  (method and subject to the user's `id`) in the `jadpo_sign_in_links`
  system table, so the next sign-in skips the action. A magic link needs
  no link: the user is found by `address:`.
- Linking an existing user by address is the action's choice, and should
  only happen when `address_verified` is `true` (question 11).
- Without `on_first_sign_in:`, only existing users sign in: an unknown
  address gets no email, and an unknown provider account or token subject
  is `sign_in_refused`.
- A passkey never runs it: a passkey is only ever added by a person
  already signed in, so every passkey that signs in has its user.

### Signing in from code the app writes

`sessions.start(user_id)` and `sessions.issue_token(user_id)` stay for a
person the app checks itself, such as one accepting an invitation it sent
(see "Starting and ending a session").

### Events

Every sign-in publishes a built-in event, for monitoring and the app's own
subscribers (area 12):

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
`passkey` when `passkey:` is declared. No event carries an address, a
link, a code, a challenge or a passkey's credential ID or key.

### In development and tests

In `jadpo dev` the email lands in the local inbox (area 16), and its link
and code work there: the link opens the frontend's page, and an agent
reads the token or code with `jadpo email links <id>` or `show <id>` and
posts it to `POST /auth/magic-link/confirm` itself.
A route test asks for a link with `POST /auth/magic-link`, reads the email
from `email.sent` and follows its link or posts its code; `as:` still
signs a test in directly. A test calls `on_first_sign_in:`'s action
directly with a `FirstSignIn` it builds, every field given, and a
subscriber's action with a sign-in event the same way:

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

Browsers allow passkeys on `http://localhost` without TLS, so in `jadpo dev`
a browser's own passkeys work against a `browser_origin` of
`http://localhost:<port>`; an agent uses the browser's virtual
authenticator (Chrome DevTools' WebAuthn panel, or WebDriver's virtual
authenticator commands) rather than a real device. A route test has
`test_passkeys`, which only tests call, a software authenticator driving
the generated routes for real:

| Operation | Does |
|---|---|
| `test_passkeys.register(user: Principal.user, name: Text) -> TestPasskey` | Adds a passkey for `user` through `POST /auth/passkey/register/options` and `POST /auth/passkey/register`, signed in as `as:` signs in, and gives the device holding it |
| `test_passkeys.sign_in(passkey: TestPasskey) -> Response` | Signs in with it through the two sign-in routes and gives the second's `Response` |

Each `TestPasskey` is a device of its own holding one passkey, made by the
test's host, so only a test ever holds one: called anywhere else,
`test_passkeys` is `semantic.request_invalid`. A route that refuses
`register`, or the options of `sign_in`, ends the test as a fault naming
the route and its code, since the test meant to have a passkey; the
sign-in's own refusal comes back in its `Response`.

```jadpo fragment
test "a passkey signs Ada in" {
    var ada: Principal.user = member_called("ada")
    var phone: TestPasskey = test_passkeys.register(ada, "Phone")
    var response: Response = test_passkeys.sign_in(phone)
    assert response.status == 204
}
```

### What the compiler checks

For magic links (chunk 7.3b, except where marked):

- `sign_in_providers:` is `semantic.auth_method_unknown`, whose edit
  renames it `sign_in:`.
- `magic_link:` without `browser_session:`, or without its `origin:`, is
  `semantic.auth_setting_required`; without the app's `email:` connection
  it is `email.connection_required`, unless it names `send:` (chunk
  7.3b2); without `application { display_name: }`, which Jadpo's email names
  the application by, `semantic.auth_setting_required`; without `link:`,
  `semantic.auth_setting_required`.
- `address:` naming anything but a field of the principal's user entity,
  built on `Email` and `unique: true`, is `semantic.auth_setting_invalid`
  (a field it doesn't have, `type.unknown_field`); left out with no
  `email` field, `semantic.auth_setting_required`.
- A setting `magic_link:` doesn't take is `semantic.auth_setting_unknown`;
  `link:` that isn't a path on the app's site, `path:` that isn't fixed
  segments, `expires_after:` under `1m`, `resend_after:` not above zero,
  a count under 1 and `same_browser:` that isn't `true` or `false` are
  `semantic.auth_setting_invalid`.
- A setting loosened past its limit is `toolchain.not_built_yet` until
  acceptances (area 18), which will put it in the approval report.
- `on_first_sign_in:` naming anything but an action taking one
  `FirstSignIn` and returning the user entity's `id` is
  `semantic.auth_setting_invalid`.
- `email:` and `send:` together are `semantic.auth_setting_invalid`
  (area 16). In chunk 7.3b2: `email:` naming anything but an
  `EmailMessage` of the app's is `semantic.auth_setting_invalid`, and so
  is one with a field besides `to:` and `locale:` that isn't picked from
  `MagicLinkEmail`; `send:` naming anything but an action taking one
  `MagicLinkRequest` and returning `Unit`, the same; `emit` of the email
  `email:` names is `event.emit_invalid`; copying from `MagicLinkEmail`
  into anything but an email is `type.not_record`; a `SignInCode` kept in
  an entity is `type.auth_credential_stored`, and one in an `Output`, a
  failure or an event `type.secret_exposed`.
- A route the application writes at a generated magic-link path is
  `route.path_overlap`; with `generate_routes: false` there are none.
- In chunk 7.3b3: `generate_routes:` that isn't `true` or `false`, or
  `false` beside `path:`, is `semantic.auth_setting_invalid`; calling
  `magic_links.request`, `.redeem` or `.redeem_code` with no
  `magic_link:`, or with its routes generated, is
  `semantic.auth_setting_required`; outside an action
  `type.auth_sessions_context`; as a route's `run:` itself
  `route.run_invalid`; a `SignInToken` kept in an entity is
  `type.auth_credential_stored`, and one in an `Output`, a failure, an
  event, an email, a message or a prompt `type.secret_exposed`.
- In chunk 7.3b4: a subscriber of `SignedIn`, `SignInRefused` or
  `SignInCodesDisabled` with no `magic_link:` is
  `subscriber.event_invalid`; a field of one it doesn't have
  `type.unknown_field`; `emit` of one `event.emit_invalid`; and
  `FirstSignIn` or a sign-in event built with `{ ... }` anywhere but a
  test, fixture or seed `type.not_record`.

For passkeys (chunk 5.5z, except where marked):

- `passkey:` without `browser_session:`, or without its `origin:`, is
  `semantic.auth_setting_required`.
- `passkey:` without `magic_link:` in the same `sign_in:` is
  `semantic.auth_setting_required` (chunk 5.5z2).
- A setting `passkey:` doesn't take is `semantic.auth_setting_unknown`;
  `expires_after:` outside `1m` to `10m`, or a `path:` that isn't fixed
  segments, is `semantic.auth_setting_invalid`.
- Anything in `sign_in:` but `magic_link:`, `passkey:` and `providers:` is
  `semantic.auth_method_unknown`.
- A route the application writes at a generated passkey path is
  `route.path_overlap`.
- In chunk 5.5z2: a subscriber of `PasskeyAdded` or `PasskeyRemoved`
  with no `passkey:` is `subscriber.event_invalid`, and `emit` of one
  `event.emit_invalid`; `test_passkeys.register` or `.sign_in` anywhere
  but a test is `semantic.request_invalid`, and with no `passkey:`
  `semantic.auth_setting_required`.

## Who is calling: `current_principal`

A route reads the signed-in caller as `current_principal` and passes it to
the action it runs, like any other argument. Selecting a variant narrows it:
`current_principal.user` is a `Principal.user`, and a service calling that
route is refused with 403 before the action runs.

```jadpo fragment
// domains/planning/actions/create_todo.jadpo
action create_todo(input: CreateTodo, caller: Principal.user) fails DueDateInPast -> TodoView {
    var owner: User.id = caller.id
    ...
}

// domains/planning/routes/todos.jadpo (route syntax is area 7)
POST / {
    input: CreateTodo
    output: TodoView
    run: create_todo(input, current_principal.user)
}
```

- `current_principal` exists only in routes that require authentication
  (question 5). Actions, queries and functions receive the caller as a
  parameter, so their signatures say they need one and tests pass one in.
  An action that only needs policy's checks takes no caller: policy (area 9)
  reads it from the request itself.
- A schedule or subscriber has no caller; an action it runs cannot ask for one
  (area 12).
- A caller value can never be returned to a client, read from request input or
  put in a failure's fields (`type.auth_principal_*`), however deep a field
  holds it: in a list, a map or an option, or in an object's field or a
  variant's data.

## Routes: signed in by default

Every route requires a signed-in caller. A route says only when that changes:

```jadpo fragment
GET /health/live {
    authentication: RouteAuthentication.none     // anyone may call it; no credential is read
    output: Health
    run: live()
}

DELETE /users/{user_id} {
    authentication: RouteAuthentication.fresh    // check the caller's record now, even with a revocation delay
    run: disable_user(user_id, current_principal.user)
}
```

- `authentication: RouteAuthentication.none` skips credential checks entirely. It does not skip
  input validation, policy or limits, and nothing it reaches may use the
  caller (`type.auth_public_principal`). Area 9 still decides whether the
  action may be invoked publicly.
- `authentication: RouteAuthentication.fresh` is for disabling accounts, changing permissions,
  billing and exports.
- Any other value is an error that offers both fixes without guessing which
  was meant, because one keeps the route protected and the other opens it.

Area 7 proposes several APIs (`main`, `admin`, `webhooks`), each with a
default authentication. An API lists the methods it accepts, and a route
inside it can only narrow that:

```jadpo fragment
// app.jadpo (the apis block is area 7's)
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

With one API, every method is accepted. Provider webhooks have no route to
authenticate: the runtime generates them from the provider package, checks
the signature and publishes each as an event (area 11).

## Service credentials

A service (a partner system, a worker) signs in with a key. Each service has
an owner, a status, and any number of keys that expire, can be revoked one at
a time, and overlap so a key can be rotated without downtime. A key is shown
once when issued and stored only as a verifier.

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

The language stores the keys, as it stores browser sessions, with generated
operations to issue, list and revoke them (`Service.issue_key(service_id,
expires_at)`, `Service.keys(service_id)`, `Service.revoke_key(key_id)`); the
application declares only `Service` (question 7).

- The operations belong to the entity the principal's `service:` names,
  once `authentication { }` declares `service_key:`, and only an action
  calls them (`type.auth_sessions_context`), as it calls `sessions.start`;
  a `GET` route also runs `keys` itself (below).
- `Service.issue_key(service_id, expires_at)` takes the service's `id`
  and an `Instant`, keeps the new key in the action's transaction and
  gives it once as a `ServiceKey`, text an `Output` can send; the
  response that carries it is never kept for an idempotent repeat, and a
  `ServiceKey` can't be stored in an entity.
- `Service.keys(service_id)` lists the service's keys that haven't
  expired, oldest first, as `IssuedKey`s, which code reads `id`,
  `started_at` and `ends_at` from, never the key itself. A `GET` route
  runs it too, and an `Output` can send the list (below).
- `Service.revoke_key(key_id)` deletes one by its `id`; an `id` no key
  has revokes nothing.
- Keys live in the `jadpo_service_keys` system table, in the store the
  service entity lives in (area 5), and a key is looked up with its
  service's record, `active:` and all, in one statement, so a disabled
  service is refused (`service_disabled`) on its next request. A service
  key signs in only a service: a route that selects
  `current_principal.user` refuses it with 403 `not_permitted`, and one
  that selects `current_principal.service` refuses a user the same way.

## Starting and ending a session

An action run by a route starts or ends a session, and the compiler sets and
clears the cookie and CSRF token (question 9). Passwords are out of scope.
Rails (`start_new_session_for`, `terminate_session`) and Phoenix
(`log_in_user`) generate the same two calls:

```jadpo fragment
action accept_invitation(token: InvitationToken) fails InvitationExpired -> Welcome {
    var invitation: Invitation = attempt Invitation.redeem(token)
    sessions.start(invitation.member_id)    // sets the cookie on this route's response
    ...
}

action sign_out() -> Unit {
    sessions.end()                   // ends the session this request came with
}
```

Magic links and providers check the person and start the session
themselves (`sign_in:`, above). `sessions.start` is for a person the app
checks some other way, such as an invitation it issued; passwords stay out
of scope.

- `sessions.start` keeps the session in the action's own transaction, in
  the store the principal's user entity lives in (a `jadpo_sessions` system
  table, area 5), so an action that fails starts none; the route's response
  sets its cookies once the action commits. `sessions.end` deletes the
  session the request came with, and the response clears the cookies. A
  session or token past its end, which a lookup ignores, and a service
  key past its expiry are deleted on the runtime's tick, in the store
  each is kept in (area 12, "The tick").
- Only an action calls them (`type.auth_sessions_context`); a test reaches
  them through the route, or calls the action, which keeps the session
  without a response to set it on.
- Only verifiers are kept: a session's token and its CSRF token are 32
  random bytes each, stored as their digests, so a copy of the database
  signs no one in.

### API tokens

The application's own mobile and command-line clients sign in with an API
token, which an action issues once it has checked the person and returns
in its output, the one time anyone sees it (decided 2026-10-16, like
Laravel Sanctum's `createToken`):

```jadpo fragment
action accept_invitation_in_app(token: InvitationToken) fails InvitationExpired -> AppSignedIn {
    var invitation: Invitation = attempt Invitation.redeem(token)
    return AppSignedIn {
        token: sessions.issue_token(invitation.member_id)    // an ApiToken, shown once
    }
}
```

A magic-link code is traded for a token by the generated
`POST /auth/magic-link/redeem` instead (`sign_in:`, above).

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
  route runs `sessions.tokens` itself (below); each needs `api_token:` in
  `authentication { }`.

#### Listing from a `GET` route

An application's "signed-in devices" screen is one route. A `GET` runs
`sessions.tokens(user_id)`, and `Service.keys(service_id)` for a service's
keys, directly, as generated queries that read what the runtime keeps
(decided 2026-10-16):

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
- The list goes through policy like any query's (area 9, question 8): it
  holds the tokens or keys of a holder whose record the caller may `read`,
  and is empty for any other. A route that lists a holder nobody may read
  is `policy.effect_ungranted`, and one anyone may call needs
  `Access.public` on the holder's `read:`.

### Signing in, end to end

A whole application that signs members up, keeps their notes and refuses a
suspended member. These compiled examples show the earlier, app-written
form: signing up starts a session for a handle without checking the
person, to exercise sessions on their own. A real application checks the
person with `sign_in:` (above). The principal and its one sign-in method:

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
it, so its grants let them issue and revoke its keys (area 9):

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
for whoever may read their record (area 9): the list is limited the way a
query's records are, so it is empty for a member the caller may not read.
`Member` grants a read of itself (`read: [id]`):

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

> Decided on 2026-10-16 (Richard), in area 11's "Acting for each user:
> OAuth grants". Not built yet and not yet on the roadmap.

Signing in (above) proves who a person is to this app. An OAuth grant is
the other direction: the app acting at another service (a user's Google
Calendar, an organisation's Xero, a Slack workspace) with that owner's
consent. It is not a sign-in method, and acting as an OAuth provider
(question 13) is a third, separate thing. Area 11 owns the connection and
the calls; this page owns what touches identity.

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
  failure. The callback needs no cookie, so the app's own API-token clients
  connect exactly as browsers do. Both appear in `jadpo routes`, OpenAPI and the API
  record, marked generated (area 7).
- **Who may connect for an owner is policy** (area 9): connecting and
  disconnecting need the `update` grant on the owner's record, so a user
  connects their own account and an organisation's grant needs whatever
  lets the caller update the organisation; no new effect is introduced. The
  start route checks it before answering.
- **Grants are held by the runtime**, like sessions, in a `jadpo_grants`
  system table in the owner entity's store, so grants need a database.
  Their tokens are encrypted with `application { grants: { encryption_key:
  config.grant_encryption_key } }`, a `SigningKey` setting rotated as a
  `signing_key:` is (area 10: the current value, and its `_PREVIOUS` value
  still read), never readable by code, and refreshed before expiry;
  `encryption_key:` joins the members of `application { }` a secret may be
  read in (area 10, "Where a secret may go").
- **Every call names whose grant it uses**, `on_behalf_of: owner_id`, an id
  of the owner's entity that the action has loaded or been given;
  `grant_missing` and `grant_revoked` are failures the app maps (area 11).
  The caller of the route need not be the owner: a scheduled job acting for
  an organisation names the organisation.
- **Disconnecting** is the generated `POST /connections/<name>/disconnect`
  and its operation, revoking at the provider when the package declares
  how. Erasing a
  person or record (area 15) deletes and revokes its grants; a revoked
  grant gives the `GrantRevoked` event so the app can ask the owner to
  reconnect.
- **In development and tests**, `jadpo dev grants add <connection> --owner
  <id>` makes a fake grant, and tests declare grants as fixtures.

## Failures

Authentication failures are raised by the runtime, never declared or listed in
`fails` (area 4). A client sees the usual envelope.

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
| `on_first_sign_in` refused, or no user and no `on_first_sign_in:` for a provider or token | `sign_in_refused` | `not_permitted` | 403 |
| Too many sends to one address | `rate_limited` | `rate_limited` | 429 |
| `magic_links.request` given a path to go back to that isn't on the app's site (chunk 7.3b3) | `sign_in_return_invalid` | `invalid_value` | 422 |
| A passkey ceremony's challenge expired, was used, was replaced or came from another browser or session | `passkey_challenge_expired` | `unauthenticated` | 401 |
| A passkey's answer doesn't check (origin, relying party, presence, verification, signature or counter), or its passkey isn't registered | `passkey_rejected` | `unauthenticated` | 401 |
| The passkey is registered already | `passkey_already_registered` | `conflict` | 409 |
| The person has 20 passkeys already | `passkey_limit_reached` | `conflict` | 409 |
| No passkey of the signed-in person has that `id` | `passkey_not_found` | `not_found` | 404 |

`user_disabled` and `service_disabled` are built in and never declared
(question 6), as are the `sign_in_` failures (decided 2026-10-16) and the
`passkey_` failures (chunk 5.5z), whose
wording for people is Jadpo's own (area 16, "Built-in emails"). With a revocation delay, a disabled caller is
noticed within that delay. The log records the method, the credential's
record id and the reason, never the credential.

## Errors

Every error this area's checks produce, with an example that triggers it.
The examples are one file each, so the application, principal and
sign-in methods sit beside the entity they name; a project keeps them in
`app.jadpo` and `authentication.jadpo`.

### `semantic.multiple_applications`

```jadpo check project=e-multiple-applications expect=semantic.multiple_applications
application {}

application {
    revocation_delay: 5m
}
```

```text output
error: `application { }` is declared twice
 --> main.jadpo:3:1
  |
1 | application {}
  | ----------- first declared here
3 | application {
  | ^^^^^^^^^^^ declared again
  reason: A project is one application, declared once in `app.jadpo`; its settings all go in that one block.
  next: Move what this `application` holds into the first, and remove it (agent)
  rule: semantic.multiple_applications
  help: docs/language/08-identity.html#semantic-multiple-applications

check failed: 1 error, 1 file
```

### `semantic.multiple_principals`

```jadpo check project=e-multiple-principals expect=semantic.multiple_principals
principal {
    user: {
        subject: Member.handle
    }
}

principal {
    user: {
        subject: Member.handle
    }
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
}
```

```text output
error: `principal { }` is declared twice
 --> main.jadpo:7:1
  |
1 | principal {
  | --------- first declared here
7 | principal {
  | ^^^^^^^^^ declared again
  reason: The caller is declared once, in `app.jadpo`: one `user:` and at most one `service:`, so every route reads the same caller.
  next: Move what this `principal` holds into the first, and remove it (agent)
  rule: semantic.multiple_principals
  help: docs/language/08-identity.html#semantic-multiple-principals

check failed: 1 error, 1 file
```

### `semantic.multiple_authentication`

```jadpo check project=e-multiple-authentication expect=semantic.multiple_authentication
principal {
    user: {
        subject: Member.handle
    }
}

authentication {
    browser_session: {
        cookie: "session"
    }
}

authentication {}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
}
```

```text output
error: `authentication { }` is declared twice
 --> main.jadpo:13:1
   |
 7 | authentication {
   | -------------- first declared here
13 | authentication {}
   | ^^^^^^^^^^^^^^ declared again
  reason: The ways a caller signs in are listed once, in `authentication.jadpo`, so one place says how anyone gets in.
  next: Move what this `authentication` holds into the first, and remove it (agent)
  rule: semantic.multiple_authentication
  help: docs/language/08-identity.html#semantic-multiple-authentication

check failed: 1 error, 1 file
```

### `semantic.principal_variant_required`

```jadpo check project=e-principal-variant-required expect=semantic.principal_variant_required
principal {}
```

```text output
error: The principal declares no kind of caller
 --> main.jadpo:1:1
  |
1 | principal {}
  | ^^^^^^^^^ no `user:` or `service:`
  reason: A principal says which entity a signed-in caller is: `user:` for a person, `service:` for another system, or both.
  next: Add `user: { subject: <Entity>.<field> }` (agent)
  rule: semantic.principal_variant_required
  help: docs/language/08-identity.html#semantic-principal-variant-required

check failed: 1 error, 1 file
```

### `semantic.principal_variant_unknown`

A role is a field of the user's entity, which policy reads (area 9), never
a kind of caller:

```jadpo check project=e-principal-variant-unknown expect=semantic.principal_variant_unknown
principal {
    user: {
        subject: Member.handle
    }
    admin: {
        subject: Member.handle
    }
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
}
```

```text output
error: `admin` isn't a kind of caller
 --> main.jadpo:5:5
  |
5 |     admin: {
  |     ^^^^^ not `user` or `service`
  reason: A principal declares `user:`, a person, and `service:`, another system; a role or a tenant is a field of the user's entity, which policy reads (area 9).
  next: Remove `admin` (agent)
  rule: semantic.principal_variant_unknown
  help: docs/language/08-identity.html#semantic-principal-variant-unknown

check failed: 1 error, 1 file
```

### `semantic.principal_variant_duplicate`

```jadpo check project=e-principal-variant-duplicate expect=semantic.principal_variant_duplicate
principal {
    user: {
        subject: Member.handle
    }
    user: {
        subject: Member.handle
    }
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
}
```

```text output
error: `user:` is declared twice
 --> main.jadpo:5:5
  |
2 |     user: {
  |     ---- first declared here
5 |     user: {
  |     ^^^^ declared again
  reason: A principal says once which entity a user is, so a caller is looked up one way.
  next: Remove one `user:` (agent)
  rule: semantic.principal_variant_duplicate
  help: docs/language/08-identity.html#semantic-principal-variant-duplicate

check failed: 1 error, 1 file
```

### `semantic.principal_variant_invalid`

```jadpo check project=e-principal-variant-invalid expect=semantic.principal_variant_invalid
principal {
    user: {
        active: Member.suspended == false
    }
}

entity Member {
    id: Uuid
    suspended: Bool
}
```

```text output
error: `user:` has no `subject:`
 --> main.jadpo:2:5
  |
2 |     user: {
  |     ^^^^ no `subject:`
  reason: A principal's variant names the unique field that identifies a caller, `subject: <Entity>.<field>`, and may say when a record counts as active, `active: <condition>`.
  next: Add `subject: <Entity>.<field>`, naming a unique field (agent)
  rule: semantic.principal_variant_invalid
  help: docs/language/08-identity.html#semantic-principal-variant-invalid

check failed: 1 error, 1 file
```

### `type.principal_subject_invalid`

```jadpo check project=e-principal-subject-invalid expect=type.principal_subject_invalid
principal {
    user: {
        subject: Member
    }
}

entity Member {
    id: Uuid
}
```

```text output
error: `Member` isn't an entity's field
 --> main.jadpo:3:18
  |
3 |         subject: Member
  |                  ^^^^^^ not `<Entity>.<field>`
  reason: A user is looked up by a unique field of the entity it is stored as, written `<Entity>.<field>`, such as `User.email`.
  next: Name a unique field of the entity, such as `User.email` (agent)
  rule: type.principal_subject_invalid
  help: docs/language/08-identity.html#type-principal-subject-invalid

check failed: 1 error, 1 file
```

### `type.principal_subject_not_unique`

```jadpo check project=e-principal-subject-not-unique expect=type.principal_subject_not_unique
principal {
    user: {
        subject: Member.handle
    }
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle
}
```

```text output
error: `Member.handle` doesn't identify one user
 --> main.jadpo:3:18
  |
3 |         subject: Member.handle
  |                  ^^^^^^^^^^^^^ not unique
  reason: A caller is looked up by its subject, so the subject names exactly one record: `Member.handle` isn't `unique: true`.
  next: Make `handle` a required field with `unique: true`, or name one that is (agent)
  rule: type.principal_subject_not_unique
  help: docs/language/08-identity.html#type-principal-subject-not-unique

check failed: 1 error, 1 file
```

### `type.principal_active_invalid`

```jadpo check project=e-principal-active-invalid expect=type.principal_active_invalid
principal {
    user: {
        subject: Member.handle
        active: Member.suspended
    }
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
    suspended: Bool
}
```

```text output
error: `Member.suspended` isn't a condition `active:` can use
 --> main.jadpo:4:17
  |
4 |         active: Member.suspended
  |                 ^^^^^^^^^^^^^^^^ not a comparison
  reason: `active:` is a condition on `Member`'s own fields, such as `Member.<field> == <value>`; a field alone says nothing to compare it with.
  next: Compare a field of `Member` with a value (agent)
  rule: type.principal_active_invalid
  help: docs/language/08-identity.html#type-principal-active-invalid

check failed: 1 error, 1 file
```

### `semantic.revocation_delay_invalid`

```jadpo check project=e-revocation-delay-invalid expect=semantic.revocation_delay_invalid
application {
    revocation_delay: 0m
}
```

```text output
error: `0m` can't be a revocation delay
 --> main.jadpo:2:23
  |
2 |     revocation_delay: 0m
  |                       ^^ not above zero
  reason: `revocation_delay:` is how long a disabled caller or a revoked session may keep working, a duration above zero such as `5m`; leaving it out checks the caller's record on every request.
  next: Write a duration such as `5m`, or leave `revocation_delay:` out (agent)
  rule: semantic.revocation_delay_invalid
  help: docs/language/08-identity.html#semantic-revocation-delay-invalid

check failed: 1 error, 1 file
```

### `semantic.auth_method_unknown`

Passwords are out of scope: checking the person is the application's job,
and a session starts once it has (`sessions.start`):

```jadpo check project=e-auth-method-unknown expect=semantic.auth_method_unknown
authentication {
    password: {}
}
```

```text output
error: `password` isn't a way to sign in
 --> main.jadpo:2:5
  |
2 |     password: {}
  |     ^^^^^^^^ not a sign-in method
  reason: A caller signs in with one of the language's methods: `browser_session`, `api_token`, `external_token`, `service_key` and `oauth_token`. Each decides where its credential travels and how it is checked.
  next: Remove `password:` (agent)
  rule: semantic.auth_method_unknown
  help: docs/language/08-identity.html#semantic-auth-method-unknown

check failed: 1 error, 1 file
```

### `semantic.auth_method_duplicate`

```jadpo check project=e-auth-method-duplicate expect=semantic.auth_method_duplicate
principal {
    user: {
        subject: Member.handle
    }
}

authentication {
    browser_session: {
        cookie: "session"
    }
    browser_session: {
        cookie: "admin_session"
    }
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
}
```

```text output
error: `browser_session:` is declared twice
 --> main.jadpo:11:5
   |
 8 |     browser_session: {
   |     --------------- first declared here
11 |     browser_session: {
   |     ^^^^^^^^^^^^^^^ declared again
  reason: Each way to sign in is one block, written once, so its settings are in one place.
  next: Move the settings into the first `browser_session:`, and remove this one (agent)
  rule: semantic.auth_method_duplicate
  help: docs/language/08-identity.html#semantic-auth-method-duplicate

check failed: 1 error, 1 file
```

### `semantic.auth_setting_unknown`

The language owns the cookie's flags, its lifetime and the CSRF check:

```jadpo check project=e-auth-setting-unknown expect=semantic.auth_setting_unknown
principal {
    user: {
        subject: Member.handle
    }
}

authentication {
    browser_session: {
        cookie: "session"
        same_site: "lax"
    }
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
}
```

```text output
error: `same_site` isn't a `browser_session` setting
 --> main.jadpo:10:9
   |
10 |         same_site: "lax"
   |         ^^^^^^^^^ unknown setting
  reason: A browser session takes `cookie:`, its cookie's name, and `origin:`; the language sets the cookie's flags, its lifetime and the CSRF check.
  next: Remove `same_site:` (agent)
  rule: semantic.auth_setting_unknown
  help: docs/language/08-identity.html#semantic-auth-setting-unknown

check failed: 1 error, 1 file
```

### `semantic.auth_setting_invalid`

```jadpo check project=e-auth-setting-invalid expect=semantic.auth_setting_invalid
principal {
    user: {
        subject: Member.handle
    }
}

authentication {
    browser_session: {
        cookie: "my session"
    }
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
}
```

```text output
error: `cookie:` isn't a cookie's name
 --> main.jadpo:9:17
  |
9 |         cookie: "my session"
  |                 ^^^^^^^^^^^^ not a cookie name
  reason: A browser session's cookie is named in text: 1 to 64 letters, digits, `_` and `-`, starting with a letter, such as `"session"`.
  next: Write the cookie's name in quotes, such as `cookie: "session"` (agent)
  rule: semantic.auth_setting_invalid
  help: docs/language/08-identity.html#semantic-auth-setting-invalid

check failed: 1 error, 1 file
```

### `semantic.auth_setting_required`

```jadpo check project=e-auth-setting-required expect=semantic.auth_setting_required
principal {
    user: {
        subject: Member.handle
    }
}

authentication {
    browser_session: {}
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
}
```

```text output
error: `browser_session:` doesn't name its cookie
 --> main.jadpo:8:5
  |
8 |     browser_session: {}
  |     ^^^^^^^^^^^^^^^ no `cookie:`
  reason: A browser session travels in a cookie the application names, so two applications on one domain never share one.
  next: Add `cookie: "<name>"`, such as `cookie: "session"` (agent)
  rule: semantic.auth_setting_required
  help: docs/language/08-identity.html#semantic-auth-setting-required

check failed: 1 error, 1 file
```

### `semantic.auth_principal_required`

```jadpo check project=e-auth-principal-required expect=semantic.auth_principal_required
authentication {
    browser_session: {
        cookie: "session"
    }
}
```

```text output
error: `browser_session:` signs in users, but no `user:` is declared
 --> main.jadpo:2:5
  |
2 |     browser_session: {
  |     ^^^^^^^^^^^^^^^ nobody to sign in
  reason: A browser session signs in a user, looked up through the principal's `user:` in `app.jadpo`, which names the entity and the unique field that identify one.
  next: Declare `principal { user: { subject: <Entity>.<field> } }` in `app.jadpo` (agent)
  rule: semantic.auth_principal_required
  help: docs/language/08-identity.html#semantic-auth-principal-required

check failed: 1 error, 1 file
```

### `semantic.auth_not_set_up`

Every route needs a signed-in caller unless it says otherwise, so routes
with nothing to sign anyone in could never be called:

```jadpo check project=e-auth-not-set-up expect=semantic.auth_not_set_up
type Memo = Text {
    max_length: 200
}

type AddMemo = Input {
    text: Memo
}

action add_memo(input: AddMemo) -> Unit {
    var memo: Memo = input.text
    assert memo != ""
}

routes Memos {
    prefix: /memos

    POST / {
        input: AddMemo
        status: SuccessStatus.no_content
        run: add_memo(input)
    }
}
```

```text output
error: `POST /memos` needs a signed-in caller, but nothing signs anyone in
 --> main.jadpo:17:5
   |
17 |     POST / {
   |     ^^^^^^ needs a signed-in caller
  reason: Every route needs a signed-in caller unless it says `authentication: RouteAuthentication.none`, and `authentication.jadpo` declares no way to sign in that is built, so no request could reach this route.
  next: Declare how callers sign in, `authentication { browser_session: { cookie: "<name>" } }`, or let anyone call the route with `authentication: RouteAuthentication.none` (human)
  rule: semantic.auth_not_set_up
  help: docs/language/08-identity.html#semantic-auth-not-set-up

check failed: 1 error, 1 file
```

### `type.auth_principal_output`

```jadpo check project=e-auth-principal-output expect=type.auth_principal_output
principal {
    user: {
        subject: Member.handle
    }
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

type Whoami = Output {
    caller: Principal.user
}
```

```text output
error: `Whoami.caller` holds a caller
 --> main.jadpo:19:13
   |
19 |     caller: Principal.user
   |             ^^^^^^^^^^^^^^ a caller
  reason: A caller value is who signed in, as the runtime holds it, and is never sent to a client; an output sends the fields a client needs.
  next: Type `caller` with the entity's id, such as `User.id`, or send the fields a client needs (agent)
  rule: type.auth_principal_output
  help: docs/language/08-identity.html#type-auth-principal-output

check failed: 1 error, 1 file
```

### `type.auth_principal_input`

```jadpo check project=e-auth-principal-input expect=type.auth_principal_input
principal {
    user: {
        subject: Member.handle
    }
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

type ActAs = Input {
    caller: Principal.user
}
```

```text output
error: `ActAs.caller` holds a caller
 --> main.jadpo:19:13
   |
19 |     caller: Principal.user
   |             ^^^^^^^^^^^^^^ a caller
  reason: A caller comes only from signing in, never from what a request sends, so a route takes it as `current_principal` and passes it on.
  next: Remove `caller`, and pass `current_principal` to what the route runs (agent)
  rule: type.auth_principal_input
  help: docs/language/08-identity.html#type-auth-principal-input

check failed: 1 error, 1 file
```

### `type.auth_principal_failure_context`

```jadpo check project=e-auth-principal-failure-context expect=type.auth_principal_failure_context
principal {
    user: {
        subject: Member.handle
    }
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

type MemoLocked = Failure {
    kind: FailureKind.conflict
    client: {
        caller: Principal.user
    }
}
```

```text output
error: `MemoLocked.caller` holds a caller
 --> main.jadpo:21:17
   |
21 |         caller: Principal.user
   |                 ^^^^^^^^^^^^^^ a caller
  reason: A failure is logged and its client fields reach the client, while a caller value holds who signed in, so a failure carries the caller's `id` instead.
  next: Type `caller` with the entity's id, such as `User.id` (agent)
  rule: type.auth_principal_failure_context
  help: docs/language/08-identity.html#type-auth-principal-failure-context

check failed: 1 error, 1 file
```

### `type.auth_principal_variant_unknown`

```jadpo check project=e-auth-principal-variant-unknown expect=type.auth_principal_variant_unknown
principal {
    user: {
        subject: Member.handle
    }
}

authentication {
    browser_session: {
        cookie: "session"
    }
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

type MemberView = Output {
    handle: Handle
}

type MemberNotFound = Failure {
    kind: FailureKind.not_found
}

query whoami(caller: Principal.user) fails MemberNotFound -> MemberView {
    from: Member
    where: Member.id == caller.id
    missing: MemberNotFound
}

routes Members {
    prefix: /members

    GET /me {
        output: MemberView
        run: whoami(current_principal.service)
    }
}
```

```text output
error: `current_principal.service` isn't a kind of caller
 --> main.jadpo:43:39
   |
43 |         run: whoami(current_principal.service)
   |                                       ^^^^^^^ not declared
  reason: `principal { }` declares no `service:`, so no service ever signs in; it declares `user`.
  next: Write `current_principal.user` instead of `current_principal.service` (agent)
  rule: type.auth_principal_variant_unknown
  help: docs/language/08-identity.html#type-auth-principal-variant-unknown

check failed: 1 error, 1 file
```

### `type.auth_public_principal`

```jadpo check project=e-auth-public-principal expect=type.auth_public_principal
principal {
    user: {
        subject: Member.handle
    }
}

authentication {
    browser_session: {
        cookie: "session"
    }
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

type MemberView = Output {
    handle: Handle
}

type MemberNotFound = Failure {
    kind: FailureKind.not_found
}

query whoami(caller: Principal.user) fails MemberNotFound -> MemberView {
    from: Member
    where: Member.id == caller.id
    missing: MemberNotFound
}

routes Members {
    prefix: /members

    GET /me {
        authentication: RouteAuthentication.none
        output: MemberView
        run: whoami(current_principal.user)
    }
}
```

```text output
error: A route anyone may call passes `current_principal`
 --> main.jadpo:44:21
   |
44 |         run: whoami(current_principal.user)
   |                     ^^^^^^^^^^^^^^^^^ no one is signed in here
  reason: `authentication: RouteAuthentication.none` reads no credential, so the route has no caller to pass on, and nothing it runs may use one.
  next: Remove `authentication: RouteAuthentication.none` so callers sign in, or stop passing `current_principal` (human)
  rule: type.auth_public_principal
  help: docs/language/08-identity.html#type-auth-public-principal

check failed: 1 error, 1 file
```

### `type.auth_sessions_context`

```jadpo check project=e-auth-sessions-context expect=type.auth_sessions_context
principal {
    user: {
        subject: Member.handle
    }
}

authentication {
    browser_session: {
        cookie: "session"
    }
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

test "signing out ends the session" {
    sessions.end()
}
```

```text output
error: A test can't call `sessions.end`
 --> main.jadpo:25:5
   |
25 |     sessions.end()
   |     ^^^^^^^^^^^^ outside an action
  reason: `sessions.end` ends the session of the request a route answers, so only an action calls it.
  next: Call `sessions.end` in the action a sign-in or sign-out route runs (agent)
  rule: type.auth_sessions_context
  help: docs/language/08-identity.html#type-auth-sessions-context

check failed: 1 error, 1 file
```

### `type.auth_principal_construction`

A caller only comes from signing in; tests, fixtures and seeds make one
from a record:

```jadpo check project=e-auth-principal-construction expect=type.auth_principal_construction
principal {
    user: {
        subject: Member.handle
    }
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

action act_as(member: Member) -> Principal.user {
    return Principal.user(member)
}
```

```text output
error: An action can't make a caller
 --> main.jadpo:19:12
   |
19 |     return Principal.user(member)
   |            ^^^^^^^^^^^^^^ made from a record
  reason: A caller only comes from signing in; tests, fixtures and seeds make one from a record with `Principal.user(...)`, and application code takes it as a parameter a route passes.
  next: Take a `Principal.user` parameter, which the route passes as `current_principal.user` (agent)
  rule: type.auth_principal_construction
  help: docs/language/08-identity.html#type-auth-principal-construction

check failed: 1 error, 1 file
```

### `type.auth_credential_stored`

An API token or a service key is shown once and kept only as its verifier,
so no entity can keep one:

```jadpo check project=e-auth-credential-stored expect=type.auth_credential_stored
principal {
    user: {
        subject: Member.handle
    }
}

authentication {
    api_token: {}
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
    last_token: ApiToken
}
```

```text output
error: `Member.last_token` keeps an API token
 --> main.jadpo:20:17
   |
20 |     last_token: ApiToken
   |                 ^^^^^^^^ a secret, stored
  reason: An API token is shown once, in the output of the action that issues it, and the runtime keeps only its verifier, so a copy of the database signs no one in.
  next: Remove `last_token`, and give the token in the output of the action that issues it (agent)
  rule: type.auth_credential_stored
  help: docs/language/08-identity.html#type-auth-credential-stored

check failed: 1 error, 1 file
```

## Review

### Status

Built in chunks 4.3 and 4.3b. `jadpo check` reads `application { }`'s
`revocation_delay:`, `principal { }` and `authentication { }` (each declared
once), with each variant's `subject:` and `active:`, `browser_session:`
with its cookie, and `api_token:` and `service_key:`; routes need a
signed-in caller unless they say `authentication: RouteAuthentication.none`
(or `.fresh`), an API lists the methods it accepts, and protected routes
with nothing to sign anyone in are `semantic.auth_not_set_up`. A route
passes `current_principal`, or the kind it selects, to what it runs, and
actions, queries and functions take `Principal.user` and
`Principal.service` parameters whose `id`, `subject` and `strength` they
read; a caller never sits in an `Input`, an `Output` or a failure, and an
`ApiToken` or `ServiceKey` never in an entity. Tests, fixtures and seeds
make a caller with `Principal.user(record)` or `Principal.service(record)`,
and a route test signs in with `as:`, through a browser session, an API
token or a service key. The runtime authenticates a request before its
input is read, refuses it as "Failures" lists, checks a cookie write's
`Origin` and CSRF token, starts and ends sessions and issues, lists and
revokes API tokens and service keys from actions (and lists them for a
`GET` route), and keeps sessions and
tokens in the `jadpo_sessions` system table beside the principal's users
and keys in `jadpo_service_keys` beside its services. Since chunk 4.4 the
owning record's grants decide who may list (`read`) and revoke or issue
(`update`) a token or a key (area 9, question 8). OpenAPI shows each
API's security schemes, the API record marks the routes anyone may call,
and `jadpo routes` lists each route's authentication. Since chunk 4.9 the
record also names each API's sign-in methods and each route's kind of
caller, a request with a credential to a route anyone may call counts
against its address before it runs, and a handled failure undoes its
`sessions.start` or `sessions.end`. Since chunk 5.1b `revocation_delay:`
signs browser sessions, API tokens and exchanged service tokens with each
method's `signing_key:`, read from a `SigningKey` setting with its
`_PREVIOUS` value: a request whose signed part hasn't expired signs in
without a read, renewing reads the record, and a `fresh` route always
does. The runtime answers `POST /auth/exchange`, which the API record,
OpenAPI and `jadpo routes` list, and a browser session's `origin:` is
read from a `Url` setting. Since chunk 5.5z `sign_in:` is read, with
`passkey:` built: the runtime answers its seven routes, registering,
signing in, listing, renaming and removing passkeys, on `webauthn-rs`
through the host, keeping passkeys in `jadpo_passkeys` and ceremonies in
`jadpo_passkey_challenges`; the API record, OpenAPI, `jadpo routes` and
the per-locale failure export list them.

Since chunk 7.3b `sign_in:`'s `magic_link:` is built: the runtime
answers its three routes, keeping one row per address in
`jadpo_sign_in_requests`, sends Jadpo's own email through the `email:`
connection with its link and code made as it is sent, signs the person in
with a browser session or an API token, and runs `on_first_sign_in:`'s
action for an address no user has; the API record, OpenAPI, `jadpo
routes` and the per-locale failure export list them ("Magic links
(chosen in chunk 7.3b)" below). Since chunk 7.3b2 `email:` fills and
sends the app's own sign-in email instead, and `send:` hands the link and
the code to the app's own action, retried as a subscriber ("The app's own
sign-in email and sending (chosen in chunk 7.3b2)" below). Since chunk
7.3b3 `generate_routes: false` switches the routes off and the app's own
routes call `magic_links.request`, `.redeem` and `.redeem_code`, through
the same code the generated routes run ("The app's own routes (chosen in
chunk 7.3b3)" below). Since chunk 7.3b4 every way in through a magic
link publishes `SignedIn`, `SignInRefused` and `SignInCodesDisabled` to
the app's subscribers, the refusal's reason kept on the request's log
line, and a test builds a `FirstSignIn` to call `on_first_sign_in:`'s
action directly ("Sign-in events (chosen in chunk 7.3b4)" below). Since
chunk 5.5z2 `passkey:` needs `magic_link:` beside it, how a person with
no passkey left gets back in; the device lists a passkey under the
person's address; a passkey's sign-in publishes `SignedIn` and
`SignInRefused`, and adding and removing one `PasskeyAdded` and
`PasskeyRemoved`; and a route test drives the routes with
`test_passkeys` ("Passkey recovery, events and route tests (chosen in
chunk 5.5z2)" below). Since chunk 8.2l the Workers host answers the
ceremonies too, `webauthn-rs`'s checks written in pure Rust on
jadpo-value's ES256 and RS256 and held to `webauthn-rs`'s answers, a
device's attestation `none` or packed self-attestation there ("Passkeys
on Workers' attestation (chosen in chunk 8.2l)" below).

Built in chunk 9.1d1: `external_token:` with `issuer: config.issuer`
and `audience: config.audience` (`Url` and `Text` settings) accepts area
19's EdDSA, RS256 and ES256 JWTs
for provisioned subjects on native and Workers. Multi-column and
structured principal subject fields remain explicitly
`toolchain.not_built_yet` for this method. Issuer and audience
comparison is exact, and each request reads the subject's current user
record, active condition and lifecycle visibility. Provider profile and
permission claims do not become stored identity or policy grants. The
runtime uses one provider cache per application startup, with at most 64
keys, 64 KiB discovery and 256 KiB JWKS documents, 3 seconds per fetch,
5 minutes of cached keys, and at most one refresh per 30 seconds even
when a request is cancelled. Explicit encryption keys are ignored;
signing keys retain area 19's strict `Jwk` contract. Invalid credentials
are 401, invalid provider/configuration data is
`authentication_misconfigured` (500), and unreachable providers are
`authentication_unavailable` (503); responses expose neither provider
content nor credentials. The outer route deadline also cancels discovery
and key requests. Production native transport filters actual DNS answers;
Workers uses the public network with `global_fetch_strictly_public`.
Local generated-CA fixture transports exercise authentication and TLS on
both hosts, with explicit test-only address mapping; they do not prove
Cloudflare production DNS filtering.

Not yet, each `toolchain.not_built_yet` where it is written:

- `sign_in:`'s `providers:` and `oauth_provider:`, after the first build,
  and `on_first_sign_in:` for provider and external-token sign-in.
  Chunk 9.1d1 builds external tokens for provisioned users; declaring
  their first-sign-in hook remains an explicit `toolchain.not_built_yet`.
  An unknown subject without a hook is `sign_in_refused` (403).
- Scope declarations and policy scope restrictions remain unbuilt.
  As area 9 specifies, an external method narrows scopes only when it
  declares the scopes it reads; undeclared provider scope/profile claims
  never replace application authority.
- A setting loosened past its limit, until acceptances (area 18).
- OAuth grants: `OAuthGrant` credentials, the connecting and disconnecting
  routes and `on_behalf_of:` (decided 2026-10-16, area 11); no roadmap chunk
  yet. `Driver.oidc` for Google and Microsoft sign-in comes with providers,
  after the first build.

**Listing tokens and keys from a `GET`** (decided 2026-10-16, built in
chunk 4.4c). `sessions.tokens(user_id)` and `Service.keys(service_id)` are
for an application's own "signed-in devices" and key screens. A `GET`
route runs them directly, as generated queries, and `IssuedToken` and
`IssuedKey` are types an `Output` can hold, so
`GET /devices { run: sessions.tokens(current_principal.user.id) }` works
the way a named query does, with no action to copy each `IssuedToken`'s
fields into an `Output` of its own.

**How an API token is issued** (decided 2026-10-16). The page says `api_token`
travels as `Authorization: Bearer` for the application's own mobile and
command-line clients, but not how a client gets one: `sessions.start` sets a
cookie, and nothing hands a token to a client. *Recommendation:* a
generated `sessions.issue_token(user_id) -> ApiToken`, called by an action
a sign-in route runs and returned in its output once (shown once, stored as
a verifier, like `Service.issue_key` and Laravel Sanctum's `createToken`),
with `sessions.end()` ending the token a request came with, and
`sessions.tokens(user_id)` and `sessions.revoke_token(token_id)` beside
`Service.keys` and `Service.revoke_key` for an application's own "signed-in
devices" screen. *Decided 2026-10-16:* yes, as recommended; chunk 4.3b
builds it.

### Consistency with the rest of the language

- **`principal` has five meanings** (syntax review #44): the declaration, the
  application setting, a validator's variant, a credential role and the usual
  parameter name.
- **Mappings use `->`** (`id -> Principal.user.user_id`) where every other block
  uses `name: value` (area 3's colon rule).
- **`secret` and `mode` are vague**: `secret:` is a signing key, `mode:` picks
  the credential format (syntax review #46, #47).
- **The transport key is the variant** (`bearer: authorization_header`, whose
  value has one legal option), against area 2's setting-options rule.
- **Declared fields that cannot vary** (principal fields, claims limited to two
  sources) are the "fake flexibility" area 2 removed elsewhere.
- **Inactive failures** already use area 4's `condition: Failure` form
  (`inactive: UserDisabled`) but may be `Rejected` (422), which area 4 keeps
  for business rules.
- **Placement**: area 1 says `authentication.jadpo` holds methods "and their
  failures", while area 4 puts every failure in a `failures/` folder. Question
  6 removes the conflict.

### Carried to later areas

| Item | Area | Status |
|---|---|---|
| Route syntax around `authentication: RouteAuthentication.none` and `.fresh`; `route.auth_value_invalid` (a `route.*` rule, left to area 7's page) | 7 | Input to area 7 |
| Generated `POST /auth/exchange` in the route inventory and OpenAPI, and the security schemes per route | 7 | Built in chunk 5.1b |
| Each API's accepted methods (`authentication: [AuthenticationMethod.browser_session, AuthenticationMethod.api_token]` in area 7's `apis: { }`); a route only narrows them | 7 | Input to area 7 |
| Webhook signature checks as a route's authentication (`authentication: stripe.signature`) | 11 | Input to area 11 |
| Policy reads the caller and the entity named by `subject:`; `Access.public` versus `authentication: none`; custom roles and tenants come from entities, not the principal | 9 | Input to area 9 |
| Which callers may issue, list and revoke service keys | 9 | Depends on question 7 |
| Signing keys, OIDC issuer and audience as secret and plain configuration; key rotation | 10 | Input to area 10 |
| Verifying a person before `sessions.start` (magic links, OIDC code exchange) as integrations | 11 | Depends on question 9 (Superseded 2026-10-16: magic links and providers are built in, `sign_in:` above.) |
| The magic-link email, its wording, and the five levels of changing a built-in email | 16, 10 | Decided 2026-10-16 (area 16, "Built-in emails") |
| Schedules and subscribers run with no caller | 12 | Input to area 12 |
| Signing in as a user or service in tests | 13 | Input to area 13 |
| `toolchain.target_auth_not_implemented`, raised only by `jadpo build`; moving the "protected routes but no authentication" check into `jadpo check` | 14 | Built in chunk 4.3: `semantic.auth_not_set_up` |
| A `lifecycle { }` block on an entity written `id: Uuid identity` is silently ignored | 5 | Raised |
| Sessions in the principal's user store, rather than a `sessions: { store: }` setting (chosen in chunk 4.3, below) | 5 | Recorded on area 5's system tables |
| Deleting ended sessions from the minute tick | 12 | Built in chunk 6.10: area 12, "The tick" |

### Open questions

1. **How simple should declaring authentication be?** Today: strategies,
   transports, named validators, modes, claims and a resolution per variant,
   120 lines in the golden todo with the user lookup written twice. Three
   options are in the [proposals file](research/08-identity-proposals.md): A,
   name the methods and let the language own transports, token formats, claims
   and lookups; B, keep today's model with the decided syntax; C, accept only
   external OIDC tokens and service keys. *Recommendation:* A. Rails, Phoenix,
   Django, Sanctum and Auth.js all converged on "the author names the user
   record, the methods and the public routes; the framework owns the rest", and
   today's extra flexibility does not build anyway. *Decided 2026-10-12:* yes, as recommended.
2. **Are the principal's fields built in?** Today they are declared, but only
   `subject`, a UUID id and `authentication_strength` build. *Recommendation:*
   yes: `id` (typed `User.id` or `Service.id`), `subject` and `strength`.
   Anything else is read from the entity, which keeps credentials free of
   profile and permission data (an AUTH-001 rule) and fixes the UUID
   conversions. *Decided 2026-10-12:* yes, as recommended.
3. **Is the service variant optional?** Today a principal without `service`
   is `semantic.principal_variant_required`, so every application declares
   one. *Recommendation:* yes, at least one variant; with no `service`,
   `service_key` is an error and `current_principal` is simply a user. *Decided 2026-10-12:* yes, as recommended.
4. **One revocation setting?** Today `revocation { mode: bounded
   maximum_delay: 5m }` or `{ mode: immediate }`. *Recommendation:*
   `revocation_delay: 5m` in `application { }`, left out for immediate. The
   strict choice is the default, as with authentication on routes, and two of
   the three revocation errors disappear. Clerk states its bound the same
   way, as a 60-second token lifetime. *Decided 2026-10-12:* yes, as recommended.
5. **How does code read the caller?** Today routes use `current_principal`
   and callables can also read an implicit `principal`. *Recommendation:*
   `current_principal` in routes only, passed to actions as a typed parameter.
   Signatures then show which actions need a caller, tests pass one in, and
   schedules visibly have none. Policy (area 9) gets the caller from the
   runtime without code passing it. *Decided 2026-10-12:* yes, as recommended.
6. **Declared or built-in failures for a disabled caller?** Today each
   resolution names a declared failure of kind `NotPermitted` or `Rejected`.
   *Recommendation:* built-in `user_disabled` and `service_disabled`, kind
   `not_permitted` (403), with no declaration. Every application declares the
   same two today, and it removes the clash between area 1's
   `authentication.jadpo` and area 4's `failures/` folders. *Decided 2026-10-12:* yes, as recommended.
7. **Who stores service keys?** Today the application declares a
   `ServiceCredential` entity and maps six of its fields; sessions are already
   compiler-owned. *Recommendation:* the language stores keys, as Laravel
   Sanctum owns `personal_access_tokens`, and generates `Service.issue_key`,
   `Service.keys` and `Service.revoke_key` so an application can build its own
   key management screens. The application declares only `Service`. *Decided 2026-10-12:* yes, as recommended.
8. **What does a route say about authentication?** Today `auth: none` or
   `auth: fresh`, and every route accepts every method. *Recommendation:*
   spell it `authentication: RouteAuthentication.none` and
   `authentication: RouteAuthentication.fresh` (syntax review
   #41), the one word a reviewer must not miss on a public route; each of area
   7's APIs lists the methods it accepts, so an admin API can refuse service
   keys without per-route lines. *Decided 2026-10-12:* yes, as recommended.
9. **Can Jadpo code start and end a session?** Today only a trusted host
   program can. *Recommendation:* yes: `sessions.start(user_id)` and
   `sessions.end()`, callable only from actions run by a route, with the
   compiler setting and clearing the cookie and CSRF token. Passwords stay out;
   checking the person is the application's job. Without this, every
   application needs a second program just to log people in. *Decided 2026-10-12:* yes, as recommended.
10. **How do tests, fixtures and seeds name a caller?** (carried from area
    13) Area 13 builds callers with `Principal.user(user)` and signs in with
    `as:`, but on this page a caller only comes from signing in.
    *Recommendation:* keep `Principal.user(user)` test-only (tests, fixtures,
    seeds), and describe it with `as:` alongside how the caller is read. *Decided 2026-10-12:* yes, as recommended.
11. **Is "Sign in with Google" built in?** The `oidc` method above only
    accepts tokens another service has already issued (Auth0, Clerk); the
    sign-in flow itself is left to the application. *Recommendation:* yes.
    `sign_in_providers: { google: { ... } github: { ... } }` in
    `authentication.jadpo`, with presets for common providers and a generic
    OpenID Connect entry for any other. The runtime generates
    `/auth/{provider}/start` and `/auth/{provider}/callback`, runs the
    authorization-code flow with PKCE, state and nonce, keeps each linked
    provider account in a compiler-owned table (provider and its user id to
    `User.id`), and starts a session. The first sign-in calls an action the
    application names (`on_first_sign_in: create_user_from_provider`),
    receiving the provider's profile; the application decides whether to
    create a user or link an existing one, and linking by email is allowed
    only when the provider says the email is verified. The existing `oidc`
    method is renamed `external_token`, so the two are not confused.
    *Decided 2026-10-12:* yes, as recommended; acting as an OAuth provider is
    built in too (question 13).
    *Revised 2026-10-16 (Richard):* `sign_in_providers:` becomes `sign_in:`,
    one list of every way to check a person in `authentication.jadpo`,
    and magic-link sign-in is built in beside the providers, replacing
    question 9's "checking the person is the application's job". The
    first-sign-in action becomes `on_first_sign_in:` on `authentication { }`,
    the one decision point for every way in, external tokens included:
    create, link or refuse, with a refusal that looks the same to the
    person. See "Checking the person: `sign_in:`" above.
12. **Should authentication checks read a cache?** *Recommendation:* no.
    Area 5 already decided that authentication always reads the database.
    The strict default is one primary-key read of the session joined to the
    user's disabled flag; with `revocation_delay` set there is no read at
    all, which is faster than any cache. A cached session would also survive
    a revocation made while the cache was unreachable, turning a cache
    outage into a security gap. `revocation_delay` stays the one speed
    setting. *Decided 2026-10-12:* yes, as recommended.
13. **How does an application act as an OAuth provider?** Richard expects
    other people's applications to sign users in through a Jadpo
    application and call its API on their behalf. *Recommendation:* built
    in, following OAuth 2.1 and OpenID Connect, the way Doorkeeper (Rails),
    Passport (Laravel) and django-oauth-toolkit add it to a framework. An
    `oauth_provider: { scopes: { ... } }` block in `authentication.jadpo`
    declares the scopes, each with a description shown to the person. The
    runtime generates the authorization, token, revocation and discovery
    endpoints and the signing keys' public set; only the authorization code
    flow with PKCE, refresh tokens that rotate, and client credentials for
    server-to-server clients; no implicit or password grants. Registered
    client applications are stored by the language, with generated
    `OAuthClient.register`, `OAuthClient.list` and `OAuthClient.revoke`, as
    service keys are. The consent screen is a route the application writes,
    because it is branded UI; it calls a generated `oauth.approve(...)`. A
    token arrives through a new `AuthenticationMethod.oauth_token`, and its
    scopes narrow what policy allows (area 9): a grant can require a scope,
    and a token never allows more than the person could do themselves.
    Designed now; built after the golden todo, which does not need it.
    *Decided 2026-10-12:* built in, as recommended, with no reliance on a
    third-party authorization server (Richard, after briefly ruling it out).
    The main uses: AI connectors (the MCP specification requires OAuth 2.1
    for remote servers), integrations and marketplaces, and partners acting
    for a customer. Scopes from any token, `oauth_token` or
    `external_token`, narrow what policy allows.
14. **How does the app act at another service for each user?** Raised
    2026-10-16 with area 11's packages, for a user's calendar or an
    organisation's accounting system. *Decided 2026-10-16 (Richard):* an
    `OAuthGrant` credential on a connection, owned by one of the app's
    records; generated `start`, `callback` and `disconnect` routes, the
    start route needing a signed-in caller and keeping the `state` on the
    server against its session or API token; connecting needs the `update`
    grant on the owner; grants in `jadpo_grants`, encrypted with
    `grants: { encryption_key: }`; tokens held and refreshed by the runtime, never readable
    by code; every call names its owner with `on_behalf_of:`. See "Acting
    for a user at another service: OAuth grants" above and area 11.
15. **`routes: false` can't be written: `routes` is a reserved word.**
    Raised 2026-10-20 in chunk 7.3b. `magic_link: { routes: false }` (and
    area 7's "Generated routes", which names the same setting) is refused
    by the parser, since no member key may be a reserved word and
    `routes` starts a routes declaration. *Recommendation:* rename the
    setting `generate_routes: false`, which reads as what it switches off
    and needs no grammar exception; the alternative, letting reserved
    words be member keys, loosens a rule every block relies on for its
    error messages. *Decided 2026-10-20 (for Richard, as recommended):*
    `generate_routes: false`; chunk 7.3b3 builds it.

### Decisions

| Topic | Decision | Date |
|---|---|---|
| Passkeys on Workers | Built for Workers on jadpo-value's pure-Rust ES256 and RSA checks (signature, challenge, origin, counter), since the WebAuthn library runs only natively; `passkey:` and its magic-link recovery work on both targets (Richard; built in chunk 8.2l) | 2026-10-21 |
| Passkeys on Workers' attestation (chosen in chunk 8.2l) | Registration asks for no attestation, and on Workers a device's statement must be `none` or packed self-attestation, whose signature by the new key is checked; one signed by a certificate (packed with `x5c`, TPM, Android key, Apple, FIDO U2F) is `passkey_rejected` rather than taken unchecked, since the Workers host reads no certificate chains and browsers send `none` when none is asked for. The native host's `webauthn-rs` checks those statements and accepts a good one, keeping no attestation either way. Every other check is `webauthn-rs`'s, on both hosts | 2026-10-21 |
| Unnamed singletons | Carried from area 1 (and the owner's 2026-10-04 selection): `application { }` and `principal { }` carry no name; the application's principal setting is removed | 2026-10-12 |
| Root files | Carried from area 1: `app.jadpo` holds the application and principal; `authentication.jadpo` holds the methods | 2026-10-12 |
| Colon rule | Carried from area 3: every member is `name: value`, so `user: { }`, `browser_session: { }` | 2026-10-12 |
| Setting options | Carried from area 2: closed options are lower_snake_case names, never strings (Superseded 2026-10-12: options are written `Type.value`; see area 2 question 17.) | 2026-10-12 |
| Runtime kinds | Carried from area 4: `unauthenticated` and `misconfigured` are raised only by the runtime; authentication failures are never listed in `fails` | 2026-10-12 |
| Mapping form | Carried from area 4: `condition: Failure` everywhere, as in `inactive: UserDisabled` | 2026-10-12 |
| Principal in failures | Carried from area 4: the signed-in principal cannot be put in a failure's fields | 2026-10-12 |
| How simple authentication is | Option A: the author names the user entity, the methods and the public routes; the language owns transports, token formats, claims and lookups | 2026-10-12 |
| Principal fields | Built in: `id` (`User.id` or `Service.id`), `subject` and `strength`; anything else is read from the entity | 2026-10-12 |
| Service variant | Optional; with no `service`, `service_key` is an error and `current_principal` is a user | 2026-10-12 |
| Revocation setting | `revocation_delay: 5m` in `application { }`; left out means immediate | 2026-10-12 |
| Reading the caller | `current_principal` in routes only, passed to actions as a typed parameter; policy gets the caller from the runtime | 2026-10-12 |
| Disabled caller failures | Built-in `user_disabled` and `service_disabled`, kind `not_permitted` (403) | 2026-10-12 |
| Service key storage | The language stores keys and generates `Service.issue_key`, `Service.keys` and `Service.revoke_key`; the application declares only `Service` | 2026-10-12 |
| Route setting spelling | `authentication: RouteAuthentication.none` / `.fresh`; each API lists the methods it accepts | 2026-10-12 |
| Starting and ending sessions | `sessions.start(user_id)` and `sessions.end()` from actions a route runs; the compiler sets and clears the cookie and CSRF token | 2026-10-12 |
| Callers in tests, fixtures and seeds | `Principal.user(user)` in tests, fixtures and seeds only, with `as:` | 2026-10-12 |
| Sign-in providers | (Revised 2026-10-16: `sign_in:` and `on_first_sign_in:`, below.) Built-in `sign_in_providers:` with presets and generic OpenID Connect; generated start and callback routes; linked accounts stored by the language; `on_first_sign_in:` action decides create or link, email linking only when verified; `oidc` renamed `external_token`; acting as an OAuth provider built in (question 13) | 2026-10-12 |
| Caching authentication | Never: authentication always reads the database; `revocation_delay` is the speed setting | 2026-10-12 |
| Acting as an OAuth provider | Built in, no third-party server: OAuth 2.1 and OpenID Connect endpoints generated; authorization code with PKCE, rotating refresh tokens, client credentials; clients stored by the language (`OAuthClient.register`, `.list`, `.revoke`); consent screen is an application route calling `oauth.approve`; `AuthenticationMethod.oauth_token`; scopes from any token narrow policy; built after the golden todo (Richard) | 2026-10-12 |
| Declarations (chosen in chunk 4.3) | `application`, `principal` and `authentication` are each declared once (`semantic.multiple_*`), in any file of a single-file project and in `app.jadpo` and `authentication.jadpo` otherwise. A variant holds `subject:`, a required `unique: true` field of an entity kept in a PostgreSQL store, and an optional `active:`; left out, every record of the entity is active. `active:` is a condition on that entity's own fields named as a query names them (`Member.status == MemberStatus.active`): comparisons with literals, enum values, `none` and `clock.now`, joined with `and` and `or`. A record the entity's lifecycle hides can't sign in. `oidc:` is refused with its rename to `external_token:` | 2026-10-16 |
| Browser sessions (chosen in chunk 4.3) | `browser_session:` requires `cookie:` (1 to 64 letters, digits, `_` and `-`, starting with a letter). The token is 32 random bytes in hexadecimal in that cookie (`HttpOnly; Secure; SameSite=Strict; Path=/`), the CSRF token the same in `<cookie>_csrf`, readable by the application's scripts; both are kept only as BLAKE3 verifiers. A session lasts 30 days from its start, with no sliding renewal. Every method but `GET` that comes with the cookie needs an `Origin` whose host is the request's `Host` (until configuration gives `origin:`) and `X-Jadpo-CSRF` matching the session, else 403 `csrf_rejected`. An unknown or ended session is 401 `invalid_credentials` and its response clears both cookies. A cookie is read only on an API that accepts `browser_session`; an `Authorization` header where no bearer method is built is 401 `invalid_credentials` | 2026-10-16 |
| Where sessions live (chosen in chunk 4.3) | `jadpo_sessions` is a system table in the store the principal's user entity lives in, in its migration snapshots, read with the user's record in one statement (question 12); area 5's `sessions:` store setting isn't needed while that holds, and a store of their own (Redis) would bring it back | 2026-10-16 |
| Issuing API tokens | A generated `sessions.issue_token(user_id) -> ApiToken`, called by an action and returned in its output once, kept only as a verifier; `sessions.end()` ends the token a request came with; `sessions.tokens(user_id)` and `sessions.revoke_token(token_id)` beside `Service.keys` and `Service.revoke_key` (Richard) | 2026-10-16 |
| Listing tokens and keys from a `GET` | A `GET` route may run `sessions.tokens(user_id)` and `Service.keys(service_id)` directly, as generated queries; `IssuedToken` and `IssuedKey` are types an `Output` can hold (Richard) | 2026-10-16 |
| Signing in on a request (chosen in chunk 4.3) | Authentication runs after routing, inside the route's deadline and before its input is read; a route that selects `current_principal.user` refuses a service with 403 `not_permitted`. A signed-in caller's requests count against its own allowance, 600 a minute (area 16), counted per worker thread natively until the Redis store; a request with a credential that is refused counts against its address, and every other against its address as before. Idempotency keys are kept per caller (`user <id>`), and per `anonymous` on routes anyone may call. A response whose action started or ended a session isn't kept for a repeat, so a retry signs in again | 2026-10-16 |
| Starting and ending sessions (chosen in chunk 4.3) | `sessions.start(user_id)` takes the user entity's `id`, `sessions.end()` nothing; only actions call them, not functions, queries, tests, fixtures or seeds. Each writes the sessions' store, so the action writes that store (one store per action, area 6). A test that calls such an action keeps the session with no response to set it on. `sessions.end()` on a request with no session ends none and still clears the cookies | 2026-10-16 |
| The caller in code (chosen in chunk 4.3) | `current_principal` alone is a `Principal`, whose fields aren't read until a kind is selected; a route passes `current_principal`, `current_principal.user` or one of its fields, never both kinds. A caller's `id` counts as a record loaded earlier, so writing a record that refers to it needs no not-found failure. `as:` signs in a user through a real browser session (a session kept in its own transaction, the cookie, the CSRF token and an `Origin`); `as:` on a route anyone may call is `semantic.request_invalid` | 2026-10-16 |
| Reports (chosen in chunk 4.3) | Each API's OpenAPI document lists the accepted methods as security schemes (`browser_session` as an `apiKey` cookie) and requires them at the top; a route anyone may call says `security: []`, and every other answers 401 (`authentication_required`, `invalid_credentials`, `ambiguous_credentials`), 403 (`user_disabled`, and `csrf_rejected` on a write) and 503 (`authentication_unavailable`). The API record writes `authentication: none` under a route anyone may call. `jadpo routes` shows `required`, `none` or `fresh`. Wrong route and API values are `route.authentication_invalid` and `api.authentication_invalid` (area 7) | 2026-10-16 |
| External-token network and provisioned subjects (Richard; chunk 9.1d1) | Public HTTPS issuer and keys only, no redirects; exact issuer metadata may name a cross-authority public HTTPS `jwks_uri`, never token header URLs. EdDSA/RS256/ES256 verification uses area 19, followed by a current active-user lookup on every request. Compiler-owned bounded discovery/cache/refresh; no provider profile authority. Unknown subject is 403 `sign_in_refused`. External `on_first_sign_in:` and scopes remain explicitly unbuilt; this does not complete provider login or the golden application. | 2026-10-22 |
| Not built yet (chosen in chunk 4.3; updated in 4.3b) | `revocation_delay:` is checked for a positive duration, then `toolchain.not_built_yet` until signing keys (chunk 5.1b), as are `origin:` and a `signing_key:` read from configuration; a `signing_key:` written as a literal is `semantic.auth_setting_invalid`. `external_token:`, `sign_in_providers:` and `oauth_provider:` wait for after the first build (Superseded in chunk 5.1b: the delay, `signing_key:` and `origin:` are built, below) | 2026-10-16 |
| Bearer credentials (chosen in chunk 4.3b) | `api_token:` and `service_key:` take only `signing_key:` (a literal is `semantic.auth_setting_invalid`, a configuration value waits for 5.1b), so each is written `{}`; `api_token:` needs the principal's `user:` and `service_key:` its `service:` (`semantic.auth_principal_required`). Each travels as `Authorization: Bearer <value>`, the scheme in any case: an API token is `jadpo_token_` and 64 hexadecimal digits, a service key `jadpo_key_` and 64, 32 random bytes kept only as their BLAKE3 verifiers. The prefix says which method a credential is; one the route's API doesn't accept, or any other value, is 401 `invalid_credentials` without clearing cookies. A bearer credential needs no `Origin` or CSRF token | 2026-10-16 |
| API tokens (chosen in chunk 4.3b) | A token is a session without a cookie: a `jadpo_sessions` row with no CSRF verifier, lasting 30 days from `sessions.issue_token(user_id) -> ApiToken`, with no sliding renewal; a cookie never signs in with a token's row, nor a bearer with a session's. `sessions.end()` ends the token or session a request came with, needing either method. `sessions.tokens(user_id) -> List<IssuedToken>` lists the user's tokens that haven't ended, oldest first; `sessions.revoke_token(token_id)` takes a `Uuid` and deletes only a token, never a browser session; an unknown `id` revokes nothing | 2026-10-16 |
| Service keys (chosen in chunk 4.3b) | `issue_key`, `keys` and `revoke_key` belong to the entity the principal's `service:` names, once `service_key:` is declared; on any other entity they are `semantic.unknown_callee`. `issue_key(service_id, expires_at: Instant) -> ServiceKey` keeps the key until `expires_at`, `keys(service_id) -> List<IssuedKey>` lists those that haven't expired, oldest first, and `revoke_key(key_id: Uuid)` deletes one. Keys live in `jadpo_service_keys` (`id`, `principal_id`, `verifier`, `started_at`, `ends_at`) in the service entity's store, read with the service's record, `active:` and lifecycle in one statement. A service caller's rate limit and idempotency keys are its own (`service <id>`) | 2026-10-16 |
| Issued credentials in code (chosen in chunk 4.3b) | `ApiToken` and `ServiceKey` are built-in text types an `Output` can send; an entity field of either is `type.auth_credential_stored`. A response from an action that issued one is never kept for an idempotent repeat, so a retry runs again. `IssuedToken` and `IssuedKey` are built-in values with `id: Uuid`, `started_at: Instant` and `ends_at: Instant` that code reads. None of the six operations fails: like `sessions.start`, each takes an `id` the action has already loaded, and an action calls them (`type.auth_sessions_context`), writing or reading the store their table lives in; the two reads are also run by a `GET` route (chunk 4.4c) | 2026-10-16 |
| Signing in from a route test (chosen in chunk 4.3b) | `as: Principal.user(...)` signs in through a browser session when the route's API accepts one, otherwise through an API token; `as: Principal.service(...)` through a key issued as `issue_key` issues one, lasting 30 days. Each is kept for real in its own transaction and sent as a client sends it. With nothing declared that signs that kind of caller in, `as:` is `semantic.request_invalid` | 2026-10-16 |
| Checking the person | (Revised 2026-10-16: providers are connections, named in `sign_in: { providers: [...] }`.) `sign_in:` in `authentication { }` lists every way to check a person (`magic_link:`, provider presets), replacing `sign_in_providers:`; magic links are built in; `on_first_sign_in:` on `authentication { }` is the one decision point for every way in, external tokens included (Richard) | 2026-10-16 |
| Magic-link settings | `address:` (default the user entity's `email`, a unique `Email` field), `expires_after: 15m`, `code_attempts: 5`, `codes_per_hour: 10`, `failed_codes_per_day: 20`, `resend_after: 1m`, `same_browser: false`; loosening past 10 attempts, an hour's expiry or 50 failed codes a day needs an acceptance; needs `browser_session:` and the `email:` connection unless `send:` is set (Richard's record of 2026-10-16; the details written out for the spec) | 2026-10-16 |
| Magic-link routes | (Revised 2026-10-16: no confirm page; endpoints only.) `POST /auth/magic-link` (`address`, optional `return_to` and `locale`) answers 202 for every address; the email links to the frontend path `link:` at the browser origin with the token; `POST /auth/magic-link/confirm` takes the token or `address` and `code`, starts a browser session and answers `{ return_to }` (a path on the app's own site, default `/`); `POST /auth/magic-link/redeem` trades `address` and `code` for an API token; `path:` moves them, `generate_routes: false` leaves the app to write them over `magic_links.request`, `.redeem` and `.redeem_code` (Richard's record of 2026-10-16; the details written out for the spec) | 2026-10-16 |
| Links and codes | One email carries a 32-byte link token and a six-digit code, single-use, either working, only the latest valid, kept as verifiers with a digest of the address in `jadpo_sign_in_requests`; wrong guesses all answer `sign_in_code_invalid`; after the day's failed codes (UTC) codes stop for that address but links work; no lockouts; sends limited per address with identical answers; `same_browser:` ties the link to a cookie (Richard's record of 2026-10-16; the details written out for the spec) | 2026-10-16 |
| First sign-in | `on_first_sign_in:` names an action taking one `FirstSignIn` (`method`, `address`, `address_verified`, `subject`, `name`) and returning the user entity's `id`; any failure refuses and the person sees `sign_in_refused`; provider and token links kept in `jadpo_sign_in_links`; without it only existing users sign in (Richard's record of 2026-10-16; the details written out for the spec) | 2026-10-16 |
| Sign-in failures and events | Built-in `sign_in_link_expired`, `sign_in_link_other_browser`, `sign_in_code_invalid` (401), `sign_in_codes_disabled` (429), `sign_in_refused` (403); events `SignedIn`, `SignInRefused` (with `SignInRefusal`) and `SignInCodesDisabled`, none carrying an address, link or code (Richard's record of 2026-10-16; the details written out for the spec) | 2026-10-16 |
| OAuth grants and identity | A connection's credential may be an `OAuthGrant` owned by a record (a user or any entity), separate from sign-in and from acting as an OAuth provider; `POST /connections/<name>/start` needs a signed-in caller and keeps the `state` on the server against its session or API token, so `GET /connections/<name>/callback` needs no cookie; `POST /connections/<name>/disconnect`; connecting and disconnecting need the `update` grant on the owner's record; grants in the `jadpo_grants` system table in the owner's store, encrypted with `application { grants: { encryption_key: } }`, rotated as a `SigningKey` is; grants are held encrypted by the runtime and outlive sessions; each call names its owner with `on_behalf_of:`; erasure deletes and revokes them (Richard, area 11) | 2026-10-16 |
| Sign-in providers as connections | Google, Microsoft and other OpenID Connect providers are `Driver.oidc` connections (`issuer:`, `client_id:`, `credential:`) in `connections:`; GitHub and Apple are `Driver.github` and `Driver.apple`; `sign_in: { magic_link: { ... }  providers: [google, github] }` names them (Richard, area 11; settled for him 2026-10-16) | 2026-10-16 |
| Listing from a `GET` route (chosen in chunk 4.4c) | `run: sessions.tokens(...)` and `run: <Service>.keys(...)` are the only generated calls a route runs, each on a `GET` and passing one argument, the holder's `id` (a placeholder, `input`'s field or `current_principal.user.id`); the call is `List<IssuedToken>` or `List<IssuedKey>`, so `output:` is exactly that. On another method it is `route.run_kind`, and `issue_token`, `revoke_token`, `issue_key` and `revoke_key` are `route.run_invalid`; with the method undeclared it is `type.auth_sessions_context`, as in an action | 2026-10-16 |
| `IssuedToken` and `IssuedKey` in an `Output` (chosen in chunk 4.4c) | An `Output` field, and a route's `output:` as a `List`, may be either; on the wire each is an object of `id` (a UUID), `started_at` and `ends_at` (timestamps), never the credential. The API record gives each an `output IssuedToken` section and OpenAPI a schema of its own. They are made only by listing, so an entity can't store one (as any structured stored field, `toolchain.not_built_yet`) | 2026-10-16 |
| Policy on a listing route (chosen in chunk 4.4c) | A route that lists reaches a `read` of the holder's entity, the same grant `sessions.tokens` needs in an action: no one granted is `policy.effect_ungranted`, and a route anyone may call needs `Access.public` (`policy.public_grant_missing`). The runtime runs it as a one-statement read at repeatable read with the statement the plan limited by those grants, so it lists the tokens or keys of holders the caller may read, and a list for any other holder is `[]` with 200, never a failure. There is nothing to prove for the output: its three fields belong to the credential, whose holder's `read` already decided the list | 2026-10-16 |
| A caller held deep in a type (chosen in chunk 4.8) | An `Input`, `Output` or failure field whose type holds a caller anywhere (in a `List`, `Set`, `Map` or option, or in a field of an object or a variant's data, followed through every named type it uses) is `type.auth_principal_input`, `type.auth_principal_output` or `type.auth_principal_failure_context`, as if it held the caller itself; an `Input`, `Output` or failure it uses is checked at its own declaration | 2026-10-16 |
| Counting and undoing on a request (chosen in chunk 4.9) | A route anyone may call runs as no one, so a request to it with a credential counts against its address before it runs, as every other such request does; only a route that needs a caller counts after signing in. An action whose failure its caller handles forgets what it did to the request's session with its writes: a `sessions.start` sets no cookie and a `sessions.end` clears none, and a token or key it issued doesn't stop the response being kept. Each API's record names the methods it accepts and each route the kind of caller it selects (area 7) | 2026-10-16 |
| Signing and origin settings (chosen in chunk 5.1b) | `signing_key:` reads a `SigningKey` setting and `origin:` a `Url` setting, each written `config.<name>`; a literal, any other expression or a setting of another type is `semantic.auth_setting_invalid`, and a name no setting has is reported where it is read (area 10). With `revocation_delay:`, every built method (`browser_session:`, `api_token:`, `service_key:`) needs its `signing_key:` (`semantic.auth_setting_required`); without one a `signing_key:` is accepted and signs nothing, so the delay is switched off in one line. `origin:` is optional: given, a cookie write's `Origin` must be its scheme, host and port exactly (in lower case, without a default port or a path), so `Host` is never consulted; left out, the write's `Origin` must name the server its `Host` gives, as in chunk 4.3. A route test sends the configured origin | 2026-10-16 |
| Signed parts (chosen in chunk 5.1b) | A signed part is `<payload>.<tag>` in base64url without padding: the payload JSON of the credential's method, its row `id`, its verifier, the caller's `id` and subject, when it expires and a browser session's CSRF verifier; the tag BLAKE3 in keyed mode under a key derived (BLAKE3 `derive_key`) from the setting's text, compared in constant time, in `jadpo_runtime`, host-agnostic and built for wasm32. It is signed with the current value and opens with the current or the `_PREVIOUS` one. It is made only after a read of the record, never by `sessions.start` or `sessions.issue_token`, so a credential signs in its first time by reading the record, as it would with no delay, and a caller disabled before signing in is refused at once. It expires the delay after that read, or when the session or the key ends if that is sooner. A secret beside a part that holds another verifier, a part no key opens, one for another method or one that has expired is never refused for that: the request reads the record by its secret | 2026-10-16 |
| Renewing (chosen in chunk 5.1b) | A browser session's cookie and an API token are the 64 hexadecimal digits they are issued as, and after renewal those digits, `.` and the signed part; either form signs in with or without a delay, so turning one on or off signs no one out. A request whose credential has no good part reads the record (one statement, area 8, question 12) and its response renews it: the session's cookie set again, `HttpOnly; Secure; SameSite=Strict`, with `Max-Age` the session's time left, or the API token in an `X-Jadpo-Token` response header, which a client keeps; a client that ignores it reads the record on every request after the delay. A response whose action started or ended the session renews nothing, and neither does a refusal or a repeat answered from an `Idempotency-Key`. A `fresh` route reads the record by the credential's verifier whatever its part says, and renews it only when its part wasn't good. A write with a good part checks the CSRF token against the verifier the part holds | 2026-10-16 |
| The key exchange (chosen in chunk 5.1b) | The runtime answers `POST /auth/exchange` when `revocation_delay:` is set and `service_key:` is declared with the principal's `service:`; it signs in to every API that accepts service keys and is listed in each one's record (`generated: exchange`, `output: ExchangedToken`), OpenAPI (`x-jadpo-generated: true`) and `jadpo routes` (`generated exchange`). It takes a service key directly and nothing else: an exchanged token is 401 `invalid_credentials`, a user's credential 403 (`not_permitted`, or `csrf_rejected` for a cookie without its CSRF token), a query string or body 422 `invalid_input`; it reads no `Idempotency-Key` and keeps nothing, so a repeat is a new exchange. It answers 200 with `token`, `jadpo_service_token_` and a signed part, and `expires_at`, the delay from now or the key's end if sooner, with `Cache-Control: no-store`. An exchanged token signs in wherever the API accepts service keys, without a read except on a `fresh` route, which reads the key with the service's record by the key's verifier the part holds; it is never renewed. A route the application writes at `POST /auth/exchange` is `route.path_overlap` | 2026-10-16 |
| Passkeys (Richard) | A third built-in way to check a person (WebAuthn, FIDO2): registration and sign-in ceremonies built in on `webauthn-rs`, no attestation, credentials stored as `WebAuthnCredential`, several per person, recovery by magic link. Built in 5.5z | 2026-10-18 |
| `sign_in:` and passkeys (chosen in chunk 5.5z) | `sign_in:` is a block in `authentication { }` holding `magic_link:`, `passkey:` and `providers:`, each once (`semantic.auth_method_duplicate`); anything else is `semantic.auth_method_unknown`, and `magic_link:` and `providers:` are `toolchain.not_built_yet` until chunk 7.3b. `passkey:` takes `expires_after:` (`1m` to `10m`, default `5m`; outside it, or not a duration, `semantic.auth_setting_invalid`, a hard limit rather than an acceptance, since a longer challenge buys nothing) and `path:` (`/` and lower-case segments of letters, digits, `-` and `_`, default `/auth/passkey`). It needs `browser_session:` with `origin:` (`semantic.auth_setting_required`); the requirement of `magic_link:` waits for chunk 5.5z2. There is no `generate_routes: false` and no generated operation: the bodies are WebAuthn's, so an app's own route would add nothing. A person has at most 20 passkeys; a passkey signs in a browser session only | 2026-10-18 |
| Passkey ceremonies (chosen in chunk 5.5z) | `webauthn-rs` 0.5.5 (MPL-2.0, unmodified), its passkey ceremonies: user verification required, attestation `none`, ES256 and RS256, `credProtect` asked for as `webauthn-rs` asks. The relying-party ID is the configured origin's host and only that origin is accepted (no subdomain, no other port). Registration asks for a resident key (`residentKey: "required"`, `requireResidentKey: true`) and sign-in is discoverable (`webauthn-rs`'s `conditional-ui`, `mediation: "conditional"`), so no address is asked for and nothing tells which addresses have an account; `allowCredentials` is empty and `excludeCredentials` lists the person's passkeys. The user handle is 16 random bytes per passkey (`webauthn-rs` takes a UUID), never the user's `id`, and an answer's must match the passkey's. Its challenge is `webauthn-rs`'s own, from the operating system's generator; the cookie and the user handle are the runtime's random bytes. Its state is kept as `webauthn-rs` serialises it (`danger-allow-state-serialisation`), server-side only, and a stored passkey is loaded part by part (`danger-credential-internals`) | 2026-10-18 |
| Passkey challenges (chosen in chunk 5.5z) | `jadpo_passkey_challenges` keeps each by the BLAKE3 digest of its bytes, with its purpose, its binding, its state and when it expires, in the user entity's store. A sign-in challenge is bound to `<cookie>_passkey` (32 random bytes, kept as its digest; `HttpOnly; Secure; SameSite=Strict; Path=/`, `Max-Age` `expires_after:`, cleared on success), a registration challenge to the session row the request came with and its user; one per binding and purpose, a new request replacing it. The answer's challenge is taken with `DELETE … RETURNING` before the answer is checked, so it is used once whatever the answer; one missing, expired, bound elsewhere or for another user is `passkey_challenge_expired` (401). Expired challenges are swept when a new one is kept. The options routes take no body or query string, and every passkey POST needs an `Origin` naming the configured origin (`csrf_rejected`); the signed-in ones also need the session's CSRF token. Registering reads the record now (`fresh`) | 2026-10-18 |
| Checking an answer and the counter (chosen in chunk 5.5z) | Any failure `webauthn-rs` reports, an unreadable answer, an unknown or removed passkey or a user handle that doesn't match is `passkey_rejected` (401), never saying which. The counter is `webauthn-rs`'s strict WebAuthn check: when the stored or the answered count is above zero, one not above the stored count is refused and nothing is written; a passkey that keeps no counter sends `0` and passes. Backup eligibility may not change on a discoverable sign-in. On success the passkey's counter, backup flags and `last_used_at` are written only if its counter is still the one read (`passkey_rejected` otherwise), in the transaction that keeps the new session, which `user_disabled` (403) stops when the user's `active:` doesn't hold | 2026-10-18 |
| Keeping and showing passkeys (chosen in chunk 5.5z) | `jadpo_passkeys` holds `id`, `principal_id`, `credential_id` (`bytea`, unique), `user_handle`, `credential` (the `WebAuthnCredential` JSON: bytes in Base64, the count as text, `last_used_at` left out until used) and `registered_at`. A passkey registered already is `passkey_already_registered` (409), a 21st `passkey_limit_reached` (409). `name` is required, one line (area 19's `Line`, 1 to 200 characters; else 422 `invalid_input`). Listing, renaming and removing reach only the signed-in person's passkeys, oldest first, any other `id` `passkey_not_found` (404); `PasskeyView` is `id`, `name`, `registered_at` and `last_used_at` (or `null`). The routes go in the first API that accepts browser sessions, its record (`generated: passkey`), OpenAPI (`x-jadpo-generated: true`) and `jadpo routes` (`generated passkey …`), and their failures' wording is Jadpo's own, on with `passkey:` (area 16). Until 5.5z2 the device lists a passkey under the principal's subject | 2026-10-18 |
| Encodings of signed parts and passkeys (chosen in chunk 5.9) | A signed part's payload records the encoding it was written in as `enc` (area 19, "Changing a standard type's encoding"); a part with none, signed before encodings were recorded, is read as chunk 5.1b wrote it, and one of an encoding this release doesn't know opens nothing, so the request reads the record and is renewed, as with an expired part. A passkey is kept as `WebAuthnCredential`'s contract JSON, written and read by the codec the plan numbers in its JSON table, which holds it to the type's rules, with the encoding in `jadpo_passkeys.encoding`; one with none is read as it was kept (its count a number or, as chunk 5.5z kept it, its digits) and written in this release's encoding the next time it signs in. A passkey of an encoding this release doesn't know is `passkey_rejected` and isn't listed | 2026-10-19 |
| Magic links (chosen in chunk 7.3b) | `magic_link:` takes `address:`, `link:` (required: a path on the app's site, one `/`, no query, fragment or `\`), `expires_after:` (`1m` to `1h`), `code_attempts:` (1 to 10), `codes_per_hour:` (from 1), `failed_codes_per_day:` (1 to 50), `resend_after:` (above zero), `same_browser:` and `path:`; past a limit is `toolchain.not_built_yet` until acceptances. It needs the browser session with its `origin:`, the `email:` connection and `display_name:`. Its routes go in the first API that accepts browser sessions, beside the passkeys'; the redemption only with `api_token:`. `email:` and `send:` come in chunk 7.3b2 (below), writing the routes yourself in chunk 7.3b3 | 2026-10-20 |
| What is kept (chosen in chunk 7.3b) | `jadpo_sign_in_requests` holds one row per address, keyed by the BLAKE3 digest of the address in lower case, so the send and wrong-code limits count every way of writing it as one, with the digest of the address as written beside it; a code is confirmed only with that exact address. The link's 32 bytes and the six digits (each drawn evenly) are made as the email is sent, not when it is asked for, and kept only as verifiers: the token's BLAKE3 digest, the code's keyed by its request; the address is sealed under the token (a BLAKE3 key stream), so a confirmed link can name it. The outbox keeps only the address, the locale and the request's id, so neither it nor the table ever holds a link or a code; a delivery whose request was used, replaced or has expired sends nothing. Rows are swept once nothing in them counts | 2026-10-20 |
| Asking and confirming (chosen in chunk 7.3b) | `POST /auth/magic-link` needs no `Origin`, so the app's own client asks too; its `locale` is a supported locale's name or BCP 47 tag, else `Accept-Language` decides; a `return_to` that isn't a site path is 422 `invalid_input`. An email goes to an address an account has (active or not; confirming then answers `user_disabled`) or, with `on_first_sign_in:`, to any. The confirm needs an `Origin` naming the browser session's origin; the redemption, for the app's own client, needs none. The person is found by the address exactly as the user's field holds it. A link opened in another browser under `same_browser:` is refused and left unused; the binding cookie is `<cookie>_sign_in` and is cleared once signed in. Wrong guesses count against the code and the address's UTC day only where the address has a row; past the day's, even the right code is `sign_in_codes_disabled`, with `Retry-After` until midnight UTC | 2026-10-20 |
| First sign-in (chosen in chunk 7.3b) | `FirstSignIn` is a built-in object only the runtime makes (`method`, `address`, `address_verified`, `subject`, `name`); `SignInMethod` is a built-in enum of `magic_link`, `passkey` and `external_token`, a provider's value coming with providers. `on_first_sign_in:`'s action runs as the system (`Access.system` grants it), in its own transaction, before the session starts; a failure it declares is `sign_in_refused`, `unavailable` is `authentication_unavailable`, a fault `internal_fault`; the user it names is then read by its `id` and must be active. The reason a refusal had reaches the log and `SignInRefused` with the events (chunk 7.3b4) | 2026-10-20 |
| The app's own sign-in email and sending (chosen in chunk 7.3b2) | `email:` and `send:` name a type or an action as `on_first_sign_in:` does, `<domain>.<name>`. `SignInCode` is a built-in type made of text (six digits); like an API token it is never kept in an entity (`type.auth_credential_stored`), nor put in an `Output`, a failure or an event (`type.secret_exposed`), nor written as JSON. `MagicLinkEmail` is a built-in object only an email copies from, with `pick:` or `include:` (`type.not_record` elsewhere); the email `email:` names may hold no field of its own and is never emitted (`event.emit_invalid`), so the outbox never holds its link or code. Its delivery is still Jadpo's: the link and the code are made as it is sent, the app's email filled from them and rendered as any of its emails is, in the request's locale. `send:` keeps `display_name:` required; its action is a subscriber's of `jadpo.MagicLinkEmail`, run as the system, reaching no tenant, retried and given up under area 12's defaults, each attempt given a link and a code of its own, so only the latest works; a request used or replaced since runs nothing | 2026-10-20 |
| The app's own routes (chosen in chunk 7.3b3) | `generate_routes:` is `true` or `false`, and `false` beside `path:` is refused (`semantic.auth_setting_invalid`). The operations need `magic_link:` with `generate_routes: false` (`semantic.auth_setting_required`), so each link is asked for and used one way; only an action calls them (`type.auth_sessions_context`), and a route's `run:` never does (`route.run_invalid`). Their arguments are positional: `request` takes all three, `none` for the path meaning `/`, and a `locale` the app doesn't support falls back to its default. They run the generated routes' own code. `request` keeps the request in the action's transaction and leaves Jadpo's email as the action's own event, so it goes once the action commits, and an action that fails sends nothing and counts no send; the sweep of rows nothing counts runs on its own. `redeem` and `redeem_code` use the link or the code in a transaction of their own, before the action's writes, as the routes do: a wrong code stays counted though the action fails, and a used link or code stays used if the action fails after, so the person asks again; `on_first_sign_in:`'s action runs in its own transaction while the app's waits. An action that has asked for a link can't use one (a fault: it would wait on its own row). `same_browser:`'s cookie is read from the request; the response of the route whose action asked sets it, and the response of one whose action used a link clears it; neither response is kept for an idempotent repeat. `SignInToken` is a built-in type made of text (64 lower-case hexadecimal digits), held only by an `Input` or an `Object`: never kept (`type.auth_credential_stored`), nor put in an `Output`, a failure, an event, an email, a message or a prompt (`type.secret_exposed`), nor written as JSON. `MagicLinkRedeemed` is a built-in object only the runtime makes: `user_id`, the user entity's `id`, and `return_to: Text`, `/` unless the person gave a path. Their failures are the routes' own, raised by the runtime and passing through the action: `rate_limited` and `sign_in_codes_disabled` with `Retry-After`, and `sign_in_return_invalid` (`invalid_value`, 422), worded under the `magic_link_routes` feature of Jadpo's text. The API record and OpenAPI list the app's routes as its own, and no generated ones | 2026-10-20 |
| Sign-in events (chosen in chunk 7.3b4) | `SignedIn`, `SignInRefused` and `SignInCodesDisabled` are built-in types only the runtime emits (`event.emit_invalid`), named in `on:` as an app's events are, and reserved as names; a subscriber of one needs `magic_link:` (`subscriber.event_invalid`), and its fields are area 8's, `user_id` typed as the user entity's `id`. A test, fixture or seed may build one, or a `FirstSignIn`, with every field, to call an action directly; elsewhere `type.not_record`. Each is kept in the outbox of the store sessions are kept in, under `jadpo.<Name>`, its payload the runtime's own (the user's `id`, the way in, the reason, `until` in milliseconds), and only when a subscriber listens, so nothing records who signed in that no one asked for. `SignedIn` commits with the session or token it reports; from the app's own routes it is the action's event, delivered only if that action commits. `SignInRefused` and `SignInCodesDisabled` are kept in a transaction of their own once the refusal is decided, so they stay though the action fails, and a store that can't keep them leaves the refusal unchanged. `user_id` is given when the link's or code's address has a user (looked up only when something listens) or the user is disabled; never for an expired link or a first sign-in the app refused. A first sign-in with no `on_first_sign_in:`, or whose action named no visible user, is `refused_by_app`; a fault or an unavailable store publishes nothing. `SignInCodesDisabled` is published by the wrong code that reaches `failed_codes_per_day:`, `until` the next midnight UTC. The reason, and for `refused_by_app` the failure the action declared, goes on the request's log line (`jadpo dev`: `└─ sign-in refused · refused_by_app · SignUpClosed`; `refused` in `--json`), never in the answer. Passkeys' events come in chunk 5.5z2 | 2026-10-20 |
| Passkey recovery, events and route tests (chosen in chunk 5.5z2) | `passkey:` with no `magic_link:` written in the same `sign_in:` is `semantic.auth_setting_required`, reported only once the browser session's origin is there, so a project missing both hears about the origin first. The device lists a passkey under the user's `magic_link:` `address:` field, read by the user's `id` as the request reads the record; a user without one is listed under the subject. A passkey's sign-in publishes `SignedIn` (`method: passkey`, `first_time: false`) committed with the session, and `SignInRefused` for `passkey_challenge_expired`, `passkey_rejected` and `user_disabled` in a transaction of its own, its reason on the request's log line; `user_id` is given when the answer named a registered passkey, never for an unreadable answer, a challenge that expired or a passkey removed or never registered. A malformed body, a missing `Origin` and an unavailable store publish nothing, as for magic links. `PasskeyAdded` and `PasskeyRemoved` carry `user_id` and `passkey_id` (the routes' `id`, never the credential ID), commit with the passkey they report, and need `passkey:` for a subscriber (`subscriber.event_invalid`); they are kept under `jadpo.<Name>` only when a subscriber listens, as the sign-in events are. `test_passkeys.register(user, name)` and `.sign_in(passkey)` are checked in tests only (`semantic.request_invalid` elsewhere, as `request` is; `semantic.auth_setting_required` with no `passkey:`), `name` any value built on `Text`; each `TestPasskey` is a software authenticator of its own (ES256, discoverable, verifying, no attestation) that only `jadpo test`'s host has (the `jadpo` tool links it through the test runner; no deployable does), opaque to code. `register` signs in as `as:` does and sends the application's own `Origin`; `sign_in` starts from a browser with no cookies. A refusal on the way to a passkey ends the test as a fault naming the route and its code. A test calling only `test_passkeys` counts as the kind its other calls make it (an action test when a fixture makes its user), since the generated routes are not graph nodes. Until `email.sent` (built in chunk 7.3c) a route test couldn't read the magic link's email, so `as:` stands in for it there; the live test in `crates/jadpo-cli/tests/passkeys.rs` recovers through the link's code | 2026-10-20 |
| The system's sign-in actions and tenants (Phase 7 review, chunk 7.9b) | The magic-link `send:` action runs with its delivery's tenant, as a subscriber's does, which for `jadpo.MagicLinkEmail` is none, and `on_first_sign_in:`'s action with no tenant: each reaches only records every tenant shares, never every tenant's, as area 9 says of the system inside a request. Each is let in only by `Access.system`: what only `Access.public` lets it in to is `policy.system_grant_missing`, at its `send:` or `on_first_sign_in:` (Richard, 2026-10-21) | 2026-10-21 |
