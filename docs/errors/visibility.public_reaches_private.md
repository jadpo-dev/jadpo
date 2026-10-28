# `visibility.public_reaches_private`

Everything a public declaration's signature names is public too (area 11),
since whoever uses it
has to build or handle it: a public action's parameters, result and failures,
a public type's fields, a public library's functions' signatures. A type in
`shared/` or the standard library is everyone's already:

```jadpo check project=p71-reaches file=app.jadpo hidden
application {}
```

```jadpo check project=p71-reaches file=domains/billing/types/amount.jadpo hidden
type Amount = Int {
    min: 0
}
```

```jadpo check project=p71-reaches file=domains/shop/actions/settle_bill.jadpo hidden
use billing

action settle_bill() -> Unit {
    billing.charge(5)
}
```

```jadpo check project=p71-reaches file=domains/billing/actions/charge.jadpo expect=visibility.public_reaches_private
use billing/types { Amount }

public action charge(amount: Amount) -> Unit {}
```

```text output
error: `charge` is public, but `Amount` isn't
 --> domains/billing/actions/charge.jadpo:3:30
  |
3 | public action charge(amount: Amount) -> Unit {}
  |                              ^^^^^^ a type only domain `billing` sees
  reason: Whoever uses a public action has to build or handle what its signature names, so everything it names is public too; `Amount` is a type in `domains/billing/types/amount.jadpo` that only domain `billing` sees.
  next: Mark `Amount` `public`, or keep it out of `charge`'s signature (agent)
  rule: visibility.public_reaches_private
  help: docs/language/01-project-layout-and-modules.html#visibility-public-reaches-private

check failed: 1 error, 4 files
```
