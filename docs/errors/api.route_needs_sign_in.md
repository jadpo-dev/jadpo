# `api.route_needs_sign_in`

A route needs a signed-in caller, but its API says `authentication: []`, which accepts no way to sign in.

## Why Jadpo refuses it

Every route needs a signed-in caller unless it says
`authentication: RouteAuthentication.none`. An API with `authentication: []`
takes no credential of any kind, whatever `authentication.jadpo` declares, so
a route in it that needs a caller would refuse every request with 401.

Jadpo won't choose the fix for you. One way lists how callers sign in to the
API, and the other opens the route to anyone.

## What it looks like

```jadpo check project=e-api-route-needs-sign-in file=app.jadpo
application {
    apis: {
        main: {
            prefix: /v1
            authentication: []
        }
    }
}
```

```jadpo check project=e-api-route-needs-sign-in file=domains/memos/actions/add_memo.jadpo hidden
type AddMemo = Input {
    text: Text
}

action add_memo(input: AddMemo) -> Unit {
    assert input.text != ""
}
```

```jadpo check project=e-api-route-needs-sign-in file=policy.jadpo hidden
use memos

policy {
    actions: {
        memos.add_memo: [Access.public]
    }
}
```

```jadpo check project=e-api-route-needs-sign-in file=domains/memos/routes/v1_memos.jadpo expect=api.route_needs_sign_in
use memos/actions { AddMemo, add_memo }

routes V1Memos {
    prefix: /v1/memos

    POST / {
        input: AddMemo
        status: SuccessStatus.no_content
        run: add_memo(input)
    }
}
```

```text output
error: `POST /v1/memos` needs a signed-in caller, but API `main` accepts no way to sign in
 --> domains/memos/routes/v1_memos.jadpo:6:5
  |
6 |     POST / {
  |     ^^^^^^ needs a signed-in caller
  reason: Every route needs a signed-in caller unless it says `authentication: RouteAuthentication.none`, and API `main` says `authentication: []`, so no request could reach this one.
  next: List the ways to sign in API `main` accepts in its `authentication:`, or let anyone call the route with `authentication: RouteAuthentication.none` (human)
  rule: api.route_needs_sign_in
  help: https://jadpo.dev/docs/errors/api.route_needs_sign_in

check failed: 1 error, 4 files
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether people must sign in to call `POST /v1/memos`.

## How to fix it

The question is whether callers of this route sign in.

**If anyone may call it**, say so on the route:

```jadpo fragment
POST / {
    authentication: RouteAuthentication.none
    ...
}
```

```jadpo check project=e-api-route-needs-sign-in-open file=domains/memos/routes/v1_memos.jadpo hidden
use memos/actions { AddMemo, add_memo }

routes V1Memos {
    prefix: /v1/memos

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        status: SuccessStatus.no_content
        run: add_memo(input)
    }
}
```

```jadpo check project=e-api-route-needs-sign-in-open file=app.jadpo hidden
application {
    apis: {
        main: {
            prefix: /v1
            authentication: []
        }
    }
}
```

```jadpo check project=e-api-route-needs-sign-in-open file=domains/memos/actions/add_memo.jadpo hidden
type AddMemo = Input {
    text: Text
}

action add_memo(input: AddMemo) -> Unit {
    assert input.text != ""
}
```

```jadpo check project=e-api-route-needs-sign-in-open file=policy.jadpo hidden
use memos

policy {
    actions: {
        memos.add_memo: [Access.public]
    }
}
```

`POST /v1/memos` answers anyone and never knows who is calling. Its input is
still validated.

**If callers sign in**, list how in the API's entry, instead of `[]`:

```jadpo fragment
main: {
    prefix: /v1
    authentication: [AuthenticationMethod.browser_session]
}
```

```jadpo check project=e-api-route-needs-sign-in-listed file=app.jadpo hidden
use memos/entities { Writer }

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
        subject: Writer.handle
    }
}
```

```jadpo check project=e-api-route-needs-sign-in-listed file=authentication.jadpo hidden
authentication {
    browser_session: {
        cookie: "memo_session"
    }
}
```

```jadpo check project=e-api-route-needs-sign-in-listed file=domains/memos/entities/writer.jadpo hidden migrate=initial
entity Writer {
    id: Uuid
    handle: Text {
        max_length: 100
        unique: true
    }
}
```

```jadpo check project=e-api-route-needs-sign-in-listed file=domains/memos/actions/add_memo.jadpo hidden
type AddMemo = Input {
    text: Text
}

action add_memo(input: AddMemo) -> Unit {
    assert input.text != ""
}
```

```jadpo check project=e-api-route-needs-sign-in-listed file=policy.jadpo hidden
use memos

policy {
    actions: {
        memos.add_memo: [Access.public]
    }
}
```

```jadpo check project=e-api-route-needs-sign-in-listed file=domains/memos/routes/v1_memos.jadpo hidden
use memos/actions { AddMemo, add_memo }

routes V1Memos {
    prefix: /v1/memos

    POST / {
        input: AddMemo
        status: SuccessStatus.no_content
        run: add_memo(input)
    }
}
```

Callers who aren't signed in now get a 401, and the API's document needs a
sign-in to read.

## Related

- [Serving the document](../language/07-http.md#serving-the-document)
- [Apps with more than one API](../language/08-identity.md#apps-with-more-than-one-api)
- [Routes: signed in by default](../language/08-identity.md#routes-signed-in-by-default)
