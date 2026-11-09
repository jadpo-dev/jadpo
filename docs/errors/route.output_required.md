# `route.output_required`

A route doesn't say what it sends, with `output:` or `status: SuccessStatus.no_content`.

## Why Jadpo refuses it

A route shows what it sends without anyone opening what it runs, so the
route's contract can be read, recorded and checked from the route alone. It
says so with `output:`, or with `status: SuccessStatus.no_content` when it
sends nothing.

## What it looks like

main.jadpo:

```jadpo check project=e-output-required expect=route.output_required
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
        run: add_memo(input)
    }
}
```

```text output
error: `POST /memos` has no `output:`
 --> main.jadpo:28:5
   |
28 |     POST / {
   |     ^^^^^^ no `output:`
  reason: A route shows its contract without opening what it runs: `output:` names what it sends, exactly what `add_memo` returns (`MemoView`).
  next: Add `output: MemoView` (agent)
  rule: route.output_required
  help: https://jadpo.dev/docs/errors/route.output_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

What returns `Unit` sends nothing, so the route answers 204 rather than
naming an `output:`:

```jadpo check project=e-output-required-unit expect=route.output_required
type Memo = Text {
    max_length: 200
}

type AddMemo = Input {
    text: Memo
}

action check_memo(input: AddMemo) -> Unit {
    assert input.text == input.text
}

policy {
    actions: {
        check_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST /checks {
        authentication: RouteAuthentication.none
        input: AddMemo
        run: check_memo(input)
    }
}
```

```text output
error: `POST /memos/checks` has no `output:`
 --> main.jadpo:22:5
   |
22 |     POST /checks {
   |     ^^^^^^^^^^^^ no `output:`
  reason: A route shows its contract without opening what it runs: `check_memo` returns `Unit`, so the route sends nothing, which `status: SuccessStatus.no_content` says, answering 204 with no body.
  next: Add `status: SuccessStatus.no_content` (agent)
  rule: route.output_required
  help: https://jadpo.dev/docs/errors/route.output_required

check failed: 1 error, 1 file
```

## How to fix it

**If what it runs returns something**, name that type:

```jadpo fragment
POST / {
    output: MemoView
    ...
}
```

```jadpo check project=e-output-required-named hidden
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

The route now answers 200 with a `MemoView`.

**If it returns `Unit`**, say the route sends nothing:

```jadpo fragment
POST /checks {
    status: SuccessStatus.no_content
    ...
}
```

```jadpo check project=e-output-required-none hidden
type Memo = Text {
    max_length: 200
}

type AddMemo = Input {
    text: Memo
}

action check_memo(input: AddMemo) -> Unit {
    assert input.text == input.text
}

policy {
    actions: {
        check_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST /checks {
        authentication: RouteAuthentication.none
        input: AddMemo
        status: SuccessStatus.no_content
        run: check_memo(input)
    }
}
```

The route now answers 204 with no body.

## Related

- [Output and status](../language/07-http.md#output-and-status)
- [What a route runs](../language/07-http.md#what-a-route-runs)
