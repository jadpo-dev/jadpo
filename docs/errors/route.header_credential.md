# `route.header_credential`

A route's input reads a header that sign-in or the runtime reads first, such as `Authorization`, a cookie or `Idempotency-Key`.

## Why Jadpo refuses it

Some headers are read before your code runs. Sign-in reads the
credentials, `Authorization` and cookies, and Jadpo's own `X-Jadpo-`
headers, to find out who is calling. The runtime reads `Idempotency-Key`
to answer a retried write once.

A route never sees these as input. If an action could read a credential,
it could copy or log it, or trust a token sign-in never checked. Who is
calling reaches your code only as the caller sign-in found.

## What it looks like

main.jadpo:

```jadpo check project=e-header-credential expect=route.header_credential
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

type TaggedMemo = Input {
    text: Memo
    token: Memo {
        header: "Authorization"
    }
}

action add_tagged_memo(input: TaggedMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

policy {
    actions: {
        add_tagged_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        input: TaggedMemo
        output: MemoView
        run: add_tagged_memo(input)
    }
}
```

```text output
error: `Authorization` belongs to authentication
 --> main.jadpo:24:17
   |
24 |         header: "Authorization"
   |                 ^^^^^^^^^^^^^^^ read by authentication
  reason: Credentials (`Authorization`, cookies) and Jadpo's own `X-Jadpo-` headers are read by authentication (area 8), so a route never sees them as input.
  next: Remove the field, and take the caller as the route's `current_principal` (agent)
  rule: route.header_credential
  help: https://jadpo.dev/docs/errors/route.header_credential

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A field read from `Idempotency-Key` is refused the same way, because the
runtime answers a retry with the same key before the action runs:

```jadpo check project=e-header-idempotency expect=route.header_credential
type Memo = Text {
    max_length: 200
}

type RetriedMemo = Input {
    text: Memo
    key: Memo {
        header: "Idempotency-Key"
    }
}

type MemoView = Output {
    text: Memo
}

action add_retried_memo(input: RetriedMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

policy {
    actions: {
        add_retried_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        input: RetriedMemo
        output: MemoView
        run: add_retried_memo(input)
    }
}
```

```text output
error: `Idempotency-Key` belongs to the runtime
 --> main.jadpo:8:17
  |
8 |         header: "Idempotency-Key"
  |                 ^^^^^^^^^^^^^^^^^ read by the runtime
  reason: The runtime reads `Idempotency-Key` to answer a retried write once, before anything runs, so the action never sees the key (area 7).
  next: Remove the field; a retry with the same key is answered by the runtime (agent)
  rule: route.header_credential
  help: https://jadpo.dev/docs/errors/route.header_credential

check failed: 1 error, 1 file
```

## How to fix it

Remove the field. The question left is whether the action needs to know
who is calling.

**If the action doesn't need the caller**, the input keeps only the
fields the client sends. `TaggedMemo` becomes:

```jadpo fragment
type TaggedMemo = Input {
    text: Memo
}
```

```jadpo check project=e-header-credential-removed hidden
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

type TaggedMemo = Input {
    text: Memo
}

action add_tagged_memo(input: TaggedMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

policy {
    actions: {
        add_tagged_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        input: TaggedMemo
        output: MemoView
        run: add_tagged_memo(input)
    }
}
```

The route reads only the body's `text`. An `Authorization` header a
client sends no longer reaches the action. The same fix applies to an
`Idempotency-Key` field, and a retry with the same key is still answered
once by the runtime.

**If the action needs to know who is calling**, the route needs a
signed-in caller, so it drops `authentication: RouteAuthentication.none`
and passes `current_principal.user`. The app must sign people in, so this
example adds a principal and `authentication { }`, and lists the action
for anyone signed in:

```jadpo fragment
action add_tagged_memo(input: TaggedMemo, caller: Principal.user) -> MemoView {
    return MemoView {
        text: input.text
        author_id: caller.id
    }
}
```

```jadpo fragment
    POST / {
        input: TaggedMemo
        output: MemoView
        run: add_tagged_memo(input, current_principal.user)
    }
```

```jadpo check project=e-header-credential-caller hidden
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

type TaggedMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
    author_id: User.id
}

action add_tagged_memo(input: TaggedMemo, caller: Principal.user) -> MemoView {
    return MemoView {
        text: input.text
        author_id: caller.id
    }
}

policy {
    actions: {
        add_tagged_memo: [Access.authenticated]
    }
}

routes Memos {
    prefix: /memos

    POST / {
        input: TaggedMemo
        output: MemoView
        run: add_tagged_memo(input, current_principal.user)
    }
}
```

Sign-in reads the `Authorization` header and checks the token. A request
without a valid one gets 401 before `add_tagged_memo` runs, and the
action gets the caller sign-in found.

## Related

- [Headers](../language/07-http.md#headers)
- [Ways to sign in](../language/08-identity.md#ways-to-sign-in)
- [Who is calling: `current_principal`](../language/08-identity.md#who-is-calling-current_principal)
- [Idempotent writes](../language/07-http.md#idempotent-writes)
