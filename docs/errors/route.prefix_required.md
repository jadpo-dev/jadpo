# `route.prefix_required`

Every group has a prefix, which names its file:

```jadpo check project=e-prefix-required expect=route.prefix_required
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
    POST / {
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

```text output
error: `Memos` has no prefix
 --> main.jadpo:21:8
   |
21 | routes Memos {
   |        ^^^^^ no `prefix:`
  reason: Every route in a group follows the group's prefix, and the prefix's fixed segments give the file its name, so a URL from a log leads to one file.
  next: Add `prefix: /<path>` as the group's first line (agent)
  rule: route.prefix_required
  help: https://jadpo.dev/docs/errors/route.prefix_required

check failed: 1 error, 1 file
```
