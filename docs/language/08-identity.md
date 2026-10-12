---
title: Identity
area: 8
status: in-review
checked-with: jadpo 3ce094d0 (2026-10-12)
rules:
  - semantic.application_principal_shape
  - semantic.auth_*
  - semantic.multiple_applications
  - semantic.multiple_principals
  - semantic.principal_variant_*
  - semantic.revocation_delay_*
  - type.auth_*
---

# Identity

Who is calling: the `application { }` and `principal { }` declarations, the
ways a caller proves who they are, how code reads the caller, service
credentials, revocation, and the failures authentication produces. Who may do
what is policy, area 9.

> In review. Code outside the collapsed boxes applies the decisions from areas 1 to 5 and this page's recommendations; each recommendation names its open question. Today's compiled syntax is collapsed beneath each section that changes.

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
| `authentication { }` | `authentication.jadpo` | The ways a caller can sign in |
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
- `active:` is a condition on that entity. A caller whose record is not
  active is refused (see Failures below).
- The principal's fields are built in (question 2): `id` (a `User.id` or
  `Service.id`), `subject` and `strength` (`primary` or `multi_factor`). Code
  that needs a role, tenant or email reads it from the entity, which is where
  policy (area 9) reads it too.
- `service:` is optional; most applications only have users (question 3).

Today the principal declares its own fields, but only `subject`, a UUID id
and `authentication_strength` build; anything else fails `jadpo build`, and
the id cannot be a `User.id` (see Status).

::: today

The owner selected the unnamed `principal { }` without the application
setting on 2026-10-04; it is not built yet. Both variants are required.

```jadpo project=today file=app.jadpo
application TodoApplication {
    authentication {
        principal: Principal
        revocation {
            mode: bounded
            maximum_delay: 5m
        }
    }
}

principal Principal {
    user {
        subject: Text
        user_id: Uuid
    }
    service {
        subject: Text
        service_id: Uuid
    }
}
```

```jadpo project=today file=config.jadpo hidden
config TodoConfiguration {
    session_signing_key: Text { binding: "SESSION_SIGNING_KEY" secret: true }
    browser_origin: Url { binding: "BROWSER_ORIGIN" }
    oidc_issuer: Url { binding: "OIDC_ISSUER" }
    oidc_audience: Text { binding: "OIDC_AUDIENCE" }
}
```

```jadpo project=today file=entities.jadpo hidden
enum UserStatus { active disabled }
enum ServiceStatus { active disabled }
enum CredentialStatus { active revoked }
entity User { id: Uuid identity authentication_subject: Text unique status: UserStatus }
entity Service {
    id: Uuid identity
    owner_id: User.id references User.id as owner on_delete restrict
    name: Text unique
    status: ServiceStatus
}
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "This account is not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "This service is not available." }
```

:::

## The application and revocation

A signed-in caller can be disabled, or a session revoked, while they still hold
a valid credential. The application says how long that may take to bite.

