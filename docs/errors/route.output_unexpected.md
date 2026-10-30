# `route.output_unexpected`

A `no_content` route sends no body:

```jadpo check project=e-output-unexpected expect=route.output_unexpected
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

routes Memos {
    prefix: /memos

    POST / {
        input: AddMemo
        output: MemoView
        status: SuccessStatus.no_content
        run: add_memo(input)
    }
}
```

```text output
error: `POST /memos` answers with no content
 --> main.jadpo:26:9
   |
26 |         output: MemoView
   |         ^^^^^^^^^^^^^^^^ no body is sent
  reason: `SuccessStatus.no_content` answers 204 with no body, so the route has no `output:` and what it runs returns `Unit`.
  next: Remove `output:`, or choose another status (agent)
  rule: route.output_unexpected
  help: https://jadpo.dev/docs/errors/route.output_unexpected

check failed: 1 error, 1 file
```
