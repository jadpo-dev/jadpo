# `route.status_invalid`

A route's `status:` isn't a `SuccessStatus`.

## Why Jadpo refuses it

A route says what success looks like with a `SuccessStatus`, one of `.ok`,
`.created`, `.accepted` and `.no_content`. Failure statuses come from the kinds
of the failures the route can produce, never from the route, so a route can't
claim a status its failures don't give it. A bare number can't say which
success it means, so Jadpo won't read it as one.

## What it looks like

main.jadpo:

```jadpo check project=e-status-invalid expect=route.status_invalid
type Memo = Text {
    max_length: 200
}

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

policy {
    actions: {
        add_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        status: 201
        run: add_memo(input)
    }
}
```

```text output
error: `201` isn't a success status
 --> main.jadpo:32:17
   |
32 |         status: 201
   |                 ^^^ not a `SuccessStatus`
  reason: `status:` takes `SuccessStatus.ok`, `.created`, `.accepted` or `.no_content`; failure statuses come from the kinds of the failures the route can produce, never from the route.
  next: Write `SuccessStatus.created` (agent)
  rule: route.status_invalid
  help: https://jadpo.dev/docs/errors/route.status_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A failure status is refused as well:

```jadpo check project=e-status-failure expect=route.status_invalid
type Memo = Text {
    max_length: 200
}

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

policy {
    actions: {
        add_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        status: 404
        run: add_memo(input)
    }
}
```

```text output
error: `404` isn't a success status
 --> main.jadpo:32:17
   |
32 |         status: 404
   |                 ^^^ not a `SuccessStatus`
  reason: `status:` takes `SuccessStatus.ok`, `.created`, `.accepted` or `.no_content`; failure statuses come from the kinds of the failures the route can produce, never from the route.
  next: Write one of the `SuccessStatus` variants (agent)
  rule: route.status_invalid
  help: https://jadpo.dev/docs/errors/route.status_invalid

check failed: 1 error, 1 file
```

## How to fix it

**If it is a success**, write its `SuccessStatus`:

```jadpo fragment
POST / {
    status: SuccessStatus.created
    ...
}
```

```jadpo check project=e-status-fixed hidden
type Memo = Text {
    max_length: 200
}

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

policy {
    actions: {
        add_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        status: SuccessStatus.created
        run: add_memo(input)
    }
}
```

The route now answers 201 when `add_memo` succeeds.

**If it is a failure**, declare the failure, of the kind that answers that
status, and reject it from what the route runs:

```jadpo fragment
type MemoMissing = Failure {
    kind: FailureKind.not_found
    message: "No memo has that text."
}

action add_memo(input: AddMemo) fails MemoMissing -> MemoView {
    if (input.text == "") {
        reject MemoMissing {}
    }
    return MemoView {
        text: input.text
    }
}
```

```jadpo check project=e-status-failure-fixed hidden
type Memo = Text {
    max_length: 200
}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

type MemoMissing = Failure {
    kind: FailureKind.not_found
    message: "No memo has that text."
}

action add_memo(input: AddMemo) fails MemoMissing -> MemoView {
    if (input.text == "") {
        reject MemoMissing {}
    }
    return MemoView {
        text: input.text
    }
}

policy {
    actions: {
        add_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

The route now answers 404 when `add_memo` rejects `MemoMissing`.

## Related

- [Output and status](../language/07-http.md#output-and-status)
- [How failures reach the client](../language/07-http.md#how-failures-reach-the-client)
