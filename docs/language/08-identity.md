---
title: Identity
area: 8
status: decided
checked-with: jadpo chunk-4.3 (2026-10-16)
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

> Decided on 2026-10-12. Code on this page applies the decisions from areas 1 to 8. Built in the new compiler in roadmap chunk 4.3: the principal, browser sessions, routes signed in by default, `current_principal`, starting and ending a session, and signing in from a route test. API tokens and service keys come with 4.3b, the revocation delay with signing keys (5.1b); "Status" below lists the rest.

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

This is the only revocation setting (question 4), and leaving it out gives
the strict behaviour. Three paths always
check the record whatever the setting: external OIDC tokens, service keys used
directly, and routes marked `authentication: RouteAuthentication.fresh` (below).

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

- Keys come only from secret configuration (area 10); a key in source is an
  error. Until configuration is built (roadmap chunk 5.1), `origin:` and
  `signing_key:` are `toolchain.not_built_yet`: a session is an opaque value
  looked up in the database, which needs no key, and a cookie write's
  `Origin` must name the server its `Host` header gives.
- `browser_session:` names its cookie (`cookie:`, 1 to 64 letters, digits,
  `_` and `-`). The session's token travels in it, `HttpOnly`, `Secure`,
  `SameSite=Strict`, for the whole site; its CSRF token in a second cookie
  of the same name with `_csrf` after it, which the application's pages
  read and send back in `X-Jadpo-CSRF` on every write. A session lasts 30
  days from when it starts.
- A bearer token is never accepted from the URL, and the cookie name and
  `Authorization` header can never be ordinary route input.
- `external_token` adds one pinned dependency; applications without it
  have none. The algorithms, limits and key refresh are fixed by the compiler.
- With a revocation delay and service keys, the compiler generates
  `POST /auth/exchange`, which trades a key for a short-lived token (question
  1's proposal fixes the path).

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

## Starting and ending a session

An action run by a route starts or ends a session, and the compiler sets and
clears the cookie and CSRF token (question 9). Passwords are out of scope.
Rails (`start_new_session_for`, `terminate_session`) and Phoenix
(`log_in_user`) generate the same two calls:

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

- `sessions.start` keeps the session in the action's own transaction, in
  the store the principal's user entity lives in (a `jadpo_sessions` system
  table, area 5), so an action that fails starts none; the route's response
  sets its cookies once the action commits. `sessions.end` deletes the
  session the request came with, and the response clears the cookies.
- Only an action calls them (`type.auth_sessions_context`); a test reaches
  them through the route, or calls the action, which keeps the session
  without a response to set it on.
- Only verifiers are kept: a session's token and its CSRF token are 32
  random bytes each, stored as their digests, so a copy of the database
  signs no one in.

### Signing in, end to end

A whole application that signs members up, keeps their notes and refuses a
suspended member. The principal and its one sign-in method:



```jadpo check project=sign-in file=app.jadpo
use notes/entities { Member }
use notes/types { MemberStatus }

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

```jadpo check project=sign-in file=domains/notes/types/handle.jadpo hidden
type Handle = Text {
    min_length: 3
    max_length: 40
}
```

```jadpo check project=sign-in file=domains/notes/types/member_status.jadpo hidden
type MemberStatus = Enum {
    active
    suspended
}
```

```jadpo check project=sign-in file=domains/notes/types/note_text.jadpo hidden
type NoteText = Text {
    min_length: 1
    max_length: 500
}
```

```jadpo check project=sign-in file=domains/notes/types/note_view.jadpo hidden
use notes/types { NoteText }

type NoteView = Output {
    text: NoteText
}
```

```jadpo check project=sign-in file=domains/notes/entities/member.jadpo hidden
use notes/types { Handle, MemberStatus }

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
    status: MemberStatus
}
```

```jadpo check project=sign-in file=domains/notes/entities/note.jadpo hidden
use notes/entities { Member }
use notes/types { NoteText }

