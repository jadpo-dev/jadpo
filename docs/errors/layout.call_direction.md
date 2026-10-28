# `layout.call_direction`

```jadpo check project=e-direction file=app.jadpo hidden
application {}
```

```jadpo check project=e-direction file=domains/ordering/types/order_total.jadpo hidden
type OrderTotal = Int {
    min: 0
}
```

```jadpo check project=e-direction file=domains/ordering/actions/apply_discount.jadpo hidden
use shared/types { DiscountTerms }

action apply_discount(discount: DiscountTerms) -> Unit {}
```

```jadpo check project=e-direction file=shared/types/discount_terms.jadpo expect=layout.call_direction
type DiscountTerms = Object {
    applies_to: OrderTotal
}
```

```text output
error: Shared code uses `OrderTotal` from domain `ordering`
 --> shared/types/discount_terms.jadpo:2:17
  |
2 |     applies_to: OrderTotal
  |                 ^^^^^^^^^^ domain code
  reason: `shared/` holds code every domain may use, so it can't depend on any one domain; `OrderTotal` is a type in `domains/ordering/types/order_total.jadpo`.
  next: Move the shared declaration into `ordering`, or stop it depending on `OrderTotal` (human)
  rule: layout.call_direction
  help: docs/language/01-project-layout-and-modules.html#layout-call-direction

check failed: 1 error, 4 files
```
