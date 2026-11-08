# `syntax.assertion_form`

A test checks a value with another language's assertion instead of `assert`.

## Why Jadpo refuses it

`assert_eq!`, `assertEquals`, `assertEqual`, `expect(...).toBe(...)` and
`assert.equal` are how other languages check a value. A Jadpo test has one
form, `assert condition`, and the condition is an ordinary expression, so
there are no assertion helpers to choose between.

## What it looks like

main.jadpo:

```jadpo check project=e-habit-assertion expect=syntax.assertion_form
type Count = Int {
    min: 0
}

function same(count: Count) -> Count {
    return count
}

test "same gives back its count" {
    assert_eq!(same(2), 2)
}
```

```text output
error: Jadpo writes `assert same(2) == 2`, not `assert_eq!(same(2), 2)`
 --> main.jadpo:10:5
   |
10 |     assert_eq!(same(2), 2)
   |     ^^^^^^^^^^^^^^^^^^^^^^ write `assert same(2) == 2`
  reason: `assert_eq!` checks a test's values in Rust, `assertEquals` in JUnit, `assertEqual` in Python's unittest, `expect(…).toBe(…)` in Jest and `assert.equal` in Node; a Jadpo test checks with `assert condition`.
  next: Write `assert same(2) == 2` (agent)
  rule: syntax.assertion_form
  help: https://jadpo.dev/docs/errors/syntax.assertion_form

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you meant to check a value**, write `assert` and the comparison:

```jadpo fragment
test "same gives back its count" {
    assert same(2) == 2
}
```

```jadpo check project=e-habit-assertion-fix hidden
type Count = Int {
    min: 0
}

function same(count: Count) -> Count {
    return count
}

test "same gives back its count" {
    assert same(2) == 2
}
```

The test now passes when `same(2)` gives `2`, and fails when it doesn't.

## Related

- [Assertions](../language/13-testing.md#assertions)
- [Writing a test](../language/13-testing.md#writing-a-test)
