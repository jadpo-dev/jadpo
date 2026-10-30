# `route.prefix_api`

A group's prefix starts with its API's, so each API's routes, OpenAPI
document and record stay its own:

```jadpo check project=e-prefix-api file=app.jadpo
application {
    apis: {
        main: {
            prefix: /v1
        }
    }
}
```

```jadpo check project=e-prefix-api file=domains/memos/actions/add_memo.jadpo hidden
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

```jadpo check project=e-prefix-api file=domains/memos/types/memo.jadpo hidden
type Memo = Text {
    max_length: 200
}
```

```jadpo check project=e-prefix-api file=domains/memos/types/memo_view.jadpo hidden
use memos/types { Memo }

type MemoView = Output {
    text: Memo
}
```

```jadpo check project=e-prefix-api file=domains/memos/routes/memos.jadpo expect=route.prefix_api
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
error: `Memos`'s prefix doesn't start with API `main`'s
 --> domains/memos/routes/memos.jadpo:5:13
  |
5 |     prefix: /memos
  |             ^^^^^^ outside `/v1`
  reason: A group's prefix starts with its API's (`/v1` for `main`), so each API's routes, OpenAPI document and record stay its own.
  next: Start the prefix with `/v1`, or name the group's API with `api:` (agent)
  rule: route.prefix_api
  help: https://jadpo.dev/docs/errors/route.prefix_api

check failed: 1 error, 5 files
```
