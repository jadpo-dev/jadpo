# `semantic.auth_not_set_up`

A route needs a signed-in caller, but the app declares no way for anyone to sign in.

## Why Jadpo refuses it

Every route needs a signed-in caller unless it says
`authentication: RouteAuthentication.none`. A caller signs in through one
of the ways `authentication { }` declares, such as a browser session. An
app that declares none has no signed-in callers, so a route that needs
one would refuse every request with 401.

Jadpo won't choose the fix for you. One way makes people sign in before
they reach the route, and the other opens it to anyone.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/semantic.auth_not_set_up

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether people must sign in to call `POST /memos`.

The error is reported once, at the first route that needs a caller, and
its reason counts the others.

## How to fix it

The question is whether people should sign in to use the app's routes.

**If people sign in**, declare who they are and how they sign in. The
principal names the stored record a signed-in person is, and
`authentication { }` lists the ways in, here a browser session kept in a
cookie. In a project the principal goes in `app.jadpo` and
`authentication { }` in `authentication.jadpo`:

```jadpo fragment
principal {
    user: {
        subject: User.handle
    }
}

authentication {
    browser_session: {
        cookie: "memo_session"
    }
}
```

```jadpo check project=e-auth-not-set-up-session hidden
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

authentication {
    browser_session: {
        cookie: "memo_session"
    }
}

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

policy {
    actions: {
        add_memo: [Access.authenticated]
    }
}
```

`POST /memos` now answers callers with a valid session cookie. Anyone
else gets 401 before `add_memo` runs. The checked project also
stores the `User` record the principal names, and lists `add_memo`, which
reaches no stored record, for anyone signed in under `actions:`.

**If anyone may call the route, signed in or not**, say so on the route
with `authentication: RouteAuthentication.none`, and list `add_memo` for
`Access.public`:

```jadpo fragment
        authentication: RouteAuthentication.none
```

```jadpo fragment
policy {
    actions: {
        add_memo: [Access.public]
    }
}
```

```jadpo check project=e-auth-not-set-up-open hidden
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
        authentication: RouteAuthentication.none
        input: AddMemo
        status: SuccessStatus.no_content
        run: add_memo(input)
    }
}

policy {
    actions: {
        add_memo: [Access.public]
    }
}
```

`POST /memos` answers anyone and never knows who is calling. Its input is
still validated.

## Related

- [Routes: signed in by default](../language/08-identity.md#routes-signed-in-by-default)
- [Ways to sign in](../language/08-identity.md#ways-to-sign-in)
- [The principal](../language/08-identity.md#the-principal)
- [Open to anyone: `RouteAuthentication.none`](../language/08-identity.md#open-to-anyone-routeauthenticationnone)
- [Actions](../language/09-policy.md#actions)
