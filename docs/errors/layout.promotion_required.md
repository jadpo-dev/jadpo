# `layout.promotion_required`

```jadpo check project=e-promotion file=app.jadpo hidden
application {}
```

```jadpo check project=e-promotion file=domains/planning/actions/postpone_all.jadpo
use planning/actions { SnoozeDays }

action postpone_all(days: SnoozeDays) -> Unit {}
```

```jadpo check project=e-promotion file=domains/planning/actions/snooze_todo.jadpo expect=layout.promotion_required
type SnoozeDays = Int {
    min: 1
    max: 30
}

action snooze_todo(days: SnoozeDays) -> Unit {}
```

```text output
error: `SnoozeDays` is used beyond this file
 --> domains/planning/actions/snooze_todo.jadpo:1:6
  |
1 | type SnoozeDays = Int {
  |      ^^^^^^^^^^ used by another file
  reason: A type used by one file lives in it; once another file uses it, it moves to a file of its own in `domains/planning/types/`. `domains/planning/actions/postpone_all.jadpo` uses it too.
  next: Move `SnoozeDays` to `domains/planning/types/snooze_days.jadpo`, or run `jadpo fmt --layout`, which moves it and updates the imports (agent)
  rule: layout.promotion_required
  help: docs/language/01-project-layout-and-modules.html#layout-promotion-required

check failed: 1 error, 3 files
```

Moving to the right folder is not enough: a second declaration in a file
moves to a file of its own once another file uses it, so every name a `use`
line brings in is `<folder>/<name>.jadpo`. The edit makes the move, and
`jadpo fmt --layout` makes it and updates the imports:

```jadpo check project=e-promotion-file file=app.jadpo hidden
application {}
```

```jadpo check project=e-promotion-file file=domains/sales/types/basket.jadpo expect=layout.promotion_required
type DiscountCode = Text {
    max_length: 12
}

type Basket = Object {
    code: DiscountCode
}
```

```text output
error: `DiscountCode` is used beyond this file
 --> domains/sales/types/basket.jadpo:1:6
  |
1 | type DiscountCode = Text {
  |      ^^^^^^^^^^^^ used by another file
  reason: A declaration another file uses has a file of its own named after it, so its `use` line says where it is; `DiscountCode` is in `basket.jadpo`, named after another declaration, and moves to `domains/sales/types/`. `domains/sales/actions/apply_code.jadpo` uses it too.
  next: Move `DiscountCode` to `domains/sales/types/discount_code.jadpo`, or run `jadpo fmt --layout`, which moves it and updates the imports (agent)
  rule: layout.promotion_required
  help: docs/language/01-project-layout-and-modules.html#layout-promotion-required

check failed: 1 error, 3 files
```

```jadpo check project=e-promotion-file file=domains/sales/actions/apply_code.jadpo
use sales/types { Basket, DiscountCode }

action apply_code(basket: Basket, code: DiscountCode) -> Unit {}
```