entity Note {
    id: Uuid
    author_id: Member.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText
}
```

```jadpo check project=sign-in file=domains/notes/failures/handle_taken.jadpo hidden
type HandleTaken = Failure {
    kind: FailureKind.conflict
    message: "That handle is taken."
}
```

```jadpo check project=sign-in file=domains/notes/actions/register.jadpo hidden
use notes/entities { Member }
use notes/types { Handle, MemberStatus }
use notes/failures { HandleTaken }

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

```jadpo check project=sign-in file=domains/notes/actions/suspend.jadpo hidden
use notes/entities { Member, MemberNotFound }
use notes/types { MemberStatus }

action suspend(member_id: Member.id) fails MemberNotFound -> Member {
    return attempt Member.update(member_id) {
        set: {
            status: MemberStatus.suspended
        }
    }
}
```

Signing up checks the handle, starts a session and answers with no content; its response sets the cookies. Writing a note takes the signed-in member as a parameter:

```jadpo check project=sign-in file=domains/notes/actions/sign_up.jadpo
use notes/entities { Member }
use notes/types { Handle }
use notes/failures { HandleTaken }
use notes/actions { register }

/// Registers a member and signs them in.
action sign_up(input: SignUp) fails HandleTaken -> Unit {
    var member: Member = attempt register(input.handle)
    sessions.start(member.id)
}

/// What signing up takes.
type SignUp = Input {
    handle: Handle
}
```

```jadpo check project=sign-in file=domains/notes/actions/sign_out.jadpo
/// Ends the session the request came with.
action sign_out() -> Unit {
    sessions.end()
}
```

```jadpo check project=sign-in file=domains/notes/actions/write_note.jadpo
use notes/entities { Note }
use notes/types { NoteText, NoteView }

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

/// What writing a note takes.
type WriteNote = Input {
    text: NoteText
}
```

Signing up is the one route anyone may call; every other needs the session:

```jadpo check project=sign-in file=domains/notes/routes/account.jadpo
use notes/actions { SignUp, sign_out, sign_up }

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

```jadpo check project=sign-in file=domains/notes/routes/notes.jadpo migrate=initial
use notes/types { NoteView }
use notes/actions { WriteNote, write_note }

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

```jadpo check project=sign-in file=api/main.api hidden
// Written by `jadpo api update` from the routes; never edit it by hand.
// `jadpo check` fails while it disagrees with them.
version: 1

POST /account/sign-out
    status: 204

POST /account/sign-up
    authentication: none
    input: SignUp (body)
    status: 204
    fails: handle_taken 409

POST /notes
    input: WriteNote (body)
    output: NoteView
    status: 201

output NoteView
    text: Text { min_length: 1, max_length: 500 }

input SignUp
    handle: Text { min_length: 3, max_length: 40 }

input WriteNote
    text: Text { min_length: 1, max_length: 500 }

failure handle_taken 409 "That handle is taken."
```

A route test signs in with `as:` through the real browser session (a session kept for the member, its cookie, its CSRF token and an `Origin`), so a suspended member is refused exactly as a browser would be:

