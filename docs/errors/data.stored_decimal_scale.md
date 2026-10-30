# `data.stored_decimal_scale`

The database rounds a sum its own way, so a decimal added to a stored field
keeps no more places than the field does:

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
