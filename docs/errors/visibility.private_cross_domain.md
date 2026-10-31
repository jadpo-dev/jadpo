# `visibility.private_cross_domain`

Code in one domain uses an action, type or other declaration that another domain keeps private, because it isn't marked `public`.

## Why Jadpo refuses it

A domain is the boundary of what other code can see. Everything in it is
private unless it is marked `public`, so a domain can change its private
code without breaking another domain. A public action is how other
domains change its data; anything else stays its own.

Jadpo won't mark the declaration `public` for you. Making it public is
the owning domain's promise to keep offering it to every other domain.

## What it looks like

```jadpo check project=e-private file=app.jadpo hidden
application {}
```

```jadpo check project=e-private file=shared/types/quantity.jadpo hidden
type Quantity = Int {
    min: 1
}
```

domains/inventory/actions/reserve_stock.jadpo:

```jadpo check project=e-private file=domains/inventory/actions/reserve_stock.jadpo
use shared/types { Quantity }

action reserve_stock(quantity: Quantity) -> Unit {}
```

domains/ordering/actions/checkout.jadpo:

```jadpo check project=e-private file=domains/ordering/actions/checkout.jadpo expect=visibility.private_cross_domain
use shared/types { Quantity }
use inventory

action checkout(quantity: Quantity) -> Unit {
    inventory.reserve_stock(quantity)
}
```

```text output
error: `reserve_stock` is private to domain `inventory`
 --> domains/ordering/actions/checkout.jadpo:5:15
  |
5 |     inventory.reserve_stock(quantity)
  |               ^^^^^^^^^^^^^ private
  reason: Another domain may use only what a domain marks `public`; `reserve_stock` is an action in `domains/inventory/actions/reserve_stock.jadpo`.
  next: Mark `reserve_stock` `public` if `inventory` should offer it to other domains (human)
  rule: visibility.private_cross_domain
  help: https://jadpo.dev/docs/errors/visibility.private_cross_domain

check failed: 1 error, 4 files
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether `inventory` offers `reserve_stock` to other domains.

A fixture is private to its domain too. Another domain's tests call it
only when it is `public`:

```jadpo check project=e-private-fixture file=app.jadpo hidden
application {}
```

```jadpo check project=e-private-fixture file=domains/planning/types/todo_count.jadpo hidden
type TodoCount = Int {
    min: 0
}
```

```jadpo check project=e-private-fixture file=domains/planning/fixtures/two_open_todos.jadpo hidden
use planning/types { TodoCount }

fixture two_open_todos() -> TodoCount {
    return 2
}
```

```jadpo check project=e-private-fixture file=domains/reminders/actions/send_reminders.jadpo hidden
action send_reminders() -> Unit {}
```

domains/reminders/actions/send_reminders.test.jadpo:

```jadpo check project=e-private-fixture file=domains/reminders/actions/send_reminders.test.jadpo expect=visibility.private_cross_domain
use planning

test "reminders start from planning's open todos" {
    send_reminders()
    assert planning.two_open_todos() == 2
}
```

```text output
error: `two_open_todos` is private to domain `planning`
 --> domains/reminders/actions/send_reminders.test.jadpo:5:21
  |
5 |     assert planning.two_open_todos() == 2
  |                     ^^^^^^^^^^^^^^ private
  reason: Another domain may use only what a domain marks `public`; `two_open_todos` is a fixture in `domains/planning/fixtures/two_open_todos.jadpo`.
  next: Mark `two_open_todos` `public` if `planning` should offer it to other domains' tests (human)
  rule: visibility.private_cross_domain
  help: https://jadpo.dev/docs/errors/visibility.private_cross_domain

check failed: 1 error, 5 files
```

## How to fix it

The question is whether the owning domain should offer the declaration
to other domains.

**If `inventory` should offer `reserve_stock`**, mark it `public`. In
`domains/inventory/actions/reserve_stock.jadpo`:

```jadpo fragment
public action reserve_stock(quantity: Quantity) -> Unit {}
```

```jadpo check project=e-private-fix-1 file=domains/inventory/actions/reserve_stock.jadpo hidden
use shared/types { Quantity }

public action reserve_stock(quantity: Quantity) -> Unit {}
```

```jadpo check project=e-private-fix-1 file=app.jadpo hidden
application {}
```

```jadpo check project=e-private-fix-1 file=shared/types/quantity.jadpo hidden
type Quantity = Int {
    min: 1
}
```

```jadpo check project=e-private-fix-1 file=domains/ordering/actions/checkout.jadpo hidden
use shared/types { Quantity }
use inventory

action checkout(quantity: Quantity) -> Unit {
    inventory.reserve_stock(quantity)
}
```

`checkout` can now call `inventory.reserve_stock(quantity)`, and both
domains' changes commit or roll back together, in one database
transaction. Every type in a public action's signature must be public or
shared too; `Quantity` is shared.

**If `planning` should offer `two_open_todos` to other domains' tests**,
mark the fixture `public`, and the type it returns as well. In
`domains/planning/fixtures/two_open_todos.jadpo`:

```jadpo fragment
public fixture two_open_todos() -> TodoCount {
```

```jadpo check project=e-private-fixture-fix-2 file=domains/planning/fixtures/two_open_todos.jadpo hidden
use planning/types { TodoCount }

/// Two open todos.
public fixture two_open_todos() -> TodoCount {
    return 2
}
```

```jadpo check project=e-private-fixture-fix-2 file=app.jadpo hidden
application {}
```

```jadpo check project=e-private-fixture-fix-2 file=domains/planning/types/todo_count.jadpo hidden
/// How many todos are open.
public type TodoCount = Int {
    min: 0
}
```

```jadpo check project=e-private-fixture-fix-2 file=domains/reminders/actions/send_reminders.jadpo hidden
action send_reminders() -> Unit {}
```

```jadpo check project=e-private-fixture-fix-2 file=domains/reminders/actions/send_reminders.test.jadpo hidden
use planning

test "reminders start from planning's open todos" {
    send_reminders()
    assert planning.two_open_todos() == 2
}
```

The `reminders` test can now call `planning.two_open_todos()`. A public
fixture is still offered to tests, fixtures and seeds only, never to
application code.

When the declaration should stay private, the other domain stops using
it: it calls a public action of the owning domain instead, or keeps a
declaration of its own.

## Related

- [Ownership and calls replace modules](../language/01-project-layout-and-modules.md#ownership-and-calls-replace-modules)
- [What can be public](../language/01-project-layout-and-modules.md#what-can-be-public)
- [Changing another domain's data](../language/01-project-layout-and-modules.md#changing-another-domains-data)
- [A fixture for other domains: `public`](../language/13-testing.md#a-fixture-for-other-domains-public)
