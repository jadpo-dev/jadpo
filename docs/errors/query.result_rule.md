# `query.result_rule`

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
  help: docs/language/06-reads-and-writes.html#query-result-rule

check failed: 1 error, 1 file
```

A query that needs only the thick books says so in `where:`, and a type
with the rule is built from its result where a failure can be mapped.
