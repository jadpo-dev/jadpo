# `route.api_unknown`

A route group names an API the application doesn't declare.

## Why Jadpo refuses it

A route group belongs to one API, which decides its prefix, its sign-in and
the OpenAPI document it appears in. An API `app.jadpo` doesn't declare has
none of those, so Jadpo can't tell where the group's routes belong. With
nothing declared there is one API, `main`, at `/`.

## What it looks like

main.jadpo:

```jadpo check project=e-api-unknown expect=route.api_unknown
type Memo = Text {
    max_length: 200
}

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

policy {
    actions: {
        add_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos
    api: admin

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

```text output
error: `Memos` belongs to API `admin`, which isn't declared
 --> main.jadpo:27:10
   |
27 |     api: admin
   |          ^^^^^ not declared
  reason: With no `apis:` in `app.jadpo` there is one API, `main`, at `/`.
  next: Declare `admin` under `apis:` in `app.jadpo` (agent)
  rule: route.api_unknown
  help: https://jadpo.dev/docs/errors/route.api_unknown

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`api:` takes the name itself, not text:

```jadpo check project=e-api-unknown-text expect=route.api_unknown
type Memo = Text {
    max_length: 200
}

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

policy {
    actions: {
        add_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos
    api: "admin"

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

```text output
error: `api:` names an API by its name
 --> main.jadpo:27:10
   |
27 |     api: "admin"
   |          ^^^^^^^ not an API's name
  reason: A group names the API it belongs to as `app.jadpo` declares it under `apis:`, such as `api: admin`.
  next: Write the API's name (agent)
  rule: route.api_unknown
  help: https://jadpo.dev/docs/errors/route.api_unknown

check failed: 1 error, 1 file
```

## How to fix it

The question is whether the group belongs in an API of its own.

**If it does**, declare the API in `app.jadpo`, and start the group's prefix
with the API's. A project with an `app.jadpo` keeps its routes under
`domains/`, in a file named after the group:

```jadpo fragment
application {
    apis: {
        admin: {
            prefix: /admin
        }
        ...
    }
}
```

```jadpo check project=e-api-unknown-declared file=app.jadpo hidden
application {
    apis: {
        admin: {
            prefix: /admin
        }
    }
}
```

```jadpo fragment
routes AdminMemos {
    prefix: /admin/memos
    api: admin
    ...
}
```

```jadpo check project=e-api-unknown-declared file=domains/memos/routes/admin_memos.jadpo hidden
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

```jadpo check project=e-api-unknown-declared file=domains/memos/actions/add_memo.jadpo hidden
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

```jadpo check project=e-api-unknown-declared file=domains/memos/types/memo.jadpo hidden
type Memo = Text {
    max_length: 200
}
```

```jadpo check project=e-api-unknown-declared file=domains/memos/types/memo_view.jadpo hidden
use memos/types { Memo }

type MemoView = Output {
    text: Memo
}
```

```jadpo check project=e-api-unknown-declared file=policy.jadpo hidden
use memos

policy {
    actions: {
        memos.add_memo: [Access.public]
    }
}
```

`POST /admin/memos` is now part of `admin`, with its own OpenAPI document.

**If it belongs to the default API**, remove the `api:` line:

```jadpo fragment
routes Memos {
    prefix: /memos
    ...
}
```

```jadpo check project=e-api-unknown-main hidden
type Memo = Text {
    max_length: 200
}

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

policy {
    actions: {
        add_memo: [Access.public]
    }
}

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

The group is now part of `main`, at `/memos`.

## Related

- [APIs and versions](../language/07-http.md#apis-and-versions)
- [Route groups](../language/07-http.md#route-groups)
