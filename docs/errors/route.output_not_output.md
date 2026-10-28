# `route.output_not_output`

A route sends only an `Output`, or a map layer whose properties hold only what an `Output` may, so only fields declared for clients leave the server:

```jadpo check project=e-output-not-output expect=route.output_not_output
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

type MemoRecord = Object {
    text: Memo
}

action keep_memo(input: AddMemo) -> MemoRecord {
    return MemoRecord {
        text: input.text
    }
}

routes Memos {
    prefix: /memos

    POST / {
        input: AddMemo
        output: MemoRecord
        run: keep_memo(input)
    }
}
```

```text output
error: `MemoRecord` isn't an `Output`
 --> main.jadpo:36:17
   |
36 |         output: MemoRecord
   |                 ^^^^^^^^^^ not an `Output`
  reason: A route returns only an `Output` type (area 2), a `List` or `Page` of one as a query gives them, a `FeatureCollection<G, P>` whose properties `P` hold only what an `Output` may (area 20), or an `AiStream`, a model's answer sent as it is written, so only fields declared for clients are ever sent.
  next: Declare `type <Name> = Output { ... }` with the fields clients see, and return it (agent)
  rule: route.output_not_output
  help: docs/errors/route.output_not_output.md

check failed: 1 error, 1 file
```
