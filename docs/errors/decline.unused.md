# `decline.unused`

A decline whose rule no longer fires on its declaration: the finding was
fixed, or the compiler grew smarter. Its edit removes the line. It is
reported once every other finding is answered, since another error can
stop a rule from firing.

```jadpo check project=e-decline-unused expect=decline.unused
type Stock = Int {
    min: 0
}

/// The basket total, capped for the till.
decline library.similar_exists: "Caps the basket, not a line"
function cap_total(total: Stock) -> Stock {
    if (total > Stock(1000)) {
        return Stock(1000)
    }
    return total
}
```

```text output
error: Nothing to decline: `library.similar_exists` doesn't fire here
 --> main.jadpo:6:9
  |
6 | decline library.similar_exists: "Caps the basket, not a line"
  |         ^^^^^^^^^^^^^^^^^^^^^^ no such finding here
  reason: This line declines `library.similar_exists` on `cap_total`, but `jadpo check` finds no `library.similar_exists` there now, so the decline has outlived its reason.
  next: Remove the `decline` line (agent)
  rule: decline.unused
  help: https://jadpo.dev/docs/errors/decline.unused

check failed: 1 error, 1 file
```
