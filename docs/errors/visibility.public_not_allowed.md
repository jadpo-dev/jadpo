# `visibility.public_not_allowed`

A declaration that other domains can't use is marked `public`.

## Why Jadpo refuses it

`public` is a promise to keep offering a declaration to the rest of the
project, so it means something only on what another domain can use: types,
entities, queries, libraries and actions, and fixtures for their tests. A route
or a schedule is a trigger that nothing calls, so there is nothing to offer. A
plain function belongs beside the code that calls it, and a helper for several
domains goes in a library in `shared/`.

## What it looks like

```jadpo check project=e-public file=app.jadpo hidden
application {}
```

domains/planning/routes/todos.jadpo:

```jadpo check project=e-public file=domains/planning/routes/todos.jadpo expect=visibility.public_not_allowed
public routes Todos {
    prefix: /todos
}
```

```text output
error: A route group can't be `public`
 --> domains/planning/routes/todos.jadpo:1:1
  |
1 | public routes Todos {
  | ^^^^^^ not part of a domain's interface
  reason: A route group is a trigger: nothing calls it, so there is nothing to offer another domain; who reaches a route is decided by the route and its policy.
  next: Remove `public` (agent)
  rule: visibility.public_not_allowed
  help: https://jadpo.dev/docs/errors/visibility.public_not_allowed

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A plain function stays with the code that calls it:

```jadpo check project=e-public-function file=app.jadpo hidden
application {}
```

domains/planning/actions/cap_total.jadpo:

```jadpo check project=e-public-function file=domains/planning/actions/cap_total.jadpo expect=visibility.public_not_allowed
type Stock = Int {
    min: 0
}

public function capped(amount: Stock) -> Stock {
    if (amount > Stock(1000)) {
        return Stock(1000)
    }
    return amount
}

action cap_total(total: Stock) -> Stock {
    return capped(total)
}
```

```text output
error: A function can't be `public`
 --> domains/planning/actions/cap_total.jadpo:5:1
  |
5 | public function capped(amount: Stock) -> Stock {
  | ^^^^^^ not part of a domain's interface
  reason: Another domain uses a helper through a public library, so a plain function stays with the code that calls it.
  next: Remove `public` (agent)
  rule: visibility.public_not_allowed
  help: https://jadpo.dev/docs/errors/visibility.public_not_allowed

check failed: 1 error, 2 files
```

A constant is the same:

```jadpo check project=e-public-constant file=app.jadpo hidden
application {}
```

domains/planning/actions/reorder_level.jadpo:

```jadpo check project=e-public-constant file=domains/planning/actions/reorder_level.jadpo expect=visibility.public_not_allowed
type Stock = Int {
    min: 0
}

public constant most_stock: Stock = 1000

action reorder_level() -> Stock {
    return most_stock
}
```

```text output
error: A constant can't be `public`
 --> domains/planning/actions/reorder_level.jadpo:5:1
  |
5 | public constant most_stock: Stock = 1000
  | ^^^^^^ not part of a domain's interface
  reason: Another domain reads a constant through a public library, so a constant beside its use stays with the code that reads it.
  next: Remove `public` (agent)
  rule: visibility.public_not_allowed
  help: https://jadpo.dev/docs/errors/visibility.public_not_allowed

check failed: 1 error, 2 files
```

## How to fix it

**If the declaration isn't for other domains**, remove `public`:

```jadpo fragment
routes Todos {
    ...
}
```

```jadpo check project=e-public-fix-1 file=domains/planning/routes/todos.jadpo hidden
routes Todos {
    prefix: /todos
}
```

```jadpo check project=e-public-fix-1 file=app.jadpo hidden
application {}
```

Who reaches a route is decided by the route and its policy.

**If another domain should use a helper**, move it to a library in `shared/libraries/`, which every domain can use:

```jadpo fragment
library stock_rules {
    function capped(amount: Stock) -> Stock {
        ...
    }
}
```

```jadpo check project=e-public-fix-2 file=shared/libraries/stock_rules.jadpo hidden
use shared/types { Stock }

/// Rules every domain applies to stock.
library stock_rules {
    /// The amount, capped at a thousand.
    function capped(amount: Stock) -> Stock {
        if (amount > Stock(1000)) {
            return Stock(1000)
        }
        return amount
    }
}
```

```jadpo check project=e-public-fix-2 file=app.jadpo hidden
application {}
```

```jadpo check project=e-public-fix-2 file=shared/types/stock.jadpo hidden
type Stock = Int {
    min: 0
}
```

```jadpo check project=e-public-fix-2 file=domains/billing/actions/cap_total.jadpo hidden
use shared/types { Stock }
use shared/libraries { stock_rules }

action cap_total(total: Stock) -> Stock {
    return stock_rules.capped(total)
}
```

Other domains now call `stock_rules.capped`, and the function stays in one place.

## Related

- [What can be public](../language/01-project-layout-and-modules.md#what-can-be-public)
- [`visibility.public_unused`](visibility.public_unused.md)
