# `syntax.variable_keyword`

A variable is declared with `let`, `const` or `val`, words from other languages, where Jadpo writes `var`.

## Why Jadpo refuses it

Jadpo has one word for a variable: `var name: Type = value`, which never
changes, or `var mut` when it can. Other languages' words mean different
things from one language to the next, such as whether the value can
change, so Jadpo doesn't accept them and guess. The error names the word
to write, and the rest of the file is checked as if it were written.

## What it looks like

main.jadpo:

```jadpo check project=e-habit-let expect=syntax.variable_keyword
type Count = Int {
    min: 0
}

function same(count: Count) -> Count {
    let kept: Count = count
    return kept
}
```

```text output
error: Jadpo writes `var`, not `let`
 --> main.jadpo:6:5
  |
6 |     let kept: Count = count
  |     ^^^ write `var`
  reason: `let` declares a variable in JavaScript, TypeScript, Swift and Rust; a Jadpo variable is `var name: Type = value`, or `var mut` when it can change.
  next: Use `var` instead of `let` (agent)
  rule: syntax.variable_keyword
  help: https://jadpo.dev/docs/errors/syntax.variable_keyword

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`const` and `val` inside a body are the same error, and so is Rust's
`let mut`, whose Jadpo form is `var mut`. At the top of a file, outside any
function, `const` declares a fixed value, which Jadpo writes `constant`:

main.jadpo:

```jadpo check project=e-habit-const expect=syntax.variable_keyword
type Count = Int {}

const limit: Count = 3

function lim() -> Count {
    return limit
}
```

```text output
error: Jadpo writes `constant`, not `const`
 --> main.jadpo:3:1
  |
3 | const limit: Count = 3
  | ^^^^^ write `constant`
  reason: `const` declares a value that never changes in JavaScript, TypeScript, Rust, Go and C#; at the top of a Jadpo file a fixed value is `constant name: Type = value`.
  next: Use `constant` instead of `const` (agent)
  rule: syntax.variable_keyword
  help: https://jadpo.dev/docs/errors/syntax.variable_keyword

check failed: 1 error, 1 file
```

## How to fix it

**If it is a variable in a body**, write `var`, or `var mut` if you assign
it again later:

```jadpo fragment
    var kept: Count = count
```

```jadpo check project=e-habit-let-fix hidden
type Count = Int {
    min: 0
}

function same(count: Count) -> Count {
    var kept: Count = count
    return kept
}
```

`kept` is a variable that never changes, and the function checks.

**If it is a fixed value at the top of a file**, write `constant`:

```jadpo fragment
constant limit: Count = 3
```

```jadpo check project=e-habit-const-fix hidden
type Count = Int {}

constant limit: Count = 3

function lim() -> Count {
    return limit
}
```

`limit` is a constant: a fixed value worked out while compiling, which
`lim` reads by name.

## Related

- [Variables](../language/03-logic.md#variables)
- [Constants](../language/03-logic.md#constants)
- [Habits from other languages](../language/03-logic.md#habits-from-other-languages)
