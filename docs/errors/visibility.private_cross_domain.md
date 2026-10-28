# `visibility.private_cross_domain`

```jadpo check project=e-private file=app.jadpo hidden
application {}
```

```jadpo check project=e-private file=shared/types/quantity.jadpo hidden
type Quantity = Int {
    min: 1
}
```

```jadpo check project=e-private file=domains/inventory/actions/reserve_stock.jadpo
use shared/types { Quantity }

action reserve_stock(quantity: Quantity) -> Unit {}
```

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
  help: docs/errors/visibility.private_cross_domain.md

check failed: 1 error, 4 files
```

A fixture is private to its domain too: another domain's tests call it only
when it is `public` (area 13):

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
  help: docs/errors/visibility.private_cross_domain.md

check failed: 1 error, 5 files
```
