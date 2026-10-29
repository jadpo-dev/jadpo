---
title: Identity
area: 8
status: decided
checked-with: this checkout's jadpo on every gate run
---

# Identity
<!-- topic: identity -->

Who is calling: the `application { }` and `principal { }` declarations, the
ways a caller proves who they are, how code reads the caller, service
credentials, revocation, and the failures authentication produces. Who may do
what is policy, area 9.

> Not built yet: sign-in providers, acting as an OAuth provider and OAuth
> grants. "What is built" below lists each.

Five rules hold underneath all of it:

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
<!-- topic: the-principal -->

The principal is the caller after authentication: either a **user** (a
person) or a **service** (another system). It is declared once, with no name
(area 1), and says which entity field identifies each kind and
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

- `subject:` names a unique field of a stored entity, one every tenant
  shares when the application has a tenant (area 9). Authentication looks
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
<!-- topic: the-application-and-revocation -->

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
expires and the next request renews it the same way:

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
and OpenAPI do (area 7, "Generated authentication routes in OpenAPI"):

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

`authentication.jadpo` lists the methods the application accepts. Each is one
named block from a fixed list; the method decides where the credential travels
(question 1).

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
| `external_token` | `Authorization: Bearer` JWT from a provider | users | Only the subject is used; the user is looked up on every request |
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
  applications without the method make no discovery or key requests. It
  accepts area 19's EdDSA, RS256 and ES256 JWTs for subjects the app
  already has. `issuer:` reads a `Url` setting and `audience:` a `Text`
  setting, each compared exactly, and every request reads the subject's
  current user record, its `active:` and its lifecycle. Provider profile
  and permission claims never become stored identity or policy grants.
  Multi-column and structured subject fields are `toolchain.not_built_yet`
  for this method.
- **The provider cache.** Each application start has one: at most 64
  keys, a 64 KiB discovery document and a 256 KiB JWKS, 3 seconds per
  fetch, keys kept for 5 minutes, and at most one refresh per 30 seconds,
  even when a request is cancelled. The route's deadline also cancels
  discovery and key requests. Encryption keys in the set are ignored;
  signing keys keep area 19's strict `Jwk` contract. An invalid credential
  is 401, invalid provider or configuration data
  `authentication_misconfigured` (500), and an unreachable provider
  `authentication_unavailable` (503); no response shows provider content
  or a credential.
