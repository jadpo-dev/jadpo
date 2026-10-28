# `record.out_of_date`

Each API with routes has a committed record, which `jadpo api update`
writes. Here the project has routes and no `api/main.jadpo_record` yet:

```jadpo check project=e-record file=app.jadpo hidden
application {}
```

```jadpo check project=e-record file=policy.jadpo hidden
use memos

policy {
    actions: {
        memos.add_memo: [Access.public]
    }
}
```

```jadpo check project=e-record file=domains/memos/types/memo.jadpo hidden
type Memo = Text {
    max_length: 200
}
```

```jadpo check project=e-record file=domains/memos/types/memo_view.jadpo hidden
use memos/types { Memo }

type MemoView = Output {
    text: Memo
}
```

```jadpo check project=e-record file=domains/memos/actions/add_memo.jadpo hidden
use memos/types { Memo, MemoView }

type AddMemo = Input {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}
```

```jadpo check project=e-record file=domains/memos/routes/memos.jadpo expect=record.out_of_date
use memos/types { MemoView }
use memos/actions { AddMemo, add_memo }

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

```text output
error: `api/main.jadpo_record` is missing
 --> api/main.jadpo_record
  reason: Each API with routes has a committed record of its routes, fields and failure codes, so every change to API `main` shows as a diff of one file.
  next: Run `jadpo api update`, and commit the record with the code (agent)
  rule: record.out_of_date
  help: docs/language/18-records-and-manifest.html#record-out-of-date

check failed: 1 error, 6 files
```
