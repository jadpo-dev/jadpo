# `api.authentication_invalid`

An API's `authentication:` isn't a list of sign-in methods the application declares.

## Why Jadpo refuses it

An API's `authentication:` says who can call anything in it, so each entry
has to be a way of signing in the application has set up. A method nobody
declared could never sign anyone in, and Jadpo won't drop it or read it as
another method, because either would change who can reach the API. Each
method is listed once.

## What it looks like

app.jadpo:

```jadpo check project=e-api-authentication file=app.jadpo expect=api.authentication_invalid
application {
    apis: {
        main: {
            prefix: /v1
            authentication: [AuthenticationMethod.browser_session]
        }
    }
}
```

```text output
error: API `main` accepts `browser_session`, which isn't declared
 --> app.jadpo:5:30
  |
5 |             authentication: [AuthenticationMethod.browser_session]
  |                              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not declared
  reason: An API accepts only the ways to sign in the application declares; `authentication.jadpo` declares no way to sign in yet.
  next: List the methods the API accepts, each one `authentication.jadpo` declares (agent)
  rule: api.authentication_invalid
  help: https://jadpo.dev/docs/errors/api.authentication_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

The value is a list even when it holds one method:

```jadpo check project=e-api-authentication-list file=app.jadpo expect=api.authentication_invalid
application {
    apis: {
        main: {
            prefix: /v1
            authentication: AuthenticationMethod.browser_session
        }
    }
}
```

```text output
error: API `main`'s `authentication:` isn't a list
 --> app.jadpo:5:29
  |
5 |             authentication: AuthenticationMethod.browser_session
  |                             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not a list
  reason: An API lists the sign-in methods it accepts, such as `[AuthenticationMethod.browser_session]`; `authentication.jadpo` declares no way to sign in yet.
  next: List the methods the API accepts, each one `authentication.jadpo` declares (agent)
  rule: api.authentication_invalid
  help: https://jadpo.dev/docs/errors/api.authentication_invalid

check failed: 1 error, 1 file
```

## How to fix it

The question is whether callers sign in to this API.

**If callers sign in that way**, declare the method in
`authentication.jadpo`, and the principal it signs in:

```jadpo fragment
authentication {
    browser_session: {
        cookie: "memo_session"
    }
}
```

```jadpo check project=e-api-authentication-declared file=authentication.jadpo hidden
authentication {
    browser_session: {
        cookie: "memo_session"
    }
}
```

```jadpo check project=e-api-authentication-declared file=app.jadpo hidden
use accounts/entities { Member }

application {
    apis: {
        main: {
            prefix: /v1
            authentication: [AuthenticationMethod.browser_session]
        }
    }
}

principal {
    user: {
        subject: Member.handle
    }
}
```

```jadpo check project=e-api-authentication-declared file=domains/accounts/entities/member.jadpo hidden migrate=initial
entity Member {
    id: Uuid
    handle: Text {
        max_length: 40
        unique: true
    }
}
```

The API now accepts a browser session, and a route in it finds its caller
through one.

**If the API takes no sign-in at all**, say so with an empty list:

```jadpo check project=e-api-authentication-none file=app.jadpo
application {
    apis: {
        main: {
            prefix: /v1
            authentication: []
        }
    }
}
```

The API accepts no credential, so every route in it has to say
`authentication: RouteAuthentication.none`.

## Related

- [APIs and versions](../language/07-http.md#apis-and-versions)
- [Apps with more than one API](../language/08-identity.md#apps-with-more-than-one-api)
- [Ways to sign in](../language/08-identity.md#ways-to-sign-in)
