# `query.result_rule`

A query returns a type that has rules across its fields.

## Why Jadpo refuses it

A rule across an object's fields is checked whenever a value is made, and a
value that breaks it is a failure an `attempt` maps. A query makes its
result from stored records with no `attempt` around it, so it has nowhere to
report a record that breaks the rule.

## What it looks like

main.jadpo:

```jadpo check project=e-result-rule expect=query.result_rule
type LeafCount = Int {
    min: 1
    max: 5000
}

entity Book {
    id: Uuid
    leaves: LeafCount
}

type ThickBook = Output {
    pick: [Book.id, Book.leaves]
    rule thick: leaves > 1000
}

query thick_books() -> List<ThickBook> {
    from: Book
    order: [Book.id asc]
    limit: 20
}
```

```text output
error: `thick_books` returns `ThickBook`, whose rule `thick` no read can check
 --> main.jadpo:16:24
   |
16 | query thick_books() -> List<ThickBook> {
   |                        ^^^^^^^^^^^^^^^ `ThickBook` has rules
  reason: A rule across an object's fields is checked whenever a value is made, and a value that breaks it is a failure an `attempt` maps; a query makes `ThickBook` from stored records with no `attempt` around it, so its result type has no rules across its fields.
  next: Return a type without rules, and build `ThickBook` from it with `attempt` where its rules are checked (agent)
  rule: query.result_rule
  help: https://jadpo.dev/docs/errors/query.result_rule

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the query should return only records that meet the rule**, say so in `where:`, and return a type without the rule:

```jadpo fragment
type ThickBook = Output {
    pick: [Book.id, Book.leaves]
}

query thick_books() -> List<ThickBook> {
    from: Book
    where: Book.leaves > 1000
    ...
}
```

```jadpo check project=e-result-rule-fix-1 hidden
type LeafCount = Int {
    min: 1
    max: 5000
}

entity Book {
    id: Uuid
    leaves: LeafCount
}

type ThickBook = Output {
    pick: [Book.id, Book.leaves]
}

query thick_books() -> List<ThickBook> {
    from: Book
    where: Book.leaves > 1000
    order: [Book.id asc]
    limit: 20
}
```

The condition now decides which records come back. Where a value needs the rule itself, an `attempt` builds a type with the rule from the query's result, and can map a failure.

## Related

- [What a result holds](../language/06-reads-and-writes.md#what-a-result-holds)
- [Rules across fields](../language/02-values-and-types.md#rules-across-fields)
- [Turning one failure into another](../language/04-failures.md#turning-one-failure-into-another)
