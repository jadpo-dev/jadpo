# `query.group_key`

A query's `group:` section isn't a block of fields and date buckets it can group records by.

## Why Jadpo refuses it

A group is worked out by the database, and two records fall in the same
group exactly when their stored values are equal. That holds for a field
built on `Bool`, `Int`, `Text`, `Uuid` or `CalendarDate`, or a plain enum,
and for a year, month or day of a date field.

It doesn't hold for an instant, which has a day only in a time zone, or for
a decimal, where `1.0` and `1.00` would land in one group and only one could
be shown.

## What it looks like

main.jadpo:

```jadpo check project=e-group-key expect=query.group_key
type Title = Text {
    max_length: 80
}

entity Loan {
    id: Uuid
    title: Title
    lent_at: Instant
}

type LoansByYear = Output {
    lent_year: Int
    loans_made: Int
}

query loans_by_year() -> List<LoansByYear> {
    from: Loan
    group: {
        lent_year: temporal.year(Loan.lent_at)
    }
    aggregate: {
        loans_made: count()
    }
    order: [lent_year desc]
    limit: 20
}
```

```text output
error: `temporal.year` groups by a date, and `Loan.lent_at` is `Instant`
 --> main.jadpo:19:34
   |
19 |         lent_year: temporal.year(Loan.lent_at)
   |                                  ^^^^^^^^^^^^ not a date
  reason: A time bucket is worked out by the database, which gives exactly what code gives only for a date: an instant has a day only in a zone, and PostgreSQL's time zone database can differ from the one built into Jadpo.
  next: Keep the day as a `CalendarDate` field, and group by a time bucket of it (agent)
  rule: query.group_key
  help: https://jadpo.dev/docs/errors/query.group_key

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A field of any other type is refused, such as an amount of money:

main.jadpo:

```jadpo check project=e-group-key-field expect=query.group_key
type Fine = Money {
    currency: Currency.gbp
}

entity Loan {
    id: Uuid
    fine: Fine
}

type LoansByFine = Output {
    fine: Fine
    loans_made: Int
}

query loans_by_fine() -> List<LoansByFine> {
    from: Loan
    group: {
        fine: Loan.fine
    }
    aggregate: {
        loans_made: count()
    }
    order: [fine asc]
    limit: 20
}
```

```text output
error: Records can't be grouped by `Loan.fine`
 --> main.jadpo:18:15
   |
18 |         fine: Loan.fine
   |               ^^^^^^^^^ `Fine`
  reason: A group is a field whose stored values are equal exactly when Jadpo's are: one built on `Bool`, `Int`, `Text`, `Uuid` or `CalendarDate`, or a plain enum. `Loan.fine` is `Fine`.
  next: Group by another field, or by a time bucket of a date (agent)
  rule: query.group_key
  help: https://jadpo.dev/docs/errors/query.group_key

check failed: 1 error, 1 file
```

A group is a field, or a time bucket of a date field, and nothing worked out
from one:

main.jadpo:

```jadpo check project=e-group-key-shape expect=query.group_key
type Title = Text {
    max_length: 80
}

entity Loan {
    id: Uuid
    title: Title
}

type LoansByTitle = Output {
    title: Title
    loans_made: Int
}

query loans_by_title() -> List<LoansByTitle> {
    from: Loan
    group: {
        title: text.trim(Loan.title)
    }
    aggregate: {
        loans_made: count()
    }
    order: [title asc]
    limit: 20
}
```

```text output
error: `text.trim(Loan.title)` isn't a field or a time bucket
 --> main.jadpo:18:16
   |
18 |         title: text.trim(Loan.title)
   |                ^^^^^^^^^^^^^^^^^^^^^ not a group
  reason: A group is a field, or a time bucket: `temporal.year`, `temporal.month` or `temporal.day` of a date field.
  next: Group by a field of `Loan`, or a time bucket of a date (agent)
  rule: query.group_key
  help: https://jadpo.dev/docs/errors/query.group_key

check failed: 1 error, 1 file
```

`group:` is always a block, with a name for each group:

main.jadpo:

```jadpo check project=e-group-key-block expect=query.group_key
type Title = Text {
    max_length: 80
}

entity Loan {
    id: Uuid
    title: Title
}

type LoansByTitle = Output {
    loans_made: Int
}

query loans_by_title() -> List<LoansByTitle> {
    from: Loan
    group: Loan.title
    aggregate: {
        loans_made: count()
    }
    order: [loans_made desc]
    limit: 20
}
```

```text output
error: `group:` is a block
 --> main.jadpo:16:5
   |
16 |     group: Loan.title
   |     ^^^^^ not a block
  reason: `group:` names each of its entries after the result field it fills, in a block: `group: { <name>: <Entity>.<field> }`.
  next: Write `group: { <name>: <Entity>.<field> }` (agent)
  rule: query.group_key
  help: https://jadpo.dev/docs/errors/query.group_key

check failed: 1 error, 1 file
```

## How to fix it

**If it is a time bucket of a moment**, keep the day as a `CalendarDate` field, set by the action that saves the record, and group by a bucket of it:

```jadpo fragment
    lent_on: CalendarDate
...
        lent_year: temporal.year(Loan.lent_on)
```

```jadpo check project=e-group-key-fix-1 hidden
type Title = Text {
    max_length: 80
}

entity Loan {
    id: Uuid
    title: Title
    lent_on: CalendarDate
}

type LoansByYear = Output {
    lent_year: Int
    loans_made: Int
}

query loans_by_year() -> List<LoansByYear> {
    from: Loan
    group: {
        lent_year: temporal.year(Loan.lent_on)
    }
    aggregate: {
        loans_made: count()
    }
    order: [lent_year desc]
    limit: 20
}
```

Which day a loan falls on is now decided once, in your code, in the zone the action chooses.

**If the field can't be a group**, group by one that can, and write it as a block:

```jadpo fragment
    group: {
        title: Loan.title
    }
```

```jadpo check project=e-group-key-fix-2 hidden
type Title = Text {
    max_length: 80
}

entity Loan {
    id: Uuid
    title: Title
}

type LoansByTitle = Output {
    title: Title
    loans_made: Int
}

query loans_by_title() -> List<LoansByTitle> {
    from: Loan
    group: {
        title: Loan.title
    }
    aggregate: {
        loans_made: count()
    }
    order: [title asc]
    limit: 20
}
```

The query returns one row for each title, with how many loans have it.

## Related

- [The groups](../language/06-reads-and-writes.md#the-groups)
- [The result](../language/06-reads-and-writes.md#the-result)
