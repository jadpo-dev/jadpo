# `route.output_unexpected`

A route that answers `SuccessStatus.no_content` has an `output:`.

## Why Jadpo refuses it

`SuccessStatus.no_content` answers 204, which has no body, so an `output:`
beside it promises something the route can never send. Jadpo can't tell
whether the status or the output is the mistake.

## What it looks like

main.jadpo:

```jadpo check project=e-output-unexpected expect=route.output_unexpected
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
        status: SuccessStatus.no_content
        run: add_memo(input)
    }
}
```

```text output
error: `POST /memos` answers with no content
 --> main.jadpo:31:9
   |
31 |         output: MemoView
   |         ^^^^^^^^^^^^^^^^ no body is sent
  reason: `SuccessStatus.no_content` answers 204 with no body, so the route has no `output:` and what it runs returns `Unit`.
  next: Remove `output:`, or choose another status (agent)
  rule: route.output_unexpected
  help: https://jadpo.dev/docs/errors/route.output_unexpected

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the client should get the answer**, choose a status with a body:

```jadpo fragment
POST / {
    output: MemoView
    status: SuccessStatus.created
    ...
}
```

```jadpo check project=e-output-unexpected-body hidden
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

The route now answers 201 with a `MemoView`.

**If it shouldn't**, remove `output:`, and return `Unit` from what it runs:

```jadpo fragment
action add_memo(input: AddMemo) -> Unit {
    assert input.text != ""
}
```

```jadpo fragment
POST / {
    status: SuccessStatus.no_content
    ...
}
```

```jadpo check project=e-output-unexpected-none hidden
type Memo = Text {
    max_length: 200
}

type AddMemo = Input {
    text: Memo
}

action add_memo(input: AddMemo) -> Unit {
    assert input.text != ""
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
        status: SuccessStatus.no_content
        run: add_memo(input)
    }
}
```

The route now answers 204 with no body.

## Related

- [Output and status](../language/07-http.md#output-and-status)
