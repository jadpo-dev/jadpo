# `data.stored_decimal_scale`

An update by `id` adds or subtracts a decimal with more places than its field keeps.

## Why Jadpo refuses it

An update by `id` has the database work the new value out in one step, and
PostgreSQL rounds a sum half away from zero where Jadpo rounds half to
even. So the value could differ from the one Jadpo would give, by rounding
alone. A decimal with no more places than the field keeps needs no
rounding, so it is the same in both.

## What it looks like

main.jadpo:

```jadpo check project=e-stored-decimal-scale expect=data.stored_decimal_scale
type Balance = Decimal {
    scale: 2
}

entity Wallet {
    id: Uuid
    balance: Balance
}

action add_interest(wallet_id: Wallet.id) fails WalletNotFound -> Wallet {
    return attempt Wallet.update(wallet_id) {
        set: {
            balance: Balance(Wallet.balance + 0.005)
        }
    }
}
```

```text output
error: `Wallet.update` adds a decimal with more places than `Wallet.balance` keeps
 --> main.jadpo:13:47
   |
13 |             balance: Balance(Wallet.balance + 0.005)
   |                                               ^^^^^ 3 decimal places
  reason: The database works `Wallet.balance` out in one step, and keeps it at `scale: 2`; `0.005` has 3, and PostgreSQL would round the result half away from zero where Jadpo rounds half to even, so the stored value could differ from Jadpo's.
  next: Give the value at most 2 decimal places, or a type with `scale: 2` or fewer (agent)
  rule: data.stored_decimal_scale
  help: https://jadpo.dev/docs/errors/data.stored_decimal_scale

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the amount fits the field**, give it no more places than the field
keeps:

```jadpo fragment
            balance: Balance(Wallet.balance + 0.01)
```

```jadpo check project=e-stored-decimal-fix hidden
type Balance = Decimal {
    scale: 2
}

entity Wallet {
    id: Uuid
    balance: Balance
}

action add_interest(wallet_id: Wallet.id) fails WalletNotFound -> Wallet {
    return attempt Wallet.update(wallet_id) {
        set: {
            balance: Balance(Wallet.balance + 0.01)
        }
    }
}
```

The sum now has at most two places, so the database has nothing to round.

## Related

- [Counters and other one-step updates](../language/06-reads-and-writes.md#counters-and-other-one-step-updates)
