# `visibility.public_reaches_private`

A public declaration's signature names a type or entity that isn't public.

## Why Jadpo refuses it

Whoever uses a public declaration has to build or handle everything its
signature names: a public action's parameters, result and failures, a public
type's fields, and a public library's functions' signatures. If one of those
stays private, another domain can see the declaration and still can't call it.

A type in `shared/` or the standard library is everyone's already, so it needs
no `public`.

## What it looks like

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

domains/billing/actions/charge.jadpo:

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
  help: https://jadpo.dev/docs/errors/visibility.public_reaches_private

check failed: 1 error, 4 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A public type's fields are part of what it offers, so they have to be public
too:

```jadpo check project=e-public-reaches-type file=app.jadpo hidden
application {}
```

```jadpo check project=e-public-reaches-type file=domains/billing/types/amount.jadpo hidden
type Amount = Int {
    min: 0
}
```

```jadpo check project=e-public-reaches-type file=domains/shop/actions/keep.jadpo hidden
use billing/types { Receipt }

action keep(receipt: Receipt) -> Unit {}
```

domains/billing/types/receipt.jadpo:

```jadpo check project=e-public-reaches-type file=domains/billing/types/receipt.jadpo expect=visibility.public_reaches_private
use billing/types { Amount }

public type Receipt = Object {
    amount: Amount
}
```

```text output
error: `Receipt` is public, but `Amount` isn't
 --> domains/billing/types/receipt.jadpo:4:13
  |
4 |     amount: Amount
  |             ^^^^^^ a type only domain `billing` sees
  reason: Whoever uses a public type has to build or handle what its signature names, so everything it names is public too; `Amount` is a type in `domains/billing/types/amount.jadpo` that only domain `billing` sees.
  next: Mark `Amount` `public`, or keep it out of `Receipt`'s signature (agent)
  rule: visibility.public_reaches_private
  help: https://jadpo.dev/docs/errors/visibility.public_reaches_private

check failed: 1 error, 4 files
```

## How to fix it

**If other domains should use the type**, mark it `public`. In `domains/billing/types/amount.jadpo`:

```jadpo fragment
public type Amount = Int {
    ...
}
```

```jadpo check project=p71-reaches-fix-1 file=domains/billing/types/amount.jadpo hidden
/// A bill's amount.
public type Amount = Int {
    min: 0
}
```

```jadpo check project=p71-reaches-fix-1 file=app.jadpo hidden
application {}
```

```jadpo check project=p71-reaches-fix-1 file=domains/shop/actions/settle_bill.jadpo hidden
use billing

action settle_bill() -> Unit {
    billing.charge(5)
}
```

```jadpo check project=p71-reaches-fix-1 file=domains/billing/actions/charge.jadpo hidden
use billing/types { Amount }

public action charge(amount: Amount) -> Unit {}
```

`Amount` is now part of what `billing` offers, and it keeps offering it.

**If any domain may use it**, move it to `shared/types/`, which every domain sees:

```jadpo fragment
use shared/types { Amount }

public action charge(amount: Amount) -> Unit {}
```

```jadpo check project=p71-reaches-fix-2 file=domains/billing/actions/charge.jadpo hidden
use shared/types { Amount }

public action charge(amount: Amount) -> Unit {}
```

```jadpo check project=p71-reaches-fix-2 file=app.jadpo hidden
application {}
```

```jadpo check project=p71-reaches-fix-2 file=shared/types/amount.jadpo hidden
type Amount = Int {
    min: 0
}
```

```jadpo check project=p71-reaches-fix-2 file=domains/shop/actions/settle_bill.jadpo hidden
use billing

action settle_bill() -> Unit {
    billing.charge(5)
}
```

`Amount` now belongs to the whole project, and neither domain owns it.

**If the type should stay private**, keep it out of the signature, and let the action work the value out itself:

domains/billing/actions/charge.jadpo:

```jadpo check project=p71-reaches-fix-3 file=domains/billing/actions/charge.jadpo
public action charge() -> Unit {}
```

```jadpo check project=p71-reaches-fix-3 file=app.jadpo hidden
application {}
```

```jadpo check project=p71-reaches-fix-3 file=domains/shop/actions/settle_bill.jadpo hidden
use billing

action settle_bill() -> Unit {
    billing.charge()
}
```


Callers no longer pass an amount, and `Amount` stays inside `billing`.

## Related

- [What can be public](../language/01-project-layout-and-modules.md#what-can-be-public)
- [`visibility.private_cross_domain`](visibility.private_cross_domain.md)
- [What is public](../language/11-integrations.md#what-is-public)
