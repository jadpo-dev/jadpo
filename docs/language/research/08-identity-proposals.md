# Identity: simplifying authentication

Research for [area 8](../08-identity.md), question 1. Written 2026-10-12
against jadpo 3ce094d0.

## What is complicated today

The golden todo's `authentication.jadpo` is 120 lines for one browser
cookie, one bearer header, an OIDC provider and service keys. An author must
understand six ideas that most frameworks never show:

- **Strategies** named by transport (`browser_session`, `api_bearer`), each
  with a `transport { }` block whose key picks the kind (`cookie: "x"`,
  `bearer: authorization_header`, the only legal value).
- **Validators** inside each strategy, each with a name nobody calls (except
  the exchange), a `mode:` (`signed`, `opaque`, `jwt`, `api_key`) and a
  `principal:` variant.
- **Claims** mappings (`authentication_strength -> Principal.user.authentication_strength`),
  where the compiler accepts only two sources, `subject` and
  `authentication_strength`.
- **Resolutions** per strategy, so the user lookup is written twice, once in
  each strategy, word for word.
- **Mappings** inside each resolution (`id -> Principal.user.user_id`), using
  `->` where every other block uses `name: value`.
- **A declared principal** whose fields look free but are not: anything beyond
  `subject`, a UUID id and `authentication_strength` passes `jadpo check` and
  then fails `jadpo build` with `toolchain.target_auth_not_implemented`
  (verified by compiling). It also cannot hold a typed `User.id`
  (`type.auth_mapping_mismatch`), so code converts with
  `Todo.owner_id(principal.user_id)`.

Most of these lines restate facts the compiler already fixes. The security
rules underneath (one credential per request, authentication before input
decoding, provider data never reaching business code, a stated revocation
delay) are good and none of the options below changes them.

## How other frameworks keep it simple

