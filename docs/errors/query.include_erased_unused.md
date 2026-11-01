# `query.include_erased_unused`

A query says `include_erased: true` on an entity that keeps nothing of an erased person.

## Why Jadpo refuses it

`include_erased: true` lets a query find the record erasing a person kept,
for the reason the law keeps it. An entity keeps one only when a field of
its own says `erase: Erasure.retained`. Otherwise erasing a person deletes
their whole record, so there is nothing for the query to find. Accepted, the
line would read as though the query reaches people it can't.

## What it looks like

main.jadpo:

```jadpo check project=e-include-erased-unused expect=query.include_erased_unused
entity Customer {
    id: Uuid
    email: Email {
        personal: PersonalData.contact
    }

    person: true
}

type CustomerRow = Output {
    pick: [Customer.id]
}

query every_customer() -> List<CustomerRow> {
    from: Customer
    order: [Customer.id asc]
    limit: 10
    include_erased: true
}
```

```text output
error: `every_customer` includes erased people, and erasing a `Customer` keeps nothing to find
 --> main.jadpo:18:5
   |
18 |     include_erased: true
   |     ^^^^^^^^^^^^^^ nothing to include
  reason: Erasing a `Customer` deletes their whole record, since none of its fields says `erase: Erasure.retained`, so nothing is left for `include_erased:` to find.
  next: Remove `include_erased: true` (agent)
  rule: query.include_erased_unused
  help: https://jadpo.dev/docs/errors/query.include_erased_unused

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether the law makes you keep part of an erased customer's
record.

**If it doesn't**, remove the line:

```jadpo fragment
query every_customer() -> List<CustomerRow> {
    from: Customer
    order: [Customer.id asc]
    limit: 10
}
```

```jadpo check project=e-include-erased-unused-removed hidden
entity Customer {
    id: Uuid
    email: Email {
        personal: PersonalData.contact
    }

    person: true
}

type CustomerRow = Output {
    pick: [Customer.id]
}

query every_customer() -> List<CustomerRow> {
    from: Customer
    order: [Customer.id asc]
    limit: 10
}
```

The query now reads every customer there is, and an erased one is gone.

**If it does**, say so on the field that stays
([The person's own record](../language/15-personal-data.md#the-persons-own-record)):

```jadpo fragment
policy_number: PolicyNumber {
    personal: PersonalData.identifying
    erase: Erasure.retained
}
```

```jadpo check project=e-include-erased-unused-kept hidden
config {
    policy_retention: Duration {
        default: 1826d
    }
}

type PolicyNumber = Text {
    max_length: 20
}

entity Customer {
    id: Uuid
    email: Email? {
        personal: PersonalData.contact
    }
    policy_number: PolicyNumber {
        personal: PersonalData.identifying
        erase: Erasure.retained
    }
    policy_ended_at: Instant?

    person: true

    retention: {
        after: config.policy_retention
        since: policy_ended_at
    }
}

type CustomerRow = Output {
    pick: [Customer.id]
}

query every_customer() -> List<CustomerRow> {
    from: Customer
    order: [Customer.id asc]
    limit: 10
    include_erased: true
}
```

The query now also finds customers who have been erased, and reads only
their `id`.

## Related

- [Reading what is kept](../language/15-personal-data.md#reading-what-is-kept-include_erased)
- [`query.include_erased_reads`](query.include_erased_reads.md)
