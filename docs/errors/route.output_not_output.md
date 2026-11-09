# `route.output_not_output`

A route's `output:` isn't an `Output` type.

## Why Jadpo refuses it

A route's answer leaves the server, so only fields declared for clients may
be in it. An `Output` type says exactly which those are. A `List` or a `Page`
of one is allowed as a query gives them, and so are a map layer whose
properties hold only what an `Output` may and an `AiStream`. Any other type
wasn't written to be sent, and could carry fields that were never meant to
leave.

## What it looks like

main.jadpo:

```jadpo check project=e-output-not-output expect=route.output_not_output
type Memo = Text {
    max_length: 200
}

type AddMemo = Input {
    text: Memo
}

type MemoRecord = Object {
    text: Memo
}

action keep_memo(input: AddMemo) -> MemoRecord {
    return MemoRecord {
        text: input.text
    }
}

policy {
    actions: {
        keep_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoRecord
        run: keep_memo(input)
    }
}
```

```text output
error: `MemoRecord` isn't an `Output`
 --> main.jadpo:31:17
   |
31 |         output: MemoRecord
   |                 ^^^^^^^^^^ not an `Output`
  reason: A route returns only an `Output` type (area 2), a `List` or `Page` of one as a query gives them, a `FeatureCollection<G, P>` whose properties `P` hold only what an `Output` may (area 20), or an `AiStream`, a model's answer sent as it is written, so only fields declared for clients are ever sent.
  next: Declare `type <Name> = Output { ... }` with the fields clients see, and return it (agent)
  rule: route.output_not_output
  help: https://jadpo.dev/docs/errors/route.output_not_output

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If clients should see these fields**, declare the type as an `Output`:

```jadpo fragment
type MemoRecord = Output {
    text: Memo
}
```

```jadpo check project=e-output-not-output-fixed hidden
type Memo = Text {
    max_length: 200
}

type AddMemo = Input {
    text: Memo
}

type MemoRecord = Output {
    text: Memo
}

action keep_memo(input: AddMemo) -> MemoRecord {
    return MemoRecord {
        text: input.text
    }
}

policy {
    actions: {
        keep_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoRecord
        run: keep_memo(input)
    }
}
```

The route now sends exactly the fields `MemoRecord` declares.

## Related

- [Output and status](../language/07-http.md#output-and-status)
- [Named types](../language/02-values-and-types.md#named-types)
