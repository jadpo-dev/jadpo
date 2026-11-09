# `decline.no_reason`

A `decline` line gives no reason, or only a blank one.

## Why Jadpo refuses it

A `decline` leaves a finding in the code on purpose, and it is reviewed
like the code it sits on. The reason is what a reviewer judges: whether this
really is the exception. Without one, the line says only that someone turned
a warning off, and Jadpo can't write it for you, because only whoever made
the choice knows why.

## What it looks like

main.jadpo:

```jadpo check project=e-decline-reason expect=decline.no_reason
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

decline library.similar_exists: ""
function cap_total(total: Stock) -> Stock {
    if (total > Stock(1000)) {
        return Stock(1000)
    }
    return total
}
```

```text output
error: This decline gives no reason
 --> main.jadpo:15:1
   |
15 | decline library.similar_exists: ""
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ reason needed
  reason: A decline is reviewed like the code it sits on: its reason says why the finding is right to leave, so a reviewer can agree or not.
  next: Write why in the quotes: `decline library.similar_exists: "<why>"` (agent)
  rule: decline.no_reason
  help: https://jadpo.dev/docs/errors/decline.no_reason

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A line with no reason at all is refused the same way:

main.jadpo:

```jadpo check project=e-decline-reason-missing expect=decline.no_reason
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

decline library.similar_exists
function cap_total(total: Stock) -> Stock {
    if (total > Stock(1000)) {
        return Stock(1000)
    }
    return total
}
```

```text output
error: This decline gives no reason
 --> main.jadpo:15:1
   |
15 | decline library.similar_exists
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ reason needed
  reason: A decline is reviewed like the code it sits on: its reason says why the finding is right to leave, so a reviewer can agree or not.
  next: Write why in the quotes: `decline library.similar_exists: "<why>"` (agent)
  rule: decline.no_reason
  help: https://jadpo.dev/docs/errors/decline.no_reason

check failed: 1 error, 1 file
```

## How to fix it

**If the finding is right to leave**, say why between the quotes:

```jadpo fragment
decline library.similar_exists: "Caps the basket for the till, which changes on its own"
function cap_total(total: Stock) -> Stock {
    ...
}
```

```jadpo check project=e-decline-reason-fix-1 hidden
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

decline library.similar_exists: "Caps the basket for the till, which changes on its own"
function cap_total(total: Stock) -> Stock {
    if (total > Stock(1000)) {
        return Stock(1000)
    }
    return total
}
```

The finding stays declined, with a reason a reviewer can agree with or not.

**If it isn't**, remove the `decline` line and do what the finding asks,
here by using the library's function:

```jadpo fragment
function cap_total(total: Stock) -> Stock {
    return rounding.capped(total)
}
```

```jadpo check project=e-decline-reason-fix-2 hidden
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
    return rounding.capped(total)
}
```

The two copies of the cap are now one, so there is nothing left to decline.

## Related

- [Writing a decline](../language/01-project-layout-and-modules.md#writing-a-decline)
- [`decline.unused`](decline.unused.md)
