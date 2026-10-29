# `route.deadline_invalid`

A deadline is above zero and at most 60 seconds:

```jadpo check project=e-deadline-invalid expect=route.deadline_invalid
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
        deadline: 90s
        run: add_memo(input)
    }
}
```

```text output
error: `90s` can't be a route's deadline
 --> main.jadpo:27:19
   |
27 |         deadline: 90s
   |                   ^^^ outside 1 ms to 60 s
  reason: A route's deadline is a duration above zero and at most 60 seconds; longer work answers `SuccessStatus.accepted` and carries on in a subscriber.
  next: Write a duration such as `30s`, or leave `deadline:` out for the 10-second default (agent)
  rule: route.deadline_invalid
  help: docs/errors/route.deadline_invalid.md

check failed: 1 error, 1 file
```
