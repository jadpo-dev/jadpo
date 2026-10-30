# `syntax.assertion_form`

A test checks with `assert condition`: Rust's `assert_eq!`, JUnit's `assertEquals`, Python's `assertEqual`, Jest's `expect(...).toBe(...)` and Node's `assert.equal` are habits from other languages, one error each whose edit writes the `assert`.

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