```jadpo fragment
// app.jadpo
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

This one setting replaces today's `mode:` and `maximum_delay:` (question 4),
and leaving it out gives the strict behaviour. Three paths always
check the record whatever the setting: external OIDC tokens, service keys used
directly, and routes marked `authentication: RouteAuthentication.fresh` (below).

::: today

`revocation { mode: immediate }` or `revocation { mode: bounded
maximum_delay: 5m }` inside `application Name { authentication { } }` (see the
box above). `bounded` without a delay, `immediate` with one, and a delay of
zero are errors (`semantic.revocation_delay_*`).

:::

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
    oidc: {
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
| `oidc` | `Authorization: Bearer` JWT from a provider | users | Only the subject is used; the user is looked up on every request |
| `service_key` | `Authorization: Bearer` key | services | Shown once, stored as a verifier; can be exchanged for a short token |

- Keys come only from secret configuration (area 10); a key in source is an
  error.
- A bearer token is never accepted from the URL, and the cookie name and
  `Authorization` header can never be ordinary route input.
- OIDC adds one pinned dependency (`jose`); applications without it have
  none. The algorithms, limits and key refresh are fixed by the compiler.
- With a revocation delay and service keys, the compiler generates
  `POST /auth/exchange`, which trades a key for a short-lived token (question
  1's proposal fixes the path).

Today each method is a named **strategy** with a **transport**, named
**validators**, **claims** and a **resolution** per variant: 120 lines in the
golden todo, with the user lookup written twice.

::: today

```jadpo project=today file=browser_session.jadpo
authentication browser_session {
    transport {
        cookie: "todo_session"
    }
    validators {
        signed_user {
            mode: signed
            principal: user
            secret: config.session_signing_key
            audience: "todo-browser"
            origin: config.browser_origin
        }
    }
    resolution user {
        authority: User.authentication_subject
        active: status == UserStatus.active
        mappings {
            authentication_subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}
```

```jadpo project=today file=api_bearer.jadpo
authentication api_bearer {
    transport {
        bearer: authorization_header
    }
    validators {
        jwt_user {
            mode: jwt
            principal: user
            issuer: config.oidc_issuer
            audience: config.oidc_audience
        }
        service_key {
            mode: api_key
            principal: service
            secret: config.session_signing_key
            audience: "todo-service-key"
            owner: Service.owner_id
            credentials {
                identity: ServiceCredential.id
                principal: ServiceCredential.service_id
                verifier: ServiceCredential.verifier
                active: status == CredentialStatus.active
                expires: ServiceCredential.expires_at
                revoked: ServiceCredential.revoked_at
            }
        }
        service_signed {
            mode: signed
            principal: service
            secret: config.session_signing_key
            audience: "todo-service-token"
        }
    }
    exchange {
        path: "/auth/exchange"
        key: service_key
        signed: service_signed
    }
    resolution user {
        authority: User.authentication_subject
        active: status == UserStatus.active
        mappings {
            authentication_subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
    resolution service {
        authority: Service.name
        active: status == ServiceStatus.active
        mappings {
            name -> Principal.service.subject
            id -> Principal.service.service_id
        }
        inactive: ServiceDisabled
    }
}
```

:::

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
  put in a failure's fields (`type.auth_principal_*`).

Today there are two names: routes use `current_principal`, while actions and
queries can also read the whole principal implicitly as `principal`, which
`current_principal` is not (verified by compiling).

::: today

```jadpo project=today file=routes.jadpo
output Me {
    id: Uuid
}

query me(principal: Principal.user) -> Me {
    policy {
        Access.authenticated: [invoke]
    }
    return Me {
        id: principal.user_id
    }
}

route GET /me {
    output: Me
    run: me(current_principal.user)
}

route GET /me/checked {
    auth: fresh
    output: Me
    run: me(current_principal.user)
}

output Health {
    ok: Bool
}

query live() -> Health {
    policy {
        Access.public: [invoke]
    }
    return Health {
        ok: true
    }
}

route GET /health/live {
    auth: none
    output: Health
    run: live()
}
```

:::

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

With one API, every method is accepted. A webhook route names its
provider's signature check instead, `authentication: stripe.signature`
(area 7's proposal; the check is area 11's), and has no `current_principal`.
Question 8 spells the word out; today it is `auth: none` and `auth: fresh`
(see the box above), and every route accepts every strategy.

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

Question 7 proposes that the language stores the keys, as it already stores
browser sessions, with generated operations to issue, list and revoke them
(`Service.issue_key(service_id, expires_at)`, `Service.keys(service_id)`,
`Service.revoke_key(key_id)`). Today the application declares a
`ServiceCredential` entity and maps six of its fields in the strategy, and
keys are issued only through the trusted host integration.

::: today

The six roles are mapped in the `credentials { }` block of the `service_key`
validator above.

```jadpo project=today file=service_credential.jadpo
entity ServiceCredential {
    id: Uuid identity
    service_id: Service.id references Service.id as service on_delete cascade
    verifier: Text unique
    status: CredentialStatus
    expires_at: Instant
    revoked_at: Instant?
}
```

:::

## Starting and ending a session

Today no Jadpo code can sign someone in or out. The generated application
exposes `issue`, `refresh` and `revoke` to a trusted host program, which must
check the person itself; there is no login, refresh or logout route, and
passwords are out of scope. Rails (`start_new_session_for`,
`terminate_session`) and Phoenix (`log_in_user`) generate exactly these two
calls. Question 9 proposes the same:

```jadpo fragment
action finish_magic_link(token: MagicLinkToken) fails LinkExpired -> SignedIn {
    var user: User = attempt magic_links.redeem(token)
    sessions.start(user.id)          // sets the cookie on this route's response
    ...
}

action sign_out() -> Unit {
    sessions.end()                   // ends the session this request came with
}
```

How the person is checked (a magic link, an OIDC code exchange) stays in the
application or an integration (area 11).

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

Question 6 proposes built-in `user_disabled` and `service_disabled` in place
of today's declared `inactive: UserDisabled`, whose kind may be `NotPermitted`
(403) or `Rejected` (422). With a revocation delay, a disabled caller is
noticed within that delay. The log records the method, the credential's
record id and the reason, never the credential.

## Errors

> The examples in this section use today's syntax, because they show the compiler's real output; they are rewritten as the rework lands. Under question 1's recommendation about twenty of these rules disappear, listed in the proposals file.

Every error this area can produce, with an example that triggers it and the
compiler's actual output.

### `semantic.multiple_applications`

```jadpo project=e-multiple-applications expect=semantic.multiple_applications
application App {
    authentication {
        principal: Principal
        revocation {
            mode: immediate
        }
    }
}

application Other {
    authentication {
        principal: Principal
        revocation {
            mode: immediate
        }
    }
}

principal Principal {
    user {
        subject: Text
    }
    service {
        subject: Text
    }
}
```

### `semantic.multiple_principals`

```jadpo project=e-multiple-principals expect=semantic.multiple_principals
application App {
    authentication {
        principal: Principal
        revocation {
            mode: immediate
        }
    }
}

principal Principal {
    user {
        subject: Text
    }
    service {
        subject: Text
    }
}

principal Other {
    user {
        subject: Text
    }
    service {
        subject: Text
    }
}
```

### `semantic.application_principal_shape`

```jadpo project=e-application-principal-shape expect=semantic.application_principal_shape
application App {
    authentication {
        principal: Principal?
        revocation {
            mode: immediate
        }
    }
}

principal Principal {
    user {
        subject: Text
    }
    service {
        subject: Text
    }
}
```

### `semantic.principal_variant_duplicate`

```jadpo project=e-principal-variant-duplicate expect=semantic.principal_variant_duplicate
application App {
    authentication {
        principal: Principal
        revocation {
            mode: immediate
        }
    }
}

principal Principal {
    user {
        subject: Text
    }
    user {
        subject: Text
    }
    service {
        subject: Text
    }
}
```

### `semantic.principal_variant_required`

```jadpo project=e-principal-variant-required expect=semantic.principal_variant_required
application App {
    authentication {
        principal: Principal
        revocation {
            mode: immediate
        }
    }
}

principal Principal {
    user {
        subject: Text
    }
}
```

### `semantic.revocation_delay_required`

```jadpo project=e-revocation-delay-required expect=semantic.revocation_delay_required
application App {
    authentication {
        principal: Principal
        revocation {
            mode: bounded
        }
    }
}

principal Principal {
    user {
        subject: Text
    }
    service {
        subject: Text
    }
}
```

### `semantic.revocation_delay_forbidden`

```jadpo project=e-revocation-delay-forbidden expect=semantic.revocation_delay_forbidden
application App {
    authentication {
        principal: Principal
        revocation {
            mode: immediate
            maximum_delay: 5m
        }
    }
}

principal Principal {
    user {
        subject: Text
    }
    service {
        subject: Text
    }
}
```

### `semantic.revocation_delay_invalid`

```jadpo project=e-revocation-delay-invalid expect=semantic.revocation_delay_invalid
application App {
    authentication {
        principal: Principal
        revocation {
            mode: bounded
            maximum_delay: 0m
        }
    }
}

principal Principal {
    user {
        subject: Text
    }
    service {
        subject: Text
    }
}
```

### `semantic.auth_strategy_duplicate`

```jadpo project=e-auth-strategy-duplicate file=context.jadpo expect=semantic.auth_strategy_duplicate hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
```

```jadpo project=e-auth-strategy-duplicate expect=semantic.auth_strategy_duplicate
authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: opaque
            principal: user
        }
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}

authentication browser_session {
    transport {
        bearer: authorization_header
    }
    validators {
        user_session {
            mode: opaque
            principal: user
        }
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}
```

### `semantic.auth_credential_slot_duplicate`

```jadpo project=e-auth-credential-slot-duplicate file=context.jadpo expect=semantic.auth_credential_slot_duplicate hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
```

```jadpo project=e-auth-credential-slot-duplicate expect=semantic.auth_credential_slot_duplicate
authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: opaque
            principal: user
        }
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}

authentication second_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: opaque
            principal: user
        }
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}
```

### `semantic.auth_bearer_location_forbidden`

```jadpo project=e-auth-bearer-location-forbidden file=context.jadpo expect=semantic.auth_bearer_location_forbidden hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
```

```jadpo project=e-auth-bearer-location-forbidden expect=semantic.auth_bearer_location_forbidden
authentication browser_session {
    transport {
        bearer: query
    }
    validators {
        user_session {
            mode: opaque
            principal: user
        }
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}
```

### `semantic.auth_validator_required`

```jadpo project=e-auth-validator-required file=context.jadpo expect=semantic.auth_validator_required hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
```

```jadpo project=e-auth-validator-required expect=semantic.auth_validator_required
authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}
```

### `semantic.auth_validator_duplicate`

```jadpo project=e-auth-validator-duplicate file=context.jadpo expect=semantic.auth_validator_duplicate hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
```

```jadpo project=e-auth-validator-duplicate expect=semantic.auth_validator_duplicate
authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: opaque
            principal: user
        }
        user_session {
            mode: signed
            principal: user
        }
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}
```

### `semantic.auth_validation_mode_unknown`

```jadpo project=e-auth-validation-mode-unknown file=context.jadpo expect=semantic.auth_validation_mode_unknown hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
```

```jadpo project=e-auth-validation-mode-unknown expect=semantic.auth_validation_mode_unknown
authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: homegrown
            principal: user
        }
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}
```

### `semantic.auth_principal_variant_unknown`

```jadpo project=e-auth-principal-variant-unknown file=context.jadpo expect=semantic.auth_principal_variant_unknown hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
```

```jadpo project=e-auth-principal-variant-unknown expect=semantic.auth_principal_variant_unknown
authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: opaque
            principal: administrator
        }
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}
```

### `semantic.auth_resolution_duplicate`

```jadpo project=e-auth-resolution-duplicate file=context.jadpo expect=semantic.auth_resolution_duplicate hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
```

```jadpo project=e-auth-resolution-duplicate expect=semantic.auth_resolution_duplicate
authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: opaque
            principal: user
        }
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}
```

### `semantic.auth_resolution_authority_invalid`

```jadpo project=e-auth-resolution-authority-invalid file=context.jadpo expect=semantic.auth_resolution_authority_invalid hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
```

```jadpo project=e-auth-resolution-authority-invalid expect=semantic.auth_resolution_authority_invalid
authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: opaque
            principal: user
        }
    }
    resolution user {
        authority: User
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}
```

### `semantic.auth_mapping_required`

```jadpo project=e-auth-mapping-required file=context.jadpo expect=semantic.auth_mapping_required hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
```

```jadpo project=e-auth-mapping-required expect=semantic.auth_mapping_required
authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: opaque
            principal: user
        }
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
        }
        inactive: UserDisabled
    }
}
```

### `semantic.auth_mapping_duplicate`

```jadpo project=e-auth-mapping-duplicate file=context.jadpo expect=semantic.auth_mapping_duplicate hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
```

```jadpo project=e-auth-mapping-duplicate expect=semantic.auth_mapping_duplicate
authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: opaque
            principal: user
        }
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}
```

### `semantic.auth_mapping_target_invalid`

```jadpo project=e-auth-mapping-target-invalid file=context.jadpo expect=semantic.auth_mapping_target_invalid hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
```

```jadpo project=e-auth-mapping-target-invalid expect=semantic.auth_mapping_target_invalid
authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: opaque
            principal: user
        }
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.service.service_id
        }
        inactive: UserDisabled
    }
}
```

### `type.auth_adapter_setting`

```jadpo project=e-auth-adapter-setting file=context.jadpo expect=type.auth_adapter_setting hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
```

```jadpo project=e-auth-adapter-setting expect=type.auth_adapter_setting
authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: opaque
            principal: user
            secret: "hard-coded-key"
        }
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}
```

### `type.auth_jwt_transport`

```jadpo project=e-auth-jwt-transport file=context.jadpo expect=type.auth_jwt_transport hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
```

```jadpo project=e-auth-jwt-transport expect=type.auth_jwt_transport
authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        oidc_user {
            mode: jwt
            principal: user
        }
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}
```

### `type.auth_claim_unknown`

```jadpo project=e-auth-claim-unknown file=context.jadpo expect=type.auth_claim_unknown hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
```

```jadpo project=e-auth-claim-unknown expect=type.auth_claim_unknown
authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: opaque
            principal: user
        }
    }
    claims {
        email -> Principal.user.subject
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}
```

### `type.auth_claim_authoritative_field`

```jadpo project=e-auth-claim-authoritative-field file=context.jadpo expect=type.auth_claim_authoritative_field hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
```

```jadpo project=e-auth-claim-authoritative-field expect=type.auth_claim_authoritative_field
authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: opaque
            principal: user
        }
    }
    claims {
        subject -> Principal.user.user_id
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}
```

### `type.auth_authority_not_persistent`

```jadpo project=e-auth-authority-not-persistent file=context.jadpo expect=type.auth_authority_not_persistent hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
entity Visitor { id: Uuid subject: Text enabled: Bool identity: id }
```

```jadpo project=e-auth-authority-not-persistent expect=type.auth_authority_not_persistent
authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: opaque
            principal: user
        }
    }
    resolution user {
        authority: Visitor.id
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}
```

### `type.auth_authority_not_unique`

```jadpo project=e-auth-authority-not-unique file=context.jadpo expect=type.auth_authority_not_unique hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool nickname: Text }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
```

```jadpo project=e-auth-authority-not-unique expect=type.auth_authority_not_unique
authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: opaque
            principal: user
        }
    }
    resolution user {
        authority: User.nickname
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}
```

### `type.auth_mapping_mismatch`

```jadpo project=e-auth-mapping-mismatch file=context.jadpo expect=type.auth_mapping_mismatch hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
```

```jadpo project=e-auth-mapping-mismatch expect=type.auth_mapping_mismatch
authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: opaque
            principal: user
        }
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            subject -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}
```

### `type.auth_resolution_incomplete`

```jadpo project=e-auth-resolution-incomplete file=context.jadpo expect=type.auth_resolution_incomplete hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
```

```jadpo project=e-auth-resolution-incomplete expect=type.auth_resolution_incomplete
authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: opaque
            principal: user
        }
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
        }
        inactive: UserDisabled
    }
}
```

### `type.auth_resolution_required`

```jadpo project=e-auth-resolution-required file=context.jadpo expect=type.auth_resolution_required hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
```

```jadpo project=e-auth-resolution-required expect=type.auth_resolution_required
authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: opaque
            principal: user
        }
        service_session {
            mode: opaque
            principal: service
        }
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}
```

### `type.auth_inactive_failure_kind`

```jadpo project=e-auth-inactive-failure-kind file=context.jadpo expect=type.auth_inactive_failure_kind hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
```

```jadpo project=e-auth-inactive-failure-kind expect=type.auth_inactive_failure_kind
authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: opaque
            principal: user
        }
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserMissing
    }
}

