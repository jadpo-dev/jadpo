# `syntax.function_keyword`

A function is declared with `fn`, `func`, `def` or `fun` instead of `function`.

## Why Jadpo refuses it

`fn`, `func`, `def` and `fun` each start a function in another language.
Jadpo starts one with `function`, or `action` when it reads or writes stored
data, so the first word of a declaration says which of the two it is.

## What it looks like

main.jadpo:

```jadpo check project=e-habit-fn expect=syntax.function_keyword
type Count = Int {
    min: 0
}

fn same(count: Count) -> Count {
    return count
}
```

```text output
error: Jadpo writes `function`, not `fn`
 --> main.jadpo:5:1
  |
5 | fn same(count: Count) -> Count {
  | ^^ write `function`
  reason: `fn` declares a function in Rust; Jadpo starts one with `function`, or `action` when it reads or writes stored data.
  next: Use `function` instead of `fn` (agent)
  rule: syntax.function_keyword
  help: https://jadpo.dev/docs/errors/syntax.function_keyword

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If it only works out a value**, write `function`:

```jadpo fragment
function same(count: Count) -> Count {
    return count
}
```

```jadpo check project=e-habit-fn-fix-1 hidden
type Count = Int {
    min: 0
}

function same(count: Count) -> Count {
    return count
}
```

`same` is now a function, which can't read or write stored data.

**If it reads or writes stored data**, write `action`:

```jadpo fragment
action same(count: Count) -> Count {
    return count
}
```

```jadpo check project=e-habit-fn-fix-2 hidden
type Count = Int {
    min: 0
}

action same(count: Count) -> Count {
    return count
}
```

`same` is now an action, and can read and write stored data.

## Related

- [Functions and actions](../language/03-logic.md#functions-and-actions)
- [Habits from other languages](../language/03-logic.md#habits-from-other-languages)
