# `decline.unused`

A `decline` line names a rule that no longer fires on its declaration.

## Why Jadpo refuses it

A decline answers one finding on one declaration. Once the finding has gone,
because it was fixed or the compiler no longer reports it, the line says
something that isn't true, and a reader goes looking for a problem that isn't
there. The error comes once every other finding is answered, since another
error can stop a rule from firing.

## What it looks like

main.jadpo:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the finding has gone**, remove the `decline` line:

```jadpo fragment
/// The basket total, capped for the till.
function cap_total(total: Stock) -> Stock {
    ...
}
```

```jadpo check project=e-decline-unused-fix-1 hidden
type Stock = Int {
    min: 0
}

/// The basket total, capped for the till.
function cap_total(total: Stock) -> Stock {
    if (total > Stock(1000)) {
        return Stock(1000)
    }
    return total
}
```

Nothing in the file now claims a finding that isn't there.

**If the line is on the wrong declaration**, move it to the one the finding
is on. A decline covers its one rule on the declaration below it:

```jadpo fragment
decline library.similar_exists: "Caps the basket for the till, which changes on its own"
function cap_total(total: Stock) -> Stock {
    ...
}
```

```jadpo check project=e-decline-unused-fix-2 hidden
type Stock = Int {
    min: 0
}

library rounding {
    /// The amount, capped at a thousand.
    function capped(amount: Stock) -> Stock {
        if (amount > Stock(1000)) {
            return Stock(1000)
        }
        return amount
    }
}

/// The basket total, capped for the till.
decline library.similar_exists: "Caps the basket for the till, which changes on its own"
function cap_total(total: Stock) -> Stock {
    if (total > Stock(1000)) {
        return Stock(1000)
    }
    return total
}
```

The decline now answers the finding on `cap_total`.

## Related

- [Writing a decline](../language/01-project-layout-and-modules.md#writing-a-decline)
- [`decline.no_reason`](decline.no_reason.md)
