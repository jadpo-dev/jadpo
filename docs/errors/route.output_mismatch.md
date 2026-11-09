# `route.output_mismatch`

A route says it sends something other than what it runs returns.

## Why Jadpo refuses it

`output:` is there so a route shows what it sends without anyone opening
what it runs. If it could differ from what the route runs returns, the
route's contract could promise fields that are never sent, or send ones it
doesn't declare. Jadpo can't tell which of the two is the mistake.

## What it looks like

main.jadpo:

```jadpo check project=e-output-mismatch expect=route.output_mismatch
type Memo = Text {
    max_length: 200
}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

type MemoSummary = Output {
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
        output: MemoSummary
        run: add_memo(input)
    }
}
```

```text output
error: `POST /memos` sends `MemoSummary`, but `add_memo` returns `MemoView`
 --> main.jadpo:35:17
   |
35 |         output: MemoSummary
   |                 ^^^^^^^^^^^ not `MemoView`
  reason: A route's `output:` is exactly what it runs returns, so the route shows its contract and never sends a field its type doesn't declare.
  next: Write `output: MemoView` (agent)
  rule: route.output_mismatch
  help: https://jadpo.dev/docs/errors/route.output_mismatch

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A route that sends nothing runs something that returns `Unit`, so a result
nobody receives is refused too:

```jadpo check project=e-output-mismatch-none expect=route.output_mismatch
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
        status: SuccessStatus.no_content
        run: add_memo(input)
    }
}
```

```text output
error: `add_memo` returns `MemoView`, but `POST /memos` sends nothing
 --> main.jadpo:32:14
   |
32 |         run: add_memo(input)
   |              ^^^^^^^^ returns `MemoView`
  reason: `SuccessStatus.no_content` answers 204 with no body, so what the route runs returns `Unit`; a result nobody receives is a mistake.
  next: Give the route an `output:` and another status, or run something that returns `Unit` (agent)
  rule: route.output_mismatch
  help: https://jadpo.dev/docs/errors/route.output_mismatch

check failed: 1 error, 1 file
```

## How to fix it

The question is which of the two says what you meant.

**If what it runs is right**, change `output:` to match:

```jadpo fragment
POST / {
    output: MemoView
    ...
}
```

```jadpo check project=e-output-mismatch-output hidden
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
        run: add_memo(input)
    }
}
```

The route now says it sends the `MemoView` that `add_memo` returns.

**If `output:` is right**, have what it runs return that type:

```jadpo fragment
action add_memo(input: AddMemo) -> MemoSummary {
    return MemoSummary {
        text: input.text
    }
}
```

```jadpo check project=e-output-mismatch-action hidden
type Memo = Text {
    max_length: 200
}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

type MemoSummary = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoSummary {
    return MemoSummary {
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
        output: MemoSummary
        run: add_memo(input)
    }
}
```

The route now sends the `MemoSummary` that `add_memo` returns.

## Related

- [Output and status](../language/07-http.md#output-and-status)
- [What a route runs](../language/07-http.md#what-a-route-runs)
