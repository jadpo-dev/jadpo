# `layout.call_direction`

Code in `shared/` uses a declaration that belongs to one domain.

## Why Jadpo refuses it

`shared/` holds the types, failures, libraries and messages that every
domain may use. If shared code used one domain's declaration, every
domain that used the shared code would depend on that domain too, and a
change inside it could break code that never named it.

Jadpo won't choose the fix for you. Either the shared declaration really
belongs to the one domain, or the domain's declaration is needed by
everyone, and that decides where the code lives.

## What it looks like

shared/types/discount_terms.jadpo:

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
  help: https://jadpo.dev/docs/errors/layout.call_direction

check failed: 1 error, 4 files
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether `DiscountTerms` belongs to `ordering` or `OrderTotal`
belongs to every domain.

## How to fix it

The question is whether other domains need `DiscountTerms`.

**If only `ordering` uses it**, move `DiscountTerms` into the domain, to
`domains/ordering/types/discount_terms.jadpo`, and import it from there:

```jadpo fragment
use ordering/types { OrderTotal }

type DiscountTerms = Object {
    applies_to: OrderTotal
}
```

```jadpo check project=e-direction-fix-1 file=domains/ordering/types/discount_terms.jadpo hidden
use ordering/types { OrderTotal }

type DiscountTerms = Object {
    applies_to: OrderTotal
}
```

```jadpo check project=e-direction-fix-1 file=app.jadpo hidden
application {}
```

```jadpo check project=e-direction-fix-1 file=domains/ordering/types/order_total.jadpo hidden
type OrderTotal = Int {
    min: 0
}
```

```jadpo check project=e-direction-fix-1 file=domains/ordering/actions/apply_discount.jadpo hidden
use ordering/types { DiscountTerms }

action apply_discount(discount: DiscountTerms) -> Unit {}
```

`apply_discount` now imports it with `use ordering/types { DiscountTerms }`.
`DiscountTerms` is private to `ordering`, so another domain can't use it
unless `ordering` marks it `public`.

**If other domains need it too**, move `OrderTotal` to `shared/` as well,
to `shared/types/order_total.jadpo`, and import it there:

```jadpo fragment
use shared/types { OrderTotal }
```

```jadpo check project=e-direction-fix-2 file=shared/types/discount_terms.jadpo hidden
use shared/types { OrderTotal }

type DiscountTerms = Object {
    applies_to: OrderTotal
}
```

```jadpo check project=e-direction-fix-2 file=app.jadpo hidden
application {}
```

```jadpo check project=e-direction-fix-2 file=shared/types/order_total.jadpo hidden
type OrderTotal = Int {
    min: 0
}
```

```jadpo check project=e-direction-fix-2 file=domains/ordering/actions/apply_discount.jadpo hidden
use shared/types { DiscountTerms }

action apply_discount(discount: DiscountTerms) -> Unit {}
```

Both types are now shared, so any domain may use them, and `shared/`
depends on no domain.

## Related

- [Sharing and promotion](../language/01-project-layout-and-modules.md#sharing-and-promotion)
- [Helpers that depend on a domain](../language/01-project-layout-and-modules.md#helpers-that-depend-on-a-domain)
- [Domains](../language/01-project-layout-and-modules.md#domains)
- [Imports](../language/01-project-layout-and-modules.md#imports)
