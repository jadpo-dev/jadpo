# `syntax.increment_operator`

A variable is changed with `++`, `--`, `+=`, `-=` or `*=`.

## Why Jadpo refuses it

`++`, `--`, `+=`, `-=` and `*=` change a variable in place in JavaScript,
Java, C#, Go, Kotlin, Swift and Python. Jadpo assigns a `var mut` with `=`,
and a named type converts the result back with its constructor, so every
change to a value of that type goes through the type's own rules.

## What it looks like

main.jadpo:

```jadpo check project=e-habit-increment expect=syntax.increment_operator
type Count = Int {
    min: 0
}

function bumped(count: Count) -> Count {
    var mut next: Count = count
    next++
    return next
}
```

```text output
error: Jadpo writes `next = next + 1`, not `next++`
 --> main.jadpo:7:9
  |
7 |     next++
  |         ^^ write `next = next + 1`
  reason: `++`, `--`, `+=`, `-=` and `*=` change a variable in place in JavaScript, Java, C#, Go, Kotlin, Swift and Python; Jadpo assigns a `var mut` with `=`, and a named type converts the result back with its constructor, as in `count = Count(count + 1)`.
  next: Write `next = next + 1` (agent)
  rule: syntax.increment_operator
  help: https://jadpo.dev/docs/errors/syntax.increment_operator

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you meant to change the variable**, assign to it with `=`, converting a
named type back with its constructor:

```jadpo fragment
function bumped(count: Count) -> Count {
    var mut next: Count = count
    next = Count(next + 1)
    return next
}
```

```jadpo check project=e-habit-increment-fix hidden
type Count = Int {
    min: 0
}

function bumped(count: Count) -> Count {
    var mut next: Count = count
    next = Count(next + 1)
    return next
}
```

`next` now holds one more than `count`, and `Count(...)` checks the result
is still a valid `Count`.

## Related

- [Variables](../language/03-logic.md#variables)
- [Expressions and operators](../language/03-logic.md#expressions-and-operators)
