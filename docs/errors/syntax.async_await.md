# `syntax.async_await`

A function or a call is marked `async` or `await`.

## Why Jadpo refuses it

`async` and `await` mark code that waits for a result in JavaScript,
TypeScript, Python, Rust, C# and Swift. A Jadpo call always gives its result
when it returns, so there is nothing to mark, and a reader never has to ask
whether a call was waited for.

## What it looks like

main.jadpo:

```jadpo check project=e-habit-await expect=syntax.async_await
type Count = Int {
    min: 0
}

function same(count: Count) -> Count {
    return count
}

function same_twice(count: Count) -> Count {
    return await same(count)
}
```

```text output
error: Jadpo has no `await`
 --> main.jadpo:10:12
   |
10 |     return await same(count)
   |            ^^^^^ remove `await`
  reason: `async` and `await` mark code that waits for a result in JavaScript, TypeScript, Python, Rust, C# and Swift; a Jadpo call always gives its result when it returns, so there is nothing to mark.
  next: Remove `await` (agent)
  rule: syntax.async_await
  help: https://jadpo.dev/docs/errors/syntax.async_await

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you meant to use the call's result**, remove `await`:

```jadpo fragment
function same_twice(count: Count) -> Count {
    return same(count)
}
```

```jadpo check project=e-habit-await-fix hidden
type Count = Int {
    min: 0
}

function same(count: Count) -> Count {
    return count
}

function same_twice(count: Count) -> Count {
    return same(count)
}
```

`same_twice` now returns what `same` gives back. An `async` before a
declaration is removed the same way.

## Related

- [Calling](../language/03-logic.md#calling)
- [Habits from other languages](../language/03-logic.md#habits-from-other-languages)
