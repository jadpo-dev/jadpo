# `route.secret_outside_body`

A route reads a secret value from a URL or a header.

## Why Jadpo refuses it

A URL is written to access logs and proxies' logs, kept in a browser's
history and passed on in a `Referer` header. A `Password` in a path or a
query string is read by people it was never meant for, whatever the app
does next, so a secret arrives in a request's JSON body.

A header is for credentials sent on every request. A `BearerToken`, a
`Jwt` and an `ApiKey` may be read from one; a `Password`, a `TotpSecret`, a
`SecretText` and a `OneTimeCode` are typed once into a form, so they are
not. `Authorization` and cookies are read by sign-in whatever the type
([`route.header_credential`](route.header_credential.md)).

## What it looks like

main.jadpo:

```jadpo check project=e-secret-outside-body expect=route.secret_outside_body
type Handle = Text {
    max_length: 40
}

type LoginAccepted = Bool {}

type SignInForm = Input {
    handle: Handle
    account_password: Password
}

type SignInAnswer = Output {
    accepted: LoginAccepted
}

action sign_in(login_attempt: SignInForm) -> SignInAnswer {
    return SignInAnswer {
        accepted: LoginAccepted(login_attempt.account_password == "correct horse battery staple")
    }
}

policy {
    actions: {
        sign_in: [Access.public]
    }
}

routes Logins {
    prefix: /logins

    GET / {
        authentication: RouteAuthentication.none
        input: SignInForm
        output: SignInAnswer
        run: sign_in(input)
    }
}
```

```text output
error: `SignInForm.account_password` reads a `Password` from the query string
 --> main.jadpo:33:16
   |
33 |         input: SignInForm
   |                ^^^^^^^^^^ a secret, in the query string
  reason: A URL is written to access logs and proxies' logs, kept in a browser's history and passed on in a `Referer` header, so a `Password` in one is read by people it was never meant for.
  next: Take `SignInForm.account_password` in the JSON body of a `POST`, `PUT` or `PATCH`, in the route's `input:` (agent)
  rule: route.secret_outside_body
  help: https://jadpo.dev/docs/errors/route.secret_outside_body

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A placeholder is refused the same way:

```jadpo check project=e-secret-in-path expect=route.secret_outside_body
type ResetPassword = Password {}

type ResetAnswer = Output {
    accepted: Bool
}

action reset(given: ResetPassword) -> ResetAnswer {
    return ResetAnswer {
        accepted: given == "correct horse battery staple"
    }
}

policy {
    actions: {
        reset: [Access.public]
    }
}

routes Resets {
    prefix: /resets

    POST /{given: ResetPassword} {
        authentication: RouteAuthentication.none
        output: ResetAnswer
        run: reset(given)
    }
}
```

```text output
error: `given` reads a `Password` from a URL
 --> main.jadpo:22:19
   |
22 |     POST /{given: ResetPassword} {
   |                   ^^^^^^^^^^^^^ a secret, in a URL
  reason: A URL is written to access logs and proxies' logs, kept in a browser's history and passed on in a `Referer` header, so a `Password` in one is read by people it was never meant for.
  next: Take `given` in the JSON body of a `POST`, `PUT` or `PATCH`, in the route's `input:` (agent)
  rule: route.secret_outside_body
  help: https://jadpo.dev/docs/errors/route.secret_outside_body

check failed: 1 error, 1 file
```

## How to fix it

Decide how the secret should arrive.

**If the secret is typed once into a form**, read it from the body of a
`POST`, `PUT` or `PATCH`:

```jadpo fragment
    POST / {
        authentication: RouteAuthentication.none
        input: SignInForm
        output: SignInAnswer
        run: sign_in(input)
    }
```

```jadpo check project=e-secret-outside-body-fixed hidden
type Handle = Text {
    max_length: 40
}

type LoginAccepted = Bool {}

type SignInForm = Input {
    handle: Handle
    account_password: Password
}

type SignInAnswer = Output {
    accepted: LoginAccepted
}

action sign_in(login_attempt: SignInForm) -> SignInAnswer {
    return SignInAnswer {
        accepted: LoginAccepted(login_attempt.account_password == "correct horse battery staple")
    }
}

policy {
    actions: {
        sign_in: [Access.public]
    }
}

routes Logins {
    prefix: /logins

    POST / {
        authentication: RouteAuthentication.none
        input: SignInForm
        output: SignInAnswer
        run: sign_in(input)
    }
}
```

A client now posts the password in the request's JSON, so it stays out of
the URL.

**If it is a key or a token sent on every request**, type it `ApiKey`,
`BearerToken` or `Jwt` and say `header:`:

```jadpo fragment
    partner_key: ApiKey {
        header: "X-Partner-Key"
    }
```

```jadpo check project=e-secret-outside-body-header hidden
type Known = Bool {}

type PartnerRequest = Input {
    partner_key: ApiKey {
        header: "X-Partner-Key"
    }
}

type PartnerAnswer = Output {
    known: Known
}

action partner(given: PartnerRequest) -> PartnerAnswer {
    return PartnerAnswer {
        known: Known(given.partner_key == "example_key_0123456789")
    }
}

policy {
    actions: {
        partner: [Access.public]
    }
}

routes Partners {
    prefix: /partners

    GET / {
        authentication: RouteAuthentication.none
        input: PartnerRequest
        output: PartnerAnswer
        run: partner(input)
    }
}
```

The key comes in the header, and the check passes for a key or a token.

## Related

- [One input](../language/07-http.md#one-input)
- [Where a secret can go](../language/19-standard-library.md#where-a-secret-can-go)
- [`route.header_credential`](route.header_credential.md)
