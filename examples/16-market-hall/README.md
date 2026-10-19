# 16 · A market hall's tills

The tills of a covered market: stalls price in pounds, take deals off,
add a service charge, split a table's bill, share the hall's card
takings between stalls, price for visitors in euros, give change, keep
what a receipt may keep of a card, and show every amount the way the
person reading it needs. It teaches money.

An amount of money is a whole number of its currency's minor units,
pence or cents, with its `Currency`; never a decimal of pounds that
picks up a fraction of a penny. A type built on `Money` can fix its
currency, so adding pounds to euros doesn't compile, and says how `*`
and `/` round. Splitting never rounds at all: every penny goes to
someone.

## What it teaches

1. **Money types.** `currency:` fixes a type's currency, `min:` bounds it in minor units, and `rounding:` says how `*` and `/` round; a type that fixes nothing takes any currency, known only while the till runs. See `domains/tills/types/stall_price.jadpo`, `domains/tills/types/service_charge.jadpo`, `domains/tills/types/tendered.jadpo` and [area 19, Money and payments](../../docs/language/19-standard-library.md#money-and-payments).
2. **Prices and deals.** `Price` keeps whether VAT is in it, and `Discount` is an amount off or a percentage off, taken off with `money.apply_discount`. See `domains/tills/actions/apply_deal.jadpo` and [area 19, Prices, discounts and rates](../../docs/language/19-standard-library.md#prices-discounts-and-rates).
3. **Percentages.** A `Percentage` type with bounds, written `12.5%`, multiplies an amount, which rounds as the amount's type says: down, for a service charge. See `domains/tills/types/service_rate.jadpo` and `domains/tills/actions/service_on.jadpo`.
4. **Sharing to the penny.** `money.split` shares a bill equally and `money.allocate` by ratios, the leftover pennies one each to the first shares; a list type with `min_length:` proves there is a ratio. See `domains/tills/actions/split_bill.jadpo`, `domains/tills/actions/share_card_takings.jadpo` and `domains/tills/types/hall_pitches.jadpo`.
5. **Adding up.** `money.sum` adds a list, the currency giving the zero an empty one adds up to. See `domains/tills/actions/day_total.jadpo`.
6. **Rates.** A type built on `ExchangeRate` fixes `from:` and `to:`, so `money.convert` needs no `attempt` and a rate the wrong way round doesn't compile. See `domains/tills/types/pounds_to_euros.jadpo` and `domains/tills/actions/price_in_euros.jadpo`.
7. **A currency known only while running.** `money.of` reads major units and fails for more places than the currency has; comparing `money.currency` with a currency written out proves it. See `domains/tills/actions/take_payment.jadpo`.
8. **Cards.** `money.mask` keeps a card's brand and last four, and the number itself is never kept. See `domains/tills/actions/card_on_receipt.jadpo`.
9. **Amounts for people.** `text.format` shows an amount with its symbol, code or name, in whole units, or a refund in brackets. See `domains/tills/actions/price_label.jadpo`.

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/16-market-hall
target/release/jadpo test examples/16-market-hall
target/release/jadpo fmt --check examples/16-market-hall
```

- `check` ends with `checked: <n> files` and no errors.
- `test` ends with `tested: 11 passed, 11 action tests`.

## Try next

- In `pounds_to_euros.jadpo`, swap the currencies, so the rate runs from euros to pounds. `jadpo check` reports `type.mismatch` where a price in pounds is converted at it, and `failure.attempt_required` where the test builds the morning's rate from a quote in pounds, which the type no longer proves.
  <!-- try {"file": "domains/tills/types/pounds_to_euros.jadpo", "replace": "    from: Currency.gbp\n    to: Currency.eur", "with": "    from: Currency.eur\n    to: Currency.gbp", "check": ["failure.attempt_required", "type.mismatch"]} -->
- In `service_charge.jadpo`, round `Rounding.half_up`. The service charge's test fails with `test.assertion_failed`: £2.935 now rounds up to £2.94.
  <!-- try {"file": "domains/tills/types/service_charge.jadpo", "replace": "rounding: Rounding.down", "with": "rounding: Rounding.half_up", "test": ["test.assertion_failed"], "failed": 1} -->
- In `service_on.test.jadpo`, charge `25%`. `jadpo check` reports `type.invalid_literal`: a percentage written out is checked while compiling against `ServiceRate`'s `max: 20%`.
  <!-- try {"file": "domains/tills/actions/service_on.test.jadpo", "replace": "service_on(bill, 12.5%)", "with": "service_on(bill, 25%)", "check": ["type.invalid_literal"]} -->
- Then read [17-ebike-hire](../17-ebike-hire/README.md), which measures things.
