# `query.aggregate_field`

An aggregate in a query is worked out over a field it can't add up, compare or average.

## Why Jadpo refuses it

An aggregate is worked out by the database, and the answer has to be the one
Jadpo's own code would give. `sum` adds up numbers, durations, quantities
and money in one currency, as `+` does. `min` and `max` take a field a query
can sort by, other than an identity. `average` takes a field built on `Int`
or `Decimal`, and keeps at least the places each value has.

Anything else has no answer that means something: amounts in two currencies
don't add up, and a sum of no records needs a currency for its zero.

## What it looks like

main.jadpo:

```jadpo check project=e-aggregate-field expect=query.aggregate_field
type Fine = Money {}

entity Loan {
    id: Uuid
    fine: Fine
}

type FinesOwed = Output {
    owed: Money
}

query fines_owed() -> FinesOwed {
    from: Loan
    aggregate: {
        owed: sum(Loan.fine)
    }
}
```

```text output
error: `sum` adds up money in one currency, and `Loan.fine`'s type doesn't fix one
 --> main.jadpo:15:19
   |
15 |         owed: sum(Loan.fine)
   |                   ^^^^^^^^^ `Fine`
  reason: Amounts in two currencies don't add up (`currency_mismatch`), and a sum of no records needs a currency for its zero, so the field's type fixes its currency: `Fine` doesn't.
  next: Give `Fine` a `currency:` (agent)
  rule: query.aggregate_field
  help: https://jadpo.dev/docs/errors/query.aggregate_field

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`sum` doesn't add up text, or anything else `+` wouldn't:

main.jadpo:

```jadpo check project=e-aggregate-field-sum expect=query.aggregate_field
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
}

type TitleTotal = Output {
    total: Int?
}

query title_total() -> TitleTotal {
    from: Book
    aggregate: {
        total: sum(Book.title)
    }
}
```

```text output
error: `sum` can't add up `Book.title`
 --> main.jadpo:17:20
   |
17 |         total: sum(Book.title)
   |                    ^^^^^^^^^^ `Title`
  reason: `sum` adds up numbers, durations, quantities and money in one currency, as `+` does; `Book.title` is `Title`.
  next: Add up another field, or count the records with `count()` (agent)
  rule: query.aggregate_field
  help: https://jadpo.dev/docs/errors/query.aggregate_field

check failed: 1 error, 1 file
```

`min` and `max` don't compare identities, which have no order that means
anything:

main.jadpo:

```jadpo check project=e-aggregate-field-min expect=query.aggregate_field
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
}

type FirstBook = Output {
    first_id: Uuid?
}

query first_book() -> FirstBook {
    from: Book
    aggregate: {
        first_id: min(Book.id)
    }
}
```

```text output
error: `min` can't compare `Book.id`
 --> main.jadpo:17:23
   |
17 |         first_id: min(Book.id)
   |                       ^^^^^^^ `Uuid`
  reason: `min` and `max` take a field a query can sort by, other than an identity: numbers, times, dates, durations, quantities, percentages and text, by code point. `Book.id` is `Uuid`.
  next: Take the `min` of another field (agent)
  rule: query.aggregate_field
  help: https://jadpo.dev/docs/errors/query.aggregate_field

check failed: 1 error, 1 file
```

`average` takes a field built on `Int` or `Decimal`:

main.jadpo:

```jadpo check project=e-aggregate-field-average expect=query.aggregate_field
type Fine = Money {
    currency: Currency.gbp
}

entity Loan {
    id: Uuid
    fine: Fine
}

type MeanFine = Output {
    mean: Decimal?
}

query mean_fine() -> MeanFine {
    from: Loan
    aggregate: {
        mean: average(Loan.fine, scale: 2, rounding: Rounding.half_even)
    }
}
```

```text output
error: `average` can't work out a mean of `Loan.fine`
 --> main.jadpo:17:23
   |
17 |         mean: average(Loan.fine, scale: 2, rounding: Rounding.half_even)
   |                       ^^^^^^^^^ `Fine`
  reason: `average` takes a field built on `Int` or `Decimal`; `Loan.fine` is `Fine`.
  next: Average another field, or add it up with `sum` (agent)
  rule: query.aggregate_field
  help: https://jadpo.dev/docs/errors/query.aggregate_field

check failed: 1 error, 1 file
```

An average of a decimal field keeps at least the places the field keeps,
since it is the exact mean rounded once:

main.jadpo:

```jadpo check project=e-aggregate-field-places expect=query.aggregate_field
type Weight = Decimal {
    scale: 3
}

entity Parcel {
    id: Uuid
    weight: Weight
}

type MeanWeight = Output {
    mean: Decimal?
}

query mean_weight() -> MeanWeight {
    from: Parcel
    aggregate: {
        mean: average(Parcel.weight, scale: 1, rounding: Rounding.half_even)
    }
}
```

```text output
error: `average` keeps 1 places, and `Parcel.weight` keeps 3
 --> main.jadpo:17:23
   |
17 |         mean: average(Parcel.weight, scale: 1, rounding: Rounding.half_even)
   |                       ^^^^^^^^^^^^^ `Weight`
  reason: The average is the exact mean rounded once to `scale:` places, so it keeps at least the places each value has.
  next: Write `scale: 3` or more (agent)
  rule: query.aggregate_field
  help: https://jadpo.dev/docs/errors/query.aggregate_field

check failed: 1 error, 1 file
```

## How to fix it

**If every amount is in one currency**, say which in the field's type:

```jadpo fragment
type Fine = Money {
    ...
    currency: Currency.gbp
}
```

```jadpo check project=e-aggregate-field-fix-1 hidden
type Fine = Money {
    currency: Currency.gbp
}

entity Loan {
    id: Uuid
    fine: Fine
}

type FinesOwed = Output {
    owed: Money
}

query fines_owed() -> FinesOwed {
    from: Loan
    aggregate: {
        owed: sum(Loan.fine)
    }
}
```

The sum is now in pounds, and it is a zero in pounds when no records match.

**If what you want is how many records there are**, count them:

```jadpo fragment
    aggregate: {
        total: count()
    }
```

```jadpo check project=e-aggregate-field-fix-2 hidden
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
}

type TitleTotal = Output {
    total: Int
}

query title_total() -> TitleTotal {
    from: Book
    aggregate: {
        total: count()
    }
}
```

The result's field is `Int` here, since a count is always there, even when nothing matches.

**If the average needs more places**, give it at least as many as the field keeps:

```jadpo fragment
        mean: average(Parcel.weight, scale: 3, rounding: Rounding.half_even)
```

```jadpo check project=e-aggregate-field-fix-3 hidden
type Weight = Decimal {
    scale: 3
}

entity Parcel {
    id: Uuid
    weight: Weight
}

type MeanWeight = Output {
    mean: Decimal?
}

query mean_weight() -> MeanWeight {
    from: Parcel
    aggregate: {
        mean: average(Parcel.weight, scale: 3, rounding: Rounding.half_even)
    }
}
```

Rounding now happens once, on the exact mean.

## Related

- [The aggregates](../language/06-reads-and-writes.md#the-aggregates)
- [Units and money in plain numbers](../language/02-values-and-types.md#units-and-money-in-plain-numbers)