failure UserMissing {
    kind: NotFound
    code: "user_missing"
    message: "User not found."
}
```

### `type.auth_lifecycle_active_field`

```jadpo project=e-auth-lifecycle-active-field file=context.jadpo expect=type.auth_lifecycle_active_field hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
enum UserStatus { active disabled }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
```

```jadpo project=e-auth-lifecycle-active-field expect=type.auth_lifecycle_active_field
entity User {
    id: Uuid
    subject: Text
    enabled: Bool
    status: UserStatus

    identity: id

    persistence {
        store: primary
        role: authority
        unique: subject
    }

    lifecycle {
        initial: {
            status: User.status(UserStatus.active)
        }
        visible when status == UserStatus.active
        transition disable {
            from: status == UserStatus.active
            set: {
                status: User.status(UserStatus.disabled)
            }
        }
    }
}

authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: opaque
            principal: user
        }
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}
```

### `type.auth_lifecycle_mapping_scope`

```jadpo project=e-auth-lifecycle-mapping-scope file=context.jadpo expect=type.auth_lifecycle_mapping_scope hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
enum UserStatus { active disabled }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
```

```jadpo project=e-auth-lifecycle-mapping-scope expect=type.auth_lifecycle_mapping_scope
entity User {
    id: Uuid
    subject: Text
    enabled: Bool
    handle: Text
    status: UserStatus

    identity: id

    persistence {
        store: primary
        role: authority
        unique: subject
        unique: handle
    }

    lifecycle {
        initial: {
            status: User.status(UserStatus.active)
        }
        visible when status == UserStatus.active
        transition disable {
            from: status == UserStatus.active
            set: {
                status: User.status(UserStatus.disabled)
            }
        }
    }
}

authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: opaque
            principal: user
        }
    }
    resolution user {
        authority: User.subject
        active: status == UserStatus.active
        mappings {
            handle -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}
```

### `type.auth_exchange_binding`

```jadpo project=e-auth-exchange-binding file=context.jadpo expect=type.auth_exchange_binding hidden
application TodoApplication {
    authentication { principal: Principal revocation { mode: bounded maximum_delay: 5m } }
}
config TodoConfiguration {
    signing_key: Text { binding: "SESSION_SIGNING_KEY" secret: true }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
enum UserStatus { active disabled }
enum ServiceStatus { active disabled }
enum CredentialStatus { active revoked }
entity User {
    id: Uuid identity
    name: Text unique
    status: UserStatus
}
entity Service {
    id: Uuid identity
    owner_id: User.id references User.id as owner on_delete restrict
    name: Text unique
    status: ServiceStatus
}
entity ServiceCredential {
    id: Uuid identity
    service_id: Service.id references Service.id as service on_delete cascade
    verifier: Text unique
    status: CredentialStatus
    expires_at: Instant
    revoked_at: Instant?
}
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" }
```

```jadpo project=e-auth-exchange-binding expect=type.auth_exchange_binding
authentication api_bearer {
    transport { bearer: authorization_header }
    validators {
        service_key {
            mode: api_key
            principal: service
            secret: config.signing_key
            audience: "todo-service-key"
            owner: Service.owner_id
            credentials {
                identity: ServiceCredential.id
                principal: ServiceCredential.service_id
                verifier: ServiceCredential.verifier
                active: status == CredentialStatus.active
                expires: ServiceCredential.expires_at
                revoked: ServiceCredential.revoked_at
            }
        }
        service_signed {
            mode: signed
            principal: service
            secret: config.signing_key
            audience: "todo-service-token"
        }
    }
    exchange {
        path: "/auth/exchange"
        key: service_key
        signed: service_key
    }
    resolution service {
        authority: Service.name
        active: status == ServiceStatus.active
        mappings { name -> Principal.service.subject id -> Principal.service.service_id }
        inactive: ServiceDisabled
    }
}
```

### `type.auth_exchange_path`

```jadpo project=e-auth-exchange-path file=context.jadpo expect=type.auth_exchange_path hidden
application TodoApplication {
    authentication { principal: Principal revocation { mode: bounded maximum_delay: 5m } }
}
config TodoConfiguration {
    signing_key: Text { binding: "SESSION_SIGNING_KEY" secret: true }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
enum UserStatus { active disabled }
enum ServiceStatus { active disabled }
enum CredentialStatus { active revoked }
entity User {
    id: Uuid identity
    name: Text unique
    status: UserStatus
}
entity Service {
    id: Uuid identity
    owner_id: User.id references User.id as owner on_delete restrict
    name: Text unique
    status: ServiceStatus
}
entity ServiceCredential {
    id: Uuid identity
    service_id: Service.id references Service.id as service on_delete cascade
    verifier: Text unique
    status: CredentialStatus
    expires_at: Instant
    revoked_at: Instant?
}
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" }
```

```jadpo project=e-auth-exchange-path expect=type.auth_exchange_path
authentication api_bearer {
    transport { bearer: authorization_header }
    validators {
        service_key {
            mode: api_key
            principal: service
            secret: config.signing_key
            audience: "todo-service-key"
            owner: Service.owner_id
            credentials {
                identity: ServiceCredential.id
                principal: ServiceCredential.service_id
                verifier: ServiceCredential.verifier
                active: status == CredentialStatus.active
                expires: ServiceCredential.expires_at
                revoked: ServiceCredential.revoked_at
            }
        }
        service_signed {
            mode: signed
            principal: service
            secret: config.signing_key
            audience: "todo-service-token"
        }
    }
    exchange {
        path: "auth/exchange?key=raw"
        key: service_key
        signed: service_signed
    }
    resolution service {
        authority: Service.name
        active: status == ServiceStatus.active
        mappings { name -> Principal.service.subject id -> Principal.service.service_id }
        inactive: ServiceDisabled
    }
}
```

### `type.auth_exchange_route_collision`

```jadpo project=e-auth-exchange-route-collision file=context.jadpo expect=type.auth_exchange_route_collision hidden
application TodoApplication {
    authentication { principal: Principal revocation { mode: bounded maximum_delay: 5m } }
}
config TodoConfiguration {
    signing_key: Text { binding: "SESSION_SIGNING_KEY" secret: true }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
enum UserStatus { active disabled }
enum ServiceStatus { active disabled }
enum CredentialStatus { active revoked }
entity User {
    id: Uuid identity
    name: Text unique
    status: UserStatus
}
entity Service {
    id: Uuid identity
    owner_id: User.id references User.id as owner on_delete restrict
    name: Text unique
    status: ServiceStatus
}
entity ServiceCredential {
    id: Uuid identity
    service_id: Service.id references Service.id as service on_delete cascade
    verifier: Text unique
    status: CredentialStatus
    expires_at: Instant
    revoked_at: Instant?
}
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" }
```

```jadpo project=e-auth-exchange-route-collision expect=type.auth_exchange_route_collision
authentication api_bearer {
    transport { bearer: authorization_header }
    validators {
        service_key {
            mode: api_key
            principal: service
            secret: config.signing_key
            audience: "todo-service-key"
            owner: Service.owner_id
            credentials {
                identity: ServiceCredential.id
                principal: ServiceCredential.service_id
                verifier: ServiceCredential.verifier
                active: status == CredentialStatus.active
                expires: ServiceCredential.expires_at
                revoked: ServiceCredential.revoked_at
            }
        }
        service_signed {
            mode: signed
            principal: service
            secret: config.signing_key
            audience: "todo-service-token"
        }
    }
    exchange {
        path: "/auth/exchange"
        key: service_key
        signed: service_signed
    }
    resolution service {
        authority: Service.name
        active: status == ServiceStatus.active
        mappings { name -> Principal.service.subject id -> Principal.service.service_id }
        inactive: ServiceDisabled
    }
}

type Health = Text {}

function alive() -> Health {
    return Health("alive")
}

route POST /auth/exchange {
    auth: none
    output: Health
    run: alive()
}
```

### `type.auth_principal_output`

```jadpo project=e-auth-principal-output file=context.jadpo expect=type.auth_principal_output hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: opaque
            principal: user
        }
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}
```

```jadpo project=e-auth-principal-output expect=type.auth_principal_output
output Leak {
    actor: Principal.user
}
```

### `type.auth_principal_input`

```jadpo project=e-auth-principal-input file=context.jadpo expect=type.auth_principal_input hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: opaque
            principal: user
        }
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}
```

```jadpo project=e-auth-principal-input expect=type.auth_principal_input
input Spoof {
    actor: Principal.user
}

