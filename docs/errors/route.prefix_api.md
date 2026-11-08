# `route.prefix_api`

A route group's prefix doesn't start with its API's prefix.

## Why Jadpo refuses it

A group's routes are part of its API, and each API's routes, OpenAPI document
and record stay its own. A prefix outside the API's would put the group's URLs
somewhere else, so a URL would no longer say which API it belongs to.

## What it looks like

app.jadpo:

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

```jadpo check project=e-prefix-api file=policy.jadpo hidden
use memos

policy {
    actions: {
        memos.add_memo: [Access.public]
    }
}
```

domains/memos/routes/memos.jadpo:

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

check failed: 1 error, 6 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the group belongs to this API**, start its prefix with the API's. The
file is named after the group, so rename both, here to `V1Memos` in
`v1_memos.jadpo`:

```jadpo fragment
routes V1Memos {
    prefix: /v1/memos
    ...
}
```

```jadpo check project=e-prefix-api-v1 file=domains/memos/routes/v1_memos.jadpo hidden
use memos/types { MemoView }
use memos/actions { AddMemo, add_memo }

routes V1Memos {
    prefix: /v1/memos

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

```jadpo check project=e-prefix-api-v1 file=app.jadpo hidden
application {
    apis: {
        main: {
            prefix: /v1
        }
    }
}
```

```jadpo check project=e-prefix-api-v1 file=domains/memos/actions/add_memo.jadpo hidden
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

```jadpo check project=e-prefix-api-v1 file=domains/memos/types/memo.jadpo hidden
type Memo = Text {
    max_length: 200
}
```

```jadpo check project=e-prefix-api-v1 file=domains/memos/types/memo_view.jadpo hidden
use memos/types { Memo }

type MemoView = Output {
    text: Memo
}
```

```jadpo check project=e-prefix-api-v1 file=policy.jadpo hidden
use memos

policy {
    actions: {
        memos.add_memo: [Access.public]
    }
}
```

The route is now `POST /v1/memos`, in `main`.

**If it belongs to another API**, name that API with `api:`, and start the
prefix with that API's, renaming the group and its file to match:

```jadpo fragment
routes AdminMemos {
    prefix: /admin/memos
    api: admin
    ...
}
```

```jadpo check project=e-prefix-api-admin file=domains/memos/routes/admin_memos.jadpo hidden
use memos/types { MemoView }
use memos/actions { AddMemo, add_memo }

routes AdminMemos {
    prefix: /admin/memos
    api: admin

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

```jadpo check project=e-prefix-api-admin file=app.jadpo hidden
application {
    apis: {
        main: {
            prefix: /v1
        }
        admin: {
            prefix: /admin
        }
    }
}
```

```jadpo check project=e-prefix-api-admin file=domains/memos/actions/add_memo.jadpo hidden
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

```jadpo check project=e-prefix-api-admin file=domains/memos/types/memo.jadpo hidden
type Memo = Text {
    max_length: 200
}
```

```jadpo check project=e-prefix-api-admin file=domains/memos/types/memo_view.jadpo hidden
use memos/types { Memo }

type MemoView = Output {
    text: Memo
}
```

```jadpo check project=e-prefix-api-admin file=policy.jadpo hidden
use memos

policy {
    actions: {
        memos.add_memo: [Access.public]
    }
}
```

The route is now `POST /admin/memos`, in `admin`'s document and record.

## Related

- [APIs and versions](../language/07-http.md#apis-and-versions)
- [Route groups](../language/07-http.md#route-groups)
