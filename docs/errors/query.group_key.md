# `query.group_key`

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
  help: docs/errors/query.group_key.md

check failed: 1 error, 1 file
```