output Done {
    ok: Bool
}

query echo(actor: Principal.user) -> Done {
    policy {
        Access.authenticated: [invoke]
    }
    return Done {
        ok: true
    }
}

route POST /echo {
    input: Spoof
    output: Done
    run: echo(input.actor)
}
```

### `type.auth_principal_failure_context`

```jadpo project=e-auth-principal-failure-context file=context.jadpo expect=type.auth_principal_failure_context hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: opaque
            principal: user
        }
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}
```

```jadpo project=e-auth-principal-failure-context expect=type.auth_principal_failure_context
failure Leak {
    kind: Rejected
    code: "leak"
    internal {
        actor: Principal.user
    }
}
```

### `type.auth_reserved_route_input`

```jadpo project=e-auth-reserved-route-input file=context.jadpo expect=type.auth_reserved_route_input hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: opaque
            principal: user
        }
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}
```

```jadpo project=e-auth-reserved-route-input expect=type.auth_reserved_route_input
input Note {
    session: Text
}

output Done {
    ok: Bool
}

query save(note: Note) -> Done {
    policy {
        Access.authenticated: [invoke]
    }
    return Done {
        ok: true
    }
}

route POST /notes {
    input: Note
    output: Done
    run: save(input)
}
```

### `type.auth_public_principal`

```jadpo project=e-auth-public-principal file=context.jadpo expect=type.auth_public_principal hidden
application App {
    authentication { principal: Principal revocation { mode: immediate } }
}
principal Principal {
    user { subject: Text user_id: Uuid }
    service { subject: Text service_id: Uuid }
}
entity User { id: Uuid identity subject: Text unique enabled: Bool }
entity Service { id: Uuid identity name: Text unique enabled: Bool }
failure UserDisabled { kind: NotPermitted code: "user_disabled" message: "Not available." }
failure ServiceDisabled { kind: NotPermitted code: "service_disabled" message: "Not available." }
config AuthConfig { signing_key: Text { binding: "SIGNING_KEY" secret: true } }
authentication browser_session {
    transport {
        cookie: "session"
    }
    validators {
        user_session {
            mode: opaque
            principal: user
        }
    }
    resolution user {
        authority: User.subject
        active: enabled == true
        mappings {
            subject -> Principal.user.subject
            id -> Principal.user.user_id
        }
        inactive: UserDisabled
    }
}
```

```jadpo project=e-auth-public-principal expect=type.auth_public_principal
output Me {
    id: Uuid
}

