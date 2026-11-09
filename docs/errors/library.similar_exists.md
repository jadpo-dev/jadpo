# `library.similar_exists`

A function does the same as a library function that already exists.

## Why Jadpo refuses it

Agents working in parallel each write the helper they need, and two copies
of a helper drift apart when only one gets fixed. A function is like a
library function when its parameter types, result, `fails` list and body
are the same once each parameter and variable is renamed by its position: a
copy, however it is formatted or named.

Sometimes two copies are deliberate, because they're allowed to change on
their own. Only you know that, so this is a declinable error.

## What it looks like

```jadpo check project=e-similar expect=library.similar_exists
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

function cap_total(total: Stock) -> Stock {
    if (total > Stock(1000)) {
        return Stock(1000)
    }
    return total
}
```

```text output
error: A similar library function exists: `rounding.capped`
 --> main.jadpo:15:10
   |
15 | function cap_total(total: Stock) -> Stock {
   |          ^^^^^^^^^ same as `rounding.capped`
  reason: `cap_total` takes the same parameters, gives the same result and has the same body as `rounding.capped` in `main.jadpo`, apart from the names of its variables.
  next: Call `rounding.capped` instead, and remove `cap_total`, or write `decline library.similar_exists: "<why>"` above it if this is deliberate (agent)
  rule: library.similar_exists
  help: https://jadpo.dev/docs/errors/library.similar_exists

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Of two alike library functions, the later one in file order is reported:

```jadpo check project=e-similar-two expect=library.similar_exists
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

    /// The total, capped at a thousand.
    function limited(total: Stock) -> Stock {
        if (total > Stock(1000)) {
            return Stock(1000)
        }
        return total
    }
}
```

```text output
error: A similar library function exists: `rounding.capped`
 --> main.jadpo:15:14
   |
15 |     function limited(total: Stock) -> Stock {
   |              ^^^^^^^ same as `rounding.capped`
  reason: `limited` takes the same parameters, gives the same result and has the same body as `rounding.capped` in `main.jadpo`, apart from the names of its variables.
  next: Call `rounding.capped` instead, and remove `limited`, or write `decline library.similar_exists: "<why>"` above it if this is deliberate (agent)
  rule: library.similar_exists
  help: https://jadpo.dev/docs/errors/library.similar_exists

check failed: 1 error, 1 file
```

## How to fix it

**If the two should stay the same**, call the library function and remove
the copy:

```jadpo check project=e-similar-called
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

function basket_total(total: Stock) -> Stock {
    return rounding.capped(total)
}
```

There is now one function to change, and every caller gets the change.

**If they differ on purpose**, decline the rule above the function, saying
why:

```jadpo fragment
/// The basket total, capped for the till.
decline library.similar_exists: "Caps the basket for the till, which changes on its own; rounding.capped caps one line"
function cap_total(total: Stock) -> Stock {
    ...
}
```

```jadpo check project=e-similar-declined hidden
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
decline library.similar_exists: "Caps the basket for the till, which changes on its own; rounding.capped caps one line"
function cap_total(total: Stock) -> Stock {
    if (total > Stock(1000)) {
        return Stock(1000)
    }
    return total
}
```

The two can now change separately, and the reason is on the function for
a reviewer to read.

## Related

- [Keeping code as it is, with a reason](../language/01-project-layout-and-modules.md#keeping-code-as-it-is-with-a-reason)
- [Finding the helper that already exists](../language/01-project-layout-and-modules.md#finding-the-helper-that-already-exists)