```jadpo check project=sign-in file=domains/notes/routes/notes.test.jadpo
use notes/entities { Member }
use notes/actions { WriteNote, register, suspend }

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

`user_disabled` and `service_disabled` are built in and never declared
(question 6). With a revocation delay, a disabled caller is
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
    var text: Memo = input.text
    assert text != ""
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
  reason: `sessions.end` ends the session of the request a route answers, so only an action, which a route runs, calls it.
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

## Review

### Status

Built in chunk 4.3. `jadpo check` reads `application { }`'s
`revocation_delay:`, `principal { }` and `authentication { }` (each declared
once), with each variant's `subject:` and `active:`, and `browser_session:`
with its cookie; routes need a signed-in caller unless they say
`authentication: RouteAuthentication.none` (or `.fresh`), an API lists the
methods it accepts, and protected routes with nothing to sign anyone in are
`semantic.auth_not_set_up`. A route passes `current_principal`, or the kind
it selects, to what it runs, and actions, queries and functions take
`Principal.user` parameters whose `id`, `subject` and `strength` they read;
a caller never sits in an `Input`, an `Output` or a failure. Tests,
fixtures and seeds make a caller with `Principal.user(record)`, and a route
test signs in with `as:`. The runtime authenticates a request before its
input is read, refuses it as "Failures" lists, checks a cookie write's
`Origin` and CSRF token, starts and ends sessions from actions, and keeps
sessions in the `jadpo_sessions` system table beside the principal's
users. OpenAPI shows each API's security scheme, the API record marks the
routes anyone may call, and `jadpo routes` lists each route's
authentication.

Not yet, each `toolchain.not_built_yet` where it is written:

- `api_token` and `service_key`, with `Service.issue_key`, `Service.keys`,
  `Service.revoke_key`, `service_disabled` and signing in as a service in a
  route test (roadmap chunk 4.3b). How an application issues an API token
  is open (below).
- `revocation_delay:`, signed sessions and tokens, `POST /auth/exchange`,
  and `origin:` and `signing_key:` from configuration (chunk 5.1b, after
  configuration in 5.1). Until then every request checks the caller's
  record, which `RouteAuthentication.fresh` asks for anyway.
- `external_token`, `sign_in_providers:` and `oauth_provider:`, after the
  first build.
- Deleting ended sessions from the runtime's minute tick (area 12, chunk
  6.2): a lookup ignores a session past its 30 days.

**Needs Richard: how is an API token issued?** The page says `api_token`
travels as `Authorization: Bearer` for the application's own mobile and
command-line clients, but not how a client gets one: `sessions.start` sets a
cookie, and nothing hands a token to a client. *Recommendation:* a
generated `sessions.issue_token(user_id) -> ApiToken`, called by an action
a sign-in route runs and returned in its output once (shown once, stored as
a verifier, like `Service.issue_key` and Laravel Sanctum's `createToken`),
with `sessions.end()` ending the token a request came with, and
`sessions.tokens(user_id)` and `sessions.revoke_token(token_id)` beside
`Service.keys` and `Service.revoke_key` for an application's own "signed-in
devices" screen. Chunk 4.3b builds API tokens once this is decided.

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
| `toolchain.target_auth_not_implemented`, raised only by `jadpo build`; moving the "protected routes but no authentication" check into `jadpo check` | 14 | Built in chunk 4.3: `semantic.auth_not_set_up` |
| A `lifecycle { }` block on an entity written `id: Uuid identity` is silently ignored | 5 | Raised |
| Sessions in the principal's user store, rather than a `sessions: { store: }` setting (chosen in chunk 4.3, below) | 5 | Recorded on area 5's system tables |
| Deleting ended sessions from the minute tick | 12 | Chunk 6.2 |

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
| Sign-in providers | Built-in `sign_in_providers:` with presets and generic OpenID Connect; generated start and callback routes; linked accounts stored by the language; `on_first_sign_in:` action decides create or link, email linking only when verified; `oidc` renamed `external_token`; acting as an OAuth provider built in (question 13) | 2026-10-12 |
| Caching authentication | Never: authentication always reads the database; `revocation_delay` is the speed setting | 2026-10-12 |
| Acting as an OAuth provider | Built in, no third-party server: OAuth 2.1 and OpenID Connect endpoints generated; authorization code with PKCE, rotating refresh tokens, client credentials; clients stored by the language (`OAuthClient.register`, `.list`, `.revoke`); consent screen is an application route calling `oauth.approve`; `AuthenticationMethod.oauth_token`; scopes from any token narrow policy; built after the golden todo (Richard) | 2026-10-12 |
| Declarations (chosen in chunk 4.3) | `application`, `principal` and `authentication` are each declared once (`semantic.multiple_*`), in any file of a single-file project and in `app.jadpo` and `authentication.jadpo` otherwise. A variant holds `subject:`, a required `unique: true` field of an entity kept in a PostgreSQL store, and an optional `active:`; left out, every record of the entity is active. `active:` is a condition on that entity's own fields named as a query names them (`Member.status == MemberStatus.active`): comparisons with literals, enum values, `none` and `clock.now`, joined with `and` and `or`. A record the entity's lifecycle hides can't sign in. `oidc:` is refused with its rename to `external_token:` | 2026-10-16 |
| Browser sessions (chosen in chunk 4.3) | `browser_session:` requires `cookie:` (1 to 64 letters, digits, `_` and `-`, starting with a letter). The token is 32 random bytes in hexadecimal in that cookie (`HttpOnly; Secure; SameSite=Strict; Path=/`), the CSRF token the same in `<cookie>_csrf`, readable by the application's scripts; both are kept only as BLAKE3 verifiers. A session lasts 30 days from its start, with no sliding renewal. Every method but `GET` that comes with the cookie needs an `Origin` whose host is the request's `Host` (until configuration gives `origin:`) and `X-Jadpo-CSRF` matching the session, else 403 `csrf_rejected`. An unknown or ended session is 401 `invalid_credentials` and its response clears both cookies. A cookie is read only on an API that accepts `browser_session`; an `Authorization` header where no bearer method is built is 401 `invalid_credentials` | 2026-10-16 |
| Where sessions live (chosen in chunk 4.3) | `jadpo_sessions` is a system table in the store the principal's user entity lives in, in its migration snapshots, read with the user's record in one statement (question 12); area 5's `sessions:` store setting isn't needed while that holds, and a store of their own (Redis) would bring it back | 2026-10-16 |
| Signing in on a request (chosen in chunk 4.3) | Authentication runs after routing, inside the route's deadline and before its input is read; a route that selects `current_principal.user` refuses a service with 403 `not_permitted`. A signed-in caller's requests count against its own allowance, 600 a minute (area 16), counted per worker thread natively until the Redis store; a request with a credential that is refused counts against its address, and every other against its address as before. Idempotency keys are kept per caller (`user <id>`), and per `anonymous` on routes anyone may call. A response whose action started or ended a session isn't kept for a repeat, so a retry signs in again | 2026-10-16 |
| Starting and ending sessions (chosen in chunk 4.3) | `sessions.start(user_id)` takes the user entity's `id`, `sessions.end()` nothing; only actions call them, not functions, queries, tests, fixtures or seeds. Each writes the sessions' store, so the action writes that store (one store per action, area 6). A test that calls such an action keeps the session with no response to set it on. `sessions.end()` on a request with no session ends none and still clears the cookies | 2026-10-16 |
| The caller in code (chosen in chunk 4.3) | `current_principal` alone is a `Principal`, whose fields aren't read until a kind is selected; a route passes `current_principal`, `current_principal.user` or one of its fields, never both kinds. A caller's `id` counts as a record loaded earlier, so writing a record that refers to it needs no not-found failure. `as:` signs in a user through a real browser session (a session kept in its own transaction, the cookie, the CSRF token and an `Origin`); `as:` on a route anyone may call is `semantic.request_invalid` | 2026-10-16 |
| Reports (chosen in chunk 4.3) | Each API's OpenAPI document lists the accepted methods as security schemes (`browser_session` as an `apiKey` cookie) and requires them at the top; a route anyone may call says `security: []`, and every other answers 401 (`authentication_required`, `invalid_credentials`, `ambiguous_credentials`), 403 (`user_disabled`, and `csrf_rejected` on a write) and 503 (`authentication_unavailable`). The API record writes `authentication: none` under a route anyone may call. `jadpo routes` shows `required`, `none` or `fresh`. Wrong route and API values are `route.authentication_invalid` and `api.authentication_invalid` (area 7) | 2026-10-16 |
| Not built yet (chosen in chunk 4.3) | `revocation_delay:` is checked for a positive duration, then `toolchain.not_built_yet` until signing keys (chunk 5.1b), as are `origin:` and a `signing_key:` read from configuration; a `signing_key:` written as a literal is `semantic.auth_setting_invalid`. `api_token:` and `service_key:` wait for chunk 4.3b, `external_token:`, `sign_in_providers:` and `oauth_provider:` for after the first build | 2026-10-16 |
