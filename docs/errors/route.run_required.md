# `route.run_required`

A route doesn't say what it runs.

## Why Jadpo refuses it

A route runs one named action, or for a `GET` a named query or an action that
only reads. The logic lives there, not in the route, so a route with no `run:`
has nothing to do when a request arrives.

## What it looks like

main.jadpo:

```jadpo check project=e-run-required expect=route.run_required
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
    }
}
```

```text output
error: `POST /memos` doesn't say what it runs
 --> main.jadpo:28:5
   |
28 |     POST / {
   |     ^^^^^^ no `run:`
  reason: A route runs one named action with `run:`, or for a `GET` a named query or an action that only reads; the logic lives there, not in the route.
  next: Add `run: <action>(...)` (agent)
  rule: route.run_required
  help: https://jadpo.dev/docs/errors/route.run_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the route should run something**, name it with `run:`, passing its
placeholders and `input`:

```jadpo fragment
POST / {
    run: add_memo(input)
    ...
}
```

```jadpo check project=e-run-required-fixed hidden
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

The route now runs `add_memo` when a request arrives.

## Related

- [What a route runs](../language/07-http.md#what-a-route-runs)
