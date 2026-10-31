# `record.out_of_date`

A record file that Jadpo writes from your code is missing, or no longer matches the code.

## Why Jadpo refuses it

A record is a file Jadpo writes from your code and you commit with it.
Each API with routes has one, `api/<api>.jadpo_record`, holding its
routes, fields and failure codes. Because it is committed, every change
to what the API's clients see shows in review as a diff of that one file,
rather than spread through the code.

When the code changes and the record doesn't, the record no longer says
what the API does. `jadpo check` fails until the record is written again,
and names the command that writes it. `jadpo test` and `jadpo dev` still
run.

## What it looks like

A project whose routes have no record yet:

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

domains/memos/routes/memos.jadpo:

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
  help: https://jadpo.dev/docs/errors/record.out_of_date

check failed: 1 error, 6 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the record is an API's**, run the command the error names, in the
project's folder:

```sh
jadpo api update
```

```text
Wrote api/main.jadpo_record
    version  3aef2a9774e1

next: git diff api/  shows what changed for the API's clients
```

The record now matches the routes. Commit it with the code, so the
review shows what changed for the API's clients. Never edit it by hand:
a record Jadpo couldn't have written, such as one with a comment added,
is [`record.edited`](../errors/record.edited.md).

**If it is another record**, such as `access.jadpo_record` or
`network.jadpo_record`, bring every record up to date at once:

```sh
jadpo records update
```

Each record is written again from the code. Commit them with it.

## Related

- [How `jadpo check` keeps records up to date](../language/18-records-and-manifest.md#how-jadpo-check-keeps-records-up-to-date)
- [`jadpo records`](../language/18-records-and-manifest.md#jadpo-records)
- [Checking a record](../language/18-records-and-manifest.md#checking-a-record)
- [Record files](../language/18-records-and-manifest.md#record-files)