query me(principal: Principal.user) -> Me {
    policy {
        Access.public: [invoke]
    }
    return Me {
        id: principal.user_id
    }
}

route GET /me {
    auth: none
    output: Me
    run: me(current_principal.user)
}
```

## Review

### Status

Each row says how it was checked: compiled with `jadpo check` or `jadpo
build`, or read from generated code. Runtime behaviour is from the AUTH-001
evidence (24 first-party cases on SQLite and PostgreSQL, plus service and JWT
suites), not rerun for this page.

| Feature | Status |
|---|---|
| Application and principal | Works: one of each, both variants required, revocation delay rules enforced (compiled). |
| Principal fields | **Gap (compiled):** a field beyond `subject`, a UUID id and `authentication_strength` passes `jadpo check` and fails `jadpo build` with `toolchain.target_auth_not_implemented`. **Bug (compiled):** a principal field typed `User.id` mapped from `User.id` is `type.auth_mapping_mismatch`, so code converts UUIDs by hand. |
| Missing authentication | **Gap (compiled):** a project with protected routes and no `application` or strategy passes `jadpo check`; only `jadpo build` refuses it. |
| Strategies | Browser cookie, opaque and signed bearer, JWT, service key and exchange all compile and generate (compiled the golden migration's shapes). |
| Caller access | **Inconsistency (compiled):** `principal` is an implicit value in actions and queries; routes use `current_principal`, which is undefined inside a query. |
| Lifecycle checks | `type.auth_lifecycle_*` fire only when the user entity uses the long `identity: id` and `persistence { }` form. **Bug (compiled), area 5:** on an entity written `id: Uuid identity`, a `lifecycle { }` block is silently ignored, even with an unknown field in it. |
| Failure codes | **Bug (read from generated code):** `authentication_misconfigured` answers 500 from the selector but 503 from the cookie and JWT adapters. |
| Login | None in Jadpo: sessions start only through the trusted host's `issue` (by design in AUTH-001). |

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
| Generated `POST /auth/exchange` in the route inventory and OpenAPI, and the security schemes per route | 7 | Input to area 7 |
| Each API's accepted methods (`authentication: [AuthenticationMethod.browser_session, AuthenticationMethod.api_token]` in area 7's `apis: { }`); a route only narrows them | 7 | Input to area 7 |
| Webhook signature checks as a route's authentication (`authentication: stripe.signature`) | 11 | Input to area 11 |
| Policy reads the caller and the entity named by `subject:`; `Access.public` versus `authentication: none`; custom roles and tenants come from entities, not the principal | 9 | Input to area 9 |
| Which callers may issue, list and revoke service keys | 9 | Depends on question 7 |
| Signing keys, OIDC issuer and audience as secret and plain configuration; key rotation | 10 | Input to area 10 |
| Verifying a person before `sessions.start` (magic links, OIDC code exchange) as integrations | 11 | Depends on question 9 |
| Schedules and subscribers run with no caller | 12 | Input to area 12 |
| Signing in as a user or service in tests | 13 | Input to area 13 |
| `toolchain.target_auth_not_implemented`, raised only by `jadpo build`; moving the "protected routes but no authentication" check into `jadpo check` | 14 | Input to area 14 |
| A `lifecycle { }` block on an entity written `id: Uuid identity` is silently ignored | 5 | Raised |

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
   today's extra flexibility does not build anyway.
2. **Are the principal's fields built in?** Today they are declared, but only
   `subject`, a UUID id and `authentication_strength` build. *Recommendation:*
   yes: `id` (typed `User.id` or `Service.id`), `subject` and `strength`.
   Anything else is read from the entity, which keeps credentials free of
   profile and permission data (an AUTH-001 rule) and fixes the UUID
   conversions.
3. **Is the service variant optional?** Today a principal without `service`
   is `semantic.principal_variant_required`, so every application declares
   one. *Recommendation:* yes, at least one variant; with no `service`,
   `service_key` is an error and `current_principal` is simply a user.
4. **One revocation setting?** Today `revocation { mode: bounded
   maximum_delay: 5m }` or `{ mode: immediate }`. *Recommendation:*
   `revocation_delay: 5m` in `application { }`, left out for immediate. The
   strict choice is the default, as with authentication on routes, and two of
   the three revocation errors disappear. Clerk states its bound the same
   way, as a 60-second token lifetime.
5. **How does code read the caller?** Today routes use `current_principal`
   and callables can also read an implicit `principal`. *Recommendation:*
   `current_principal` in routes only, passed to actions as a typed parameter.
   Signatures then show which actions need a caller, tests pass one in, and
   schedules visibly have none. Policy (area 9) gets the caller from the
   runtime without code passing it.
6. **Declared or built-in failures for a disabled caller?** Today each
   resolution names a declared failure of kind `NotPermitted` or `Rejected`.
   *Recommendation:* built-in `user_disabled` and `service_disabled`, kind
   `not_permitted` (403), with no declaration. Every application declares the
   same two today, and it removes the clash between area 1's
   `authentication.jadpo` and area 4's `failures/` folders.
7. **Who stores service keys?** Today the application declares a
   `ServiceCredential` entity and maps six of its fields; sessions are already
   compiler-owned. *Recommendation:* the language stores keys, as Laravel
   Sanctum owns `personal_access_tokens`, and generates `Service.issue_key`,
   `Service.keys` and `Service.revoke_key` so an application can build its own
   key management screens. The application declares only `Service`.
8. **What does a route say about authentication?** Today `auth: none` or
   `auth: fresh`, and every route accepts every method. *Recommendation:*
   spell it `authentication: RouteAuthentication.none` and
   `authentication: RouteAuthentication.fresh` (syntax review
   #41), the one word a reviewer must not miss on a public route; each of area
   7's APIs lists the methods it accepts, so an admin API can refuse service
   keys without per-route lines.
9. **Can Jadpo code start and end a session?** Today only a trusted host
   program can. *Recommendation:* yes: `sessions.start(user_id)` and
   `sessions.end()`, callable only from actions run by a route, with the
   compiler setting and clearing the cookie and CSRF token. Passwords stay out;
   checking the person is the application's job. Without this, every
   application needs a second program just to log people in.
10. **How do tests, fixtures and seeds name a caller?** (carried from area
    13) Area 13 builds callers with `Principal.user(user)` and signs in with
    `as:`, but on this page a caller only comes from signing in.
    *Recommendation:* keep `Principal.user(user)` test-only (tests, fixtures,
    seeds), and describe it with `as:` alongside how the caller is read.

### Decisions

| Topic | Decision | Date |
|---|---|---|
| Unnamed singletons | Carried from area 1 (and the owner's 2026-10-04 selection): `application { }` and `principal { }` carry no name; the application's principal setting is removed | 2026-10-12 |
| Root files | Carried from area 1: `app.jadpo` holds the application and principal; `authentication.jadpo` holds the methods | 2026-10-12 |
| Colon rule | Carried from area 3: every member is `name: value`, so `user: { }`, `browser_session: { }` | 2026-10-12 |
| Setting options | Carried from area 2: closed options are lower_snake_case names, never strings (Superseded 2026-10-12: options are written `Type.value`; see area 2 question 17.) | 2026-10-12 |
| Runtime kinds | Carried from area 4: `unauthenticated` and `misconfigured` are raised only by the runtime; authentication failures are never listed in `fails` | 2026-10-12 |
| Mapping form | Carried from area 4: `condition: Failure` everywhere, as in `inactive: UserDisabled` | 2026-10-12 |
| Principal in failures | Carried from area 4: the signed-in principal cannot be put in a failure's fields | 2026-10-12 |
| How simple authentication is | | |
| Principal fields | | |
| Service variant | | |
| Revocation setting | | |
| Reading the caller | | |
| Disabled caller failures | | |
| Service key storage | | |
| Route setting spelling | | |
| Starting and ending sessions | | |
| Callers in tests, fixtures and seeds | | |
