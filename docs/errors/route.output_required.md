# `route.output_required`

A route shows its contract without opening what it runs:

```jadpo check project=e-output-required expect=route.output_required
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
        run: add_memo(input)
    }
}
```

```text output
error: `POST /memos` has no `output:`
 --> main.jadpo:24:5
   |
24 |     POST / {
   |     ^^^^^^ no `output:`
  reason: A route shows its contract without opening what it runs: `output:` names what it sends, exactly what `add_memo` returns (`MemoView`).
  next: Add `output: MemoView` (agent)
  rule: route.output_required
  help: docs/errors/route.output_required.md

check failed: 1 error, 1 file
```

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

routes Memos {
    prefix: /memos

    POST /checks {
        input: AddMemo
        run: check_memo(input)
    }
}
```

```text output
error: `POST /memos/checks` has no `output:`
 --> main.jadpo:16:5
   |
16 |     POST /checks {
   |     ^^^^^^^^^^^^ no `output:`
  reason: A route shows its contract without opening what it runs: `check_memo` returns `Unit`, so the route sends nothing, which `status: SuccessStatus.no_content` says, answering 204 with no body.
  next: Add `status: SuccessStatus.no_content` (agent)
  rule: route.output_required
  help: docs/errors/route.output_required.md

check failed: 1 error, 1 file
```
