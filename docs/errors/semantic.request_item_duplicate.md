# `semantic.request_item_duplicate`

A route test's `request { }` gives an item, or a placeholder in `params:`, twice.

## Why Jadpo refuses it

A request says once each thing it sends: its `method:`, its `path:`, the
placeholders in `params:`, its `input:`, its `idempotency_key:`, who it
is `as:`, and how they signed in with `sign_in:` and `strength:`. Given
twice, one of the two would be ignored, and a reader can't tell which,
so the test might prove something other than what it says. The second
line is usually a leftover from copying a request, and only you know
which value the test means, but either way the fix is to keep one.

## What it looks like

main.jadpo:

```jadpo check project=e-request-item-duplicate expect=semantic.request_item_duplicate
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

action tag_memo(memo_id: MemoId, input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos

    POST /{memo_id: MemoId} {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: tag_memo(memo_id, input)
    }
}

policy {
    actions: {
        tag_memo: [Access.public]
    }
}

test "a memo is tagged" {
    var response: Response = request {
        method: HttpMethod.post
        path: "/memos/{memo_id}"
        params: {
            memo_id: "0d9c1f1e-3b8a-4d6e-9f2a-6c5b4a3e2d1f"
        }
        input: AddMemo {
            text: "Call the plumber"
        }
        input: AddMemo {
            text: "Book the dentist"
        }
    }
    assert response.code == none
}
```

```text output
error: `input:` is given twice
 --> main.jadpo:48:9
   |
45 |         input: AddMemo {
   |         ----- first given here
48 |         input: AddMemo {
   |         ^^^^^ given again
  reason: A request gives each of `method:`, `path:`, `params:`, `input:`, `idempotency_key:`, `as:`, `sign_in:` and `strength:` once, so there is never a question of which counts.
  next: Remove one `input:` (agent)
  rule: semantic.request_item_duplicate
  help: https://jadpo.dev/docs/errors/semantic.request_item_duplicate

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A placeholder given twice in `params:` is refused the same way:

```jadpo check project=e-request-item-duplicate-params expect=semantic.request_item_duplicate
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

action tag_memo(memo_id: MemoId, input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos

    POST /{memo_id: MemoId} {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: tag_memo(memo_id, input)
    }
}

policy {
    actions: {
        tag_memo: [Access.public]
    }
}

test "a memo is tagged" {
    var response: Response = request {
        method: HttpMethod.post
        path: "/memos/{memo_id}"
        params: {
            memo_id: "0d9c1f1e-3b8a-4d6e-9f2a-6c5b4a3e2d1f"
            memo_id: "7a41c2d0-5e9b-4f83-8c16-2b0d9e7f4a35"
        }
        input: AddMemo {
            text: "Call the plumber"
        }
    }
    assert response.code == none
}
```

```text output
error: `memo_id` is given twice in `params:`
 --> main.jadpo:44:13
   |
43 |             memo_id: "0d9c1f1e-3b8a-4d6e-9f2a-6c5b4a3e2d1f"
   |             ------- first given here
44 |             memo_id: "7a41c2d0-5e9b-4f83-8c16-2b0d9e7f4a35"
   |             ^^^^^^^ given again
  reason: A request gives each placeholder's value once, so there is never a question of which counts.
  next: Remove one `memo_id:` (agent)
  rule: semantic.request_item_duplicate
  help: https://jadpo.dev/docs/errors/semantic.request_item_duplicate

check failed: 1 error, 1 file
```

## How to fix it

**If one of the two is a leftover**, remove it, keeping the value the
test means. In `main.jadpo`:

```jadpo fragment
        input: AddMemo {
            text: "Call the plumber"
        }
```

```jadpo check project=e-request-item-duplicate-fix-1 hidden
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

action tag_memo(memo_id: MemoId, input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos

    POST /{memo_id: MemoId} {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: tag_memo(memo_id, input)
    }
}

policy {
    actions: {
        tag_memo: [Access.public]
    }
}

test "a memo is tagged" {
    var response: Response = request {
        method: HttpMethod.post
        path: "/memos/{memo_id}"
        params: {
            memo_id: "0d9c1f1e-3b8a-4d6e-9f2a-6c5b4a3e2d1f"
        }
        input: AddMemo {
            text: "Call the plumber"
        }
    }
    assert response.code == none
}
```

The request now sends one input, so the test checks exactly the request
it shows.

**If the test means to send both**, make each its own request, in its own
`var`, and assert on each response.

## Related

- [Route tests](../language/13-testing.md#route-tests)
- [Signing in for a route test](../language/13-testing.md#signing-in-for-a-route-test)