- **Provider reachability:** the configured issuer
  and its discovered key endpoint use HTTPS, with no redirects and only
  public resolved addresses. The discovery document must name exactly
  the configured issuer, including its trailing slash. Its `jwks_uri`
  may name another public HTTPS authority; the token never supplies an
  address to fetch. Discovery appends `/.well-known/openid-configuration`
  after the issuer's path, removing a final slash for that request only
  ([OpenID Connect Discovery §§3, 4.1, 4.3](https://openid.net/specs/openid-connect-discovery-1_0.html)).
  Natively the transport filters the actual DNS answers; Workers uses the
  public network with `global_fetch_strictly_public`.
- With a revocation delay and service keys, the compiler generates
  `POST /auth/exchange`, which trades a key for a short-lived token. A service sends its key as `Authorization:
  Bearer` and nothing else, and gets `{ "token": ..., "expires_at": ... }`:
  the token is `jadpo_service_token_` and a signed part, lasting the delay
  or until the key expires if that is sooner, and signs the service in
  wherever its key would. The key is read with the service's record, so a
  disabled service is refused there. A route the application writes at
  that path is `route.path_overlap`.

## Checking the person: `sign_in:`
<!-- topic: checking-the-person-sign-in -->

> Not built yet: `providers:` is `toolchain.not_built_yet`.

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

`sign_in_providers:` is refused with its rename to `sign_in:`. Checking the
person stays out of application code: an app writes no magic-link action,
and `sessions.start` is for a person the app checks some other way (an
invitation it issued, below).

Sign-in providers are connections (area 11, "No presets"). Google,
Microsoft and other OpenID Connect providers are each
`google: { driver: Driver.oidc  issuer:  client_id:  credential: }` in
`app.jadpo`'s `connections:`, whose discovery document gives the rest;
GitHub and Apple, which are not plain OpenID Connect, are the built-in
`Driver.github` and `Driver.apple`. `sign_in:` names them in
`providers: [...]`, as `email:` names its connection.

## Magic links
<!-- topic: magic-links -->

A person gives their email address and receives one email holding a link
and a six-digit code. Either signs them in, once: the link through a page
of the app's frontend, the code typed into the app's own sign-in page or
posted by the app's own client for an API token. Jadpo generates endpoints
only, never pages: the frontend owns every page.

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
  otherwise optional, since the link starts a browser session
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

A whole application with magic links: the settings,
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
records, in its own transaction, for an address no member
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

## Passkeys
<!-- topic: passkeys -->

A passkey (WebAuthn, FIDO2) is the third built-in way to check a person,
beside magic links and providers. The person unlocks a key held on their
device with a fingerprint, their face or the device's PIN; the biometric
never leaves the device, and the app stores only a public key, so there is
no secret on the server to leak and a passkey works only on the site it
was made for. The language owns it, as it owns magic links, because the
ceremonies are where hand-written sign-in goes wrong:

- **Registering and signing in** are built in: the one-time challenge, its
  expiry, the origin and relying-party check, and the signature counter,
  checked as the well-established `webauthn-rs` crate checks them and held
  to its answers by Jadpo's tests, never written by the app. Attestation
  is not requested.
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
  help: docs/errors/semantic.auth_setting_required.md

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
  help: docs/errors/semantic.auth_setting_required.md

check failed: 1 error, 1 file
```

## A person the app has not seen
<!-- topic: a-person-the-app-has-not-seen -->

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

## Signing in from code the app writes
<!-- topic: signing-in-from-code-the-app-writes -->

`sessions.start(user_id)` and `sessions.issue_token(user_id)` stay for a
person the app checks itself, such as one accepting an invitation it sent
(see "Starting and ending a session").

## Events
<!-- topic: events -->

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

## In development and tests
<!-- topic: in-development-and-tests -->

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

## What the compiler checks of sign-in
<!-- topic: what-the-compiler-checks-of-sign-in -->

For magic links:

- `sign_in_providers:` is `semantic.auth_method_unknown`, whose edit
  renames it `sign_in:`.
- `magic_link:` without `browser_session:`, or without its `origin:`, is
  `semantic.auth_setting_required`; without the app's `email:` connection
  it is `email.connection_required`, unless it names `send:`; without `application { display_name: }`, which Jadpo's email names
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
  (area 16). `email:` naming anything but an
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
- `generate_routes:` that isn't `true` or `false`, or
  `false` beside `path:`, is `semantic.auth_setting_invalid`; calling
  `magic_links.request`, `.redeem` or `.redeem_code` with no
  `magic_link:`, or with its routes generated, is
  `semantic.auth_setting_required`; outside an action
  `type.auth_sessions_context`; as a route's `run:` itself
  `route.run_invalid`; a `SignInToken` kept in an entity is
  `type.auth_credential_stored`, and one in an `Output`, a failure, an
  event, an email, a message or a prompt `type.secret_exposed`.
- A subscriber of `SignedIn`, `SignInRefused` or
  `SignInCodesDisabled` with no `magic_link:` is
  `subscriber.event_invalid`; a field of one it doesn't have
  `type.unknown_field`; `emit` of one `event.emit_invalid`; and
  `FirstSignIn` or a sign-in event built with `{ ... }` anywhere but a
  test, fixture or seed `type.not_record`.

For passkeys:

- `passkey:` without `browser_session:`, or without its `origin:`, is
  `semantic.auth_setting_required`.
- `passkey:` without `magic_link:` in the same `sign_in:` is
  `semantic.auth_setting_required`.
- A setting `passkey:` doesn't take is `semantic.auth_setting_unknown`;
  `expires_after:` outside `1m` to `10m`, or a `path:` that isn't fixed
  segments, is `semantic.auth_setting_invalid`.
- Anything in `sign_in:` but `magic_link:`, `passkey:` and `providers:` is
  `semantic.auth_method_unknown`.
- A route the application writes at a generated passkey path is
  `route.path_overlap`.
- A subscriber of `PasskeyAdded` or `PasskeyRemoved`
  with no `passkey:` is `subscriber.event_invalid`, and `emit` of one
  `event.emit_invalid`; `test_passkeys.register` or `.sign_in` anywhere
  but a test is `semantic.request_invalid`, and with no `passkey:`
  `semantic.auth_setting_required`.

## Who is calling: `current_principal`
<!-- topic: who-is-calling-current-principal -->

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
<!-- topic: routes-signed-in-by-default -->

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

An application may have several APIs (area 7: `main`, `admin`,
`webhooks`), each with a default authentication. An API lists the methods it accepts, and a route
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
<!-- topic: service-credentials -->

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
<!-- topic: starting-and-ending-a-session -->

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
in its output, the one time anyone sees it (like Laravel Sanctum's
`createToken`):

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
keys, directly, as generated queries that read what the runtime keeps:

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
suspended member. These examples use the app-written form: signing up starts a session for a handle without checking the
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
<!-- topic: acting-for-a-user-at-another-service-oauth-grants -->

> Not built yet. Area 11's "Acting for each user: OAuth grants" holds the
> connection's side.

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
<!-- topic: identity-failures -->

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
| `magic_links.request` given a path to go back to that isn't on the app's site | `sign_in_return_invalid` | `invalid_value` | 422 |
| A passkey ceremony's challenge expired, was used, was replaced or came from another browser or session | `passkey_challenge_expired` | `unauthenticated` | 401 |
| A passkey's answer doesn't check (origin, relying party, presence, verification, signature or counter), or its passkey isn't registered | `passkey_rejected` | `unauthenticated` | 401 |
| The passkey is registered already | `passkey_already_registered` | `conflict` | 409 |
| The person has 20 passkeys already | `passkey_limit_reached` | `conflict` | 409 |
| No passkey of the signed-in person has that `id` | `passkey_not_found` | `not_found` | 404 |

`user_disabled` and `service_disabled` are built in and never declared
(question 6), as are the `sign_in_` and `passkey_` failures, whose
wording for people is Jadpo's own (area 16, "Built-in emails"). With a revocation delay, a disabled caller is
noticed within that delay. The log records the method, the credential's
record id and the reason, never the credential.

## What is built
<!-- topic: identity-built -->

A feature not built yet is `toolchain.not_built_yet` where it is written.

| Feature | Status |
|---|---|
| Everything else on this page | Built. |
| `sign_in:`'s `providers:` and `oauth_provider:`, and `on_first_sign_in:` for provider and external-token sign-in | Not built yet. An external token's subject that no user has is `sign_in_refused` (403). |
| Scope declarations and policy's scope restrictions | Not built yet. As area 9 specifies, an external method narrows scopes only when it declares the scopes it reads; undeclared provider scope or profile claims never replace application authority. |
| A setting loosened past its limit | Not built yet: waits on acceptances (area 18). |
| OAuth grants: `OAuthGrant` credentials, the connecting and disconnecting routes and `on_behalf_of:` (area 11) | Not built yet. `Driver.oidc` for Google and Microsoft sign-in comes with providers. |

## Errors

Every error this area's checks produce, with an example that triggers it.
The examples are one file each, so the application, principal and
sign-in methods sit beside the entity they name; a project keeps them in
`app.jadpo` and `authentication.jadpo`.

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
