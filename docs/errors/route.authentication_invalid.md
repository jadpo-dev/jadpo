# `route.authentication_invalid`

A route's `authentication:` line holds something other than `RouteAuthentication.none` or `RouteAuthentication.fresh`.

## Why Jadpo refuses it

Every route needs a signed-in caller unless it says otherwise, and
`authentication:` is where it says so. It takes one of two values.
`RouteAuthentication.none` lets anyone call the route, signed in or not.
`RouteAuthentication.fresh` keeps the route signed in and looks up the
caller's record again on this request.

Any other value could have meant either. Jadpo won't pick one for you,
because one keeps the route protected and the other opens it to anyone.

## What it looks like

main.jadpo:

```jadpo check project=e-route-authentication expect=route.authentication_invalid
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
        authentication: none
        input: AddMemo
        status: SuccessStatus.no_content
        run: add_memo(input)
    }
}
```

```text output
error: `none` isn't a `RouteAuthentication`
 --> main.jadpo:18:25
   |
18 |         authentication: none
   |                         ^^^^ not `.none` or `.fresh`
  reason: Every route needs a signed-in caller unless it says otherwise: `RouteAuthentication.none` lets anyone call it, and `RouteAuthentication.fresh` checks the caller's record on this request. One opens the route and the other keeps it protected, so neither is guessed.
  next: Write `RouteAuthentication.fresh` to keep the route signed in, `RouteAuthentication.none` to let anyone call it, or remove `authentication:` (human)
  rule: route.authentication_invalid
  help: https://jadpo.dev/docs/errors/route.authentication_invalid

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether people must sign in to call `POST /memos`.

## How to fix it

The question is whether a caller must be signed in to use the route.

**If anyone may call it, signed in or not**, write
`RouteAuthentication.none`. `add_memo` reaches no stored record, so it
takes no permission from one, and root `policy { }` also lists it for
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

```jadpo check project=e-route-authentication-none hidden
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

`POST /memos` no longer looks for a session or token, so it never knows
who is calling. Its input is still validated and its limits still count.

**If callers must be signed in**, remove the `authentication:` line, since
needing a signed-in caller is the default. The app must sign people in,
so this example adds a principal and `authentication { }`, and lists
`add_memo` for anyone signed in:

```jadpo fragment
    POST / {
        input: AddMemo
        status: SuccessStatus.no_content
        run: add_memo(input)
    }
```

```jadpo check project=e-route-authentication-default hidden
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
    api_token: {}
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

A request without a valid session or token now gets 401 before
`add_memo` runs.

**If callers must be signed in and checked again on this request**, write
`RouteAuthentication.fresh`, in an app set up to sign people in as above:

```jadpo fragment
        authentication: RouteAuthentication.fresh
```

```jadpo check project=e-route-authentication-fresh hidden
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
    api_token: {}
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
        authentication: RouteAuthentication.fresh
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

The route needs a signed-in caller, and it always looks up the caller's
record, even when the app sets a `revocation_delay:`. Without a delay,
every route already does that lookup, so `fresh` changes nothing until
one is set.

## Related

- [Routes: signed in by default](../language/08-identity.md#routes-signed-in-by-default)
- [Open to anyone: `RouteAuthentication.none`](../language/08-identity.md#open-to-anyone-routeauthenticationnone)
- [Check the caller again: `RouteAuthentication.fresh`](../language/08-identity.md#check-the-caller-again-routeauthenticationfresh)
- [Actions](../language/09-policy.md#actions)
