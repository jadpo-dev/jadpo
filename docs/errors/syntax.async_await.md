# `syntax.async_await`

A call gives its result when it returns, so nothing is marked `async` or `await`.

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
  help: docs/language/03-logic.html#syntax-async-await

check failed: 1 error, 1 file
```