| Framework | What the author writes | What the framework owns |
|---|---|---|
| Rails 8 `generate authentication` ([guide](https://guides.rubyonrails.org/security.html), [concern](https://github.com/rails/rails/blob/main/railties/lib/rails/generators/rails/authentication/templates/app/controllers/concerns/authentication.rb.tt)) | A `User` with `has_secure_password`; `allow_unauthenticated_access` on public controllers | A `Session` table, a signed `session_id` cookie, `Current.session` / `Current.user`, `start_new_session_for(user)`, `terminate_session` |
| Phoenix `phx.gen.auth` ([guide](https://phoenix.hexdocs.pm/mix_phx_gen_auth.html)) | The generated code, edited in place | A tokens table, a `current_scope` assign, plugs `require_authenticated_user` and `require_sudo_mode` for sensitive pages |
| Django ([docs](https://docs.djangoproject.com/en/5.2/topics/auth/default/)) | `LoginRequiredMiddleware` (5.1) makes every view protected; `@login_not_required` opts out | `request.user`, inactive users rejected by the default backend |
| Laravel Sanctum ([docs](https://laravel.com/docs/12.x/sanctum)) | `auth:sanctum` on routes; the list of first-party domains | One guard tries the session cookie (with CSRF) and then a bearer token; a `personal_access_tokens` table it owns; tokens hashed, shown once, revoked by deleting |
| Auth.js ([docs](https://authjs.dev/getting-started/session-management/protecting)) | A provider list; `auth()` where the session is needed | The session cookie, the provider flows |
| Lucia ([site](https://lucia-auth.com/)) | Deprecated as a library in March 2025; now a one-file reference: a sessions table and a cookie | Nothing: the lesson is that sessions are small enough to own |
| Supabase / Clerk ([Supabase JWTs](https://supabase.com/docs/guides/auth/jwts), [Clerk](https://clerk.com/docs/guides/how-clerk-works/overview)) | Nothing on the server beyond verifying the token | Login, users, short-lived tokens; Clerk's session token lives 60 seconds and is refreshed every 50, which bounds revocation |

The common pattern: **the author names the user record and which routes are
public; the framework owns the session store, the cookie, CSRF, token
hashing and the "current user" value.** Nobody writes validators, claim
mappings or a principal type. Sanctum shows that one guard can accept a
cookie or a bearer token without the author separating them, and Clerk shows
a short token lifetime is a plain way to state a revocation bound.

## Option A: name the methods, the language owns the rest (recommended)

```jadpo fragment
// app.jadpo
application {
    revocation_delay: 5m          // leave out for immediate revocation (question 4)
}

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

// authentication.jadpo
authentication {
    browser_session: {
        cookie: "todo_session"
        signing_key: config.session_signing_key
        origin: config.browser_origin
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

- The principal says only which entity field identifies a user or a service
  and when it is active. Its fields are built in: `id` (typed `User.id` or
  `Service.id`), `subject` and `strength`. This is exactly what the compiler
  supports today, so nothing is lost.
- Each method is one named block from a fixed list (`browser_session`,
  `api_token`, `oidc`, `service_key`). The method decides the transport:
  `browser_session` is a cookie, the others are bearer tokens. Signed or
  opaque follows from the revocation setting: a delay means signed tokens
  checked locally; no delay means opaque tokens checked on every request.
- Audiences are derived from the application and method; only OIDC takes one,
  because the provider sets it.
- Service credentials are stored by the compiler, like browser sessions
  already are (`__jadpo_auth_sessions`), with generated operations to issue,
  list and revoke them (question 7). The key-for-token exchange is generated
  at a fixed path whenever service keys and a revocation delay are both
  present.
- Disabled users and services fail with built-in failures (question 6).

About 25 lines instead of 120, plus the principal and service credential
entity lines that disappear. The rules that go away entirely: validator and
strategy names (`semantic.auth_strategy_duplicate`,
`semantic.auth_validator_*`), claim and mapping rules (`type.auth_claim_*`,
`semantic.auth_mapping_*`, `type.auth_mapping_mismatch`,
`type.auth_resolution_incomplete`), `semantic.auth_bearer_location_forbidden`
(no location to choose), the exchange binding rules and
`semantic.application_principal_shape`.

**Cost:** the parser, semantic model and audit for authentication are
rewritten; the runtime (selector, adapters, revocation, CSRF) stays. An
application that needed a custom principal field (a tenant, a role) reads it
from its own entity through a query, which is where area 9's policy wants it
anyway.

## Option B: today's model, cleaned up

Keep strategies, validators and resolutions, but apply the decided syntax
rules and stop repeating the lookup:

```jadpo fragment
principal {
    user: {
        subject: Text
        user_id: User.id
        resolve: {
            authority: User.authentication_subject
            active: User.status == UserStatus.active
            user_id: User.id
            subject: User.authentication_subject
            inactive: UserDisabled
        }
    }
}

authentication browser_session {
    transport: {
        kind: cookie
        cookie_name: "todo_session"
    }
    validators: {
        signed_user: {
            credential: signed_token
            for: user
            signing_key: config.session_signing_key
            audience: "todo-browser"
            origin: config.browser_origin
        }
    }
}
```

This is the syntax review's findings #44 to #49 applied as written: the
resolution moves to the principal so it is written once, mappings become
object construction, `mode:` becomes `credential:`, `principal:` becomes
`for:`, `secret:` becomes `signing_key:`. It keeps every concept, so it is
cheap to build and loses no flexibility, but an author still meets
transports, validators and principal fields that the compiler fixes anyway.
The golden file shrinks to about 70 lines.

## Option C: external identity only

Jadpo accepts only OIDC tokens from a hosted provider (Auth0, Clerk,
Supabase, Cognito) for people, plus service keys for machines:

```jadpo fragment
authentication {
    oidc: {
        issuer: config.oidc_issuer
        audience: config.oidc_audience
    }
    service_key: {
        signing_key: config.service_signing_key
    }
}
```

The smallest surface, and login, passwords and multi-factor become the
provider's problem. But every application then needs a provider account and
the pinned `jose` dependency, browser sessions lose Jadpo's CSRF and origin
checks, and revocation for people is whatever the provider's token lifetime
is. It also throws away the working first-party cookie path (24 runtime cases
on SQLite and PostgreSQL).

## Trade-offs

| | A: methods | B: cleaned | C: external only |
|---|---|---|---|
| Lines in the golden todo | ~25 | ~70 | ~10 |
| Concepts an author meets | principal, method | principal, strategy, transport, validator, resolution | method |
| Custom principal fields | No (read from your entity) | Declared, but only the supported set builds | No |
| Works without a provider | Yes | Yes | No |
| Build cost | High (front end rewritten) | Low | Medium |
| Errors that disappear | About 20 | About 3 | About 30 |

## Recommendation

**Option A.** It matches what every framework above converged on: the author
names the user record, the public routes and the methods; the language owns
sessions, tokens, CSRF and the caller value. It removes declarations whose
flexibility is fake today, and keeps every security rule from the AUTH-001
contract. Option B is the fallback if the rewrite has to wait: it can land
first, and A later removes what B tidied.
