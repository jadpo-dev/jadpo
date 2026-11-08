# `syntax.try_catch`

A failure is handled with `try`, `catch`, `finally` or `?` instead of `attempt` or a `match`.

## Why Jadpo refuses it

`try` and `catch` wrap a stretch of code in several other languages, and
hide which of its calls can fail. Jadpo marks each call that can fail where
it is made: `attempt` passes its failures on, and a `match` on the call
handles each one. What should happen to a failure is yours to say, so a
`try` around a block has no automatic edit.

## What it looks like

main.jadpo:

```jadpo check project=e-habit-try expect=syntax.try_catch
type Count = Int {
    min: 0
}

type CountMissing = Failure {
    kind: FailureKind.not_found
}

function counted_or_missing(count: Count) fails CountMissing -> Count {
    if (count == 0) {
        reject CountMissing
    }
    return count
}

function first(count: Count) fails CountMissing -> Count {
    var result: Count = try counted_or_missing(count)
    return result
}
```

```text output
error: Jadpo writes `attempt`, not `try`
 --> main.jadpo:17:25
   |
17 |     var result: Count = try counted_or_missing(count)
   |                         ^^^ write `attempt`
  reason: `try` marks a call that can fail in Swift; Jadpo writes `attempt`, which passes the call's failures on to the caller.
  next: Use `attempt` instead of `try` (agent)
  rule: syntax.try_catch
  help: https://jadpo.dev/docs/errors/syntax.try_catch

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A `try` around a block is the same error, and has no edit. A `catch` or a
`finally` after it is reported too, each on its own line:

main.jadpo:

```jadpo check project=e-habit-try-block expect=syntax.try_catch
type Count = Int {
    min: 0
}

type CountMissing = Failure {
    kind: FailureKind.not_found
}

function counted_or_missing(count: Count) fails CountMissing -> Count {
    if (count == 0) {
        reject CountMissing
    }
    return count
}

function first(count: Count) fails CountMissing -> Count {
    try {
        var result: Count = counted_or_missing(count)
        return result
    }
}
```

```text output
error: Jadpo has no `try` block
 --> main.jadpo:17:5
   |
17 |     try {
   |     ^^^ not Jadpo
  reason: `try`, `catch` and `finally` wrap code that can fail in JavaScript, Java, C#, Kotlin and Python; Jadpo marks each call that can fail where it is made: `attempt` passes its failures on, and a `match` on the call handles each one.
  next: Mark each call that can fail with `attempt`, or handle its failures with `match call(...) { ... }` (agent)
  rule: syntax.try_catch
  help: https://jadpo.dev/docs/errors/syntax.try_catch

check failed: 1 error, 1 file
```

Rust's `?` after a call is the same error too:

main.jadpo:

```jadpo check project=e-habit-try-question expect=syntax.try_catch
type Count = Int {
    min: 0
}

type CountMissing = Failure {
    kind: FailureKind.not_found
}

function counted_or_missing(count: Count) fails CountMissing -> Count {
    if (count == 0) {
        reject CountMissing
    }
    return count
}

function first(count: Count) fails CountMissing -> Count {
    var result: Count = counted_or_missing(count)?
    return result
}
```

```text output
error: Jadpo writes `attempt`, not `?`
 --> main.jadpo:17:50
   |
17 |     var result: Count = counted_or_missing(count)?
   |                                                  ^ write `attempt` before the call
  reason: `?` after a call passes its failure on in Rust; Jadpo writes `attempt` before the call.
  next: Write `attempt counted_or_missing(count)` (agent)
  rule: syntax.try_catch
  help: https://jadpo.dev/docs/errors/syntax.try_catch

check failed: 1 error, 1 file
```

## How to fix it

What should happen when the call fails?

**If the caller should deal with it**, write `attempt` before the call:

```jadpo fragment
function first(count: Count) fails CountMissing -> Count {
    var result: Count = attempt counted_or_missing(count)
    return result
}
```

```jadpo check project=e-habit-try-fix-1 hidden
type Count = Int {
    min: 0
}

type CountMissing = Failure {
    kind: FailureKind.not_found
}

function counted_or_missing(count: Count) fails CountMissing -> Count {
    if (count == 0) {
        reject CountMissing
    }
    return count
}

function first(count: Count) fails CountMissing -> Count {
    var result: Count = attempt counted_or_missing(count)
    return result
}
```

If `counted_or_missing` fails, `first` now stops there and passes
`CountMissing` on to its caller.

**If this function should deal with it**, `match` on the call, with an arm
for success and one for each failure:

```jadpo fragment
function first(count: Count) -> Count {
    return match counted_or_missing(count) {
        success(found) -> found
        CountMissing -> 0
    }
}
```

```jadpo check project=e-habit-try-fix-2 hidden
type Count = Int {
    min: 0
}

type CountMissing = Failure {
    kind: FailureKind.not_found
}

function counted_or_missing(count: Count) fails CountMissing -> Count {
    if (count == 0) {
        reject CountMissing
    }
    return count
}

function first(count: Count) -> Count {
    return match counted_or_missing(count) {
        success(found) -> found
        CountMissing -> 0
    }
}
```

`first` now gives `0` when the count is missing, and can't fail.

## Related

- [Passing a failure on: `attempt`](../language/04-failures.md#passing-a-failure-on-attempt)
- [Handling a failure in `match`](../language/04-failures.md#handling-a-failure-in-match)
