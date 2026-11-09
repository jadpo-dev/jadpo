# `route.deadline_invalid`

A route's `deadline:` isn't a duration above zero and at most 60 seconds.

## Why Jadpo refuses it

A request that hangs holds a database connection, so every route has a
deadline, and none may run for more than 60 seconds. A value outside that
range, or one that isn't a duration, can't be applied, and Jadpo won't quietly
shorten it to one that can.

## What it looks like

main.jadpo:

```jadpo check project=e-deadline-invalid expect=route.deadline_invalid
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
        deadline: 90s
        run: add_memo(input)
    }
}
```

```text output
error: `90s` can't be a route's deadline
 --> main.jadpo:32:19
   |
32 |         deadline: 90s
   |                   ^^^ outside 1 ms to 60 s
  reason: A route's deadline is a duration above zero and at most 60 seconds; longer work answers `SuccessStatus.accepted` and carries on in a subscriber.
  next: Write a duration such as `30s`, or leave `deadline:` out for the 10-second default (agent)
  rule: route.deadline_invalid
  help: https://jadpo.dev/docs/errors/route.deadline_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A number with no unit is not a duration:

```jadpo check project=e-deadline-not-duration expect=route.deadline_invalid
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
        deadline: 30
        run: add_memo(input)
    }
}
```

```text output
error: `30` can't be a route's deadline
 --> main.jadpo:32:19
   |
32 |         deadline: 30
   |                   ^^ not a duration
  reason: A route's deadline is a duration above zero and at most 60 seconds; longer work answers `SuccessStatus.accepted` and carries on in a subscriber.
  next: Write a duration such as `30s`, or leave `deadline:` out for the 10-second default (agent)
  rule: route.deadline_invalid
  help: https://jadpo.dev/docs/errors/route.deadline_invalid

check failed: 1 error, 1 file
```

## How to fix it

**If the work fits inside 60 seconds**, write a duration that does:

```jadpo fragment
POST / {
    deadline: 30s
    ...
}
```

```jadpo check project=e-deadline-fixed hidden
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
        deadline: 30s
        run: add_memo(input)
    }
}
```

The route now answers 504 `deadline_exceeded` if it runs past 30 seconds.

**If the work takes longer**, answer before it is done, and carry it on in
a subscriber:

```jadpo fragment
POST / {
    status: SuccessStatus.accepted
    ...
}
```

```jadpo check project=e-deadline-accepted hidden
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
        status: SuccessStatus.accepted
        run: add_memo(input)
    }
}
```

The route answers 202 once `add_memo` commits, inside the default
10-second deadline.

## Related

- [Time limits](../language/07-http.md#time-limits)
- [Waiting for follow-on work](../language/07-http.md#waiting-for-follow-on-work)
