# `query.include_erased_reads`

A query that finds erased people reads more than their `id` and the fields erasure keeps of them.

## Why Jadpo refuses it

Erasing a person can keep part of their record, because the law makes you
keep it, and hides the rest of them from every query. `include_erased: true`
lets a query find that record for the reason it was kept, and for nothing
else. So it reads only the person's `id` and the fields that say `erase:
Erasure.retained`, in its result, its conditions and its order, for everyone
it finds, erased or not, and it brings no other records with it. Accepted, a
query could use a kept record to learn about someone who asked to be
forgotten.

## What it looks like

main.jadpo:

```jadpo check project=e-include-erased-reads expect=query.include_erased_reads
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

type KeptCustomer = Output {
    pick: [Customer.id, Customer.email]
}

query kept_customers() -> List<KeptCustomer> {
    from: Customer
    order: [Customer.id asc]
    limit: 10
    include_erased: true
}
```

```text output
error: `kept_customers` reads `Customer.email`, which isn't `erase: Erasure.retained`
 --> main.jadpo:38:5
   |
38 |     include_erased: true
   |     ^^^^^^^^^^^^^^ reads what erasure kept
  reason: `include_erased:` lets a query read a person's kept record for the reason the law keeps it, and nothing else: their `id` and the fields that say `erase: Erasure.retained`. Erasure clears a marked field, and an unmarked one isn't kept for that reason.
  next: Stop reading `email` here: leave it out of the result, the conditions and the order (human)
  rule: query.include_erased_reads
  help: https://jadpo.dev/docs/errors/query.include_erased_reads

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides what a person's kept record is used for.

An include is refused the same way: the records it brings aren't what the
law keeps.

## How to fix it

The question is why the query reads the person.

**If it serves the reason the record was kept**, read only what is kept:

```jadpo fragment
type KeptCustomer = Output {
    pick: [Customer.id, Customer.policy_number]
}
```

```jadpo check project=e-include-erased-reads-kept hidden
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

type KeptCustomer = Output {
    pick: [Customer.id, Customer.policy_number]
}

query kept_customers() -> List<KeptCustomer> {
    from: Customer
    order: [Customer.id asc]
    limit: 10
    include_erased: true
}
```

The query now reads each customer's policy number, erased or not, and
nothing else about them.

**If it is about the people who haven't been erased**, remove
`include_erased: true`:

```jadpo fragment
query customers() -> List<KeptCustomer> {
    from: Customer
    order: [Customer.id asc]
    limit: 10
}
```

```jadpo check project=e-include-erased-reads-removed hidden
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

type KeptCustomer = Output {
    pick: [Customer.id, Customer.email]
}

query customers() -> List<KeptCustomer> {
    from: Customer
    order: [Customer.id asc]
    limit: 10
}
```

The query now reads anything it likes of the customers who haven't been
erased, and finds no one else.

## Related

- [Reading what is kept](../language/15-personal-data.md#reading-what-is-kept-include_erased)
- [The person's own record](../language/15-personal-data.md#the-persons-own-record)
- [Named queries](../language/06-reads-and-writes.md#named-queries)
