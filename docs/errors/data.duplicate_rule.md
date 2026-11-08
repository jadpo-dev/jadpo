# `data.duplicate_rule`

A unique rule or an index covers the same fields as another, or an index gives one of its settings twice.

## Why Jadpo refuses it

A unique rule is a set of fields. Two rules over the same fields, in
whatever order, refuse exactly the same writes, and a write's `conflict:`
block couldn't say which of them was broken. Two indexes listing the same
fields in the same order are one index the database would keep up to date
twice; the same fields in a different order are a different index, and are
allowed.

## What it looks like

main.jadpo:

```jadpo check project=e-duplicate-rule expect=data.duplicate_rule
entity Account {
    id: Uuid
    tenant: Text
    handle: Text

    unique: {
        tenant_handle: [tenant, handle]
        handle_tenant: [handle, tenant]
    }
}
```

```text output
error: The unique rule `handle_tenant` repeats `tenant_handle`
 --> main.jadpo:8:9
  |
7 |         tenant_handle: [tenant, handle]
  |         ------------- first here
8 |         handle_tenant: [handle, tenant]
  |         ^^^^^^^^^^^^^ same fields
  reason: Both cover the same fields, in whatever order, so they are one rule; a write's conflict names one of them (area 6).
  next: Remove `handle_tenant` (agent)
  rule: data.duplicate_rule
  help: https://jadpo.dev/docs/errors/data.duplicate_rule

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Two indexes over the same fields, in the same order, are refused the same
way:

```jadpo check project=e-duplicate-index expect=data.duplicate_rule
entity Todo {
    id: Uuid
    due_at: Instant

    indexes: {
        by_due_at: [due_at]
        soonest_first: [due_at]
    }
}
```

```text output
error: The index `soonest_first` repeats `by_due_at`
 --> main.jadpo:7:9
  |
6 |         by_due_at: [due_at]
  |         --------- first here
7 |         soonest_first: [due_at]
  |         ^^^^^^^^^^^^^ same fields
  reason: Both list the same fields in the same order, so the database would keep the same index twice.
  next: Remove `soonest_first` (agent)
  rule: data.duplicate_rule
  help: https://jadpo.dev/docs/errors/data.duplicate_rule

check failed: 1 error, 1 file
```

An index that gives one of its settings twice is refused too, since a
setting has one value:

```jadpo check project=e-duplicate-setting expect=data.duplicate_rule
type ArticleMeaning = Vector {
    dimensions: 1536
}

entity Article {
    id: Uuid
    meaning: ArticleMeaning

    indexes: {
        by_meaning: {
            fields: [meaning]
            kind: IndexKind.hnsw
            m: 24
            m: 32
        }
    }
}
```

```text output
error: The index `by_meaning` sets `m` twice
 --> main.jadpo:14:13
   |
14 |             m: 32
   |             ^ set again
  reason: An index's `m:` has one value.
  next: Remove the second `m` (agent)
  rule: data.duplicate_rule
  help: https://jadpo.dev/docs/errors/data.duplicate_rule

check failed: 1 error, 1 file
```

## How to fix it

**If two rules or indexes cover the same fields**, keep one and remove the
other:

```jadpo fragment
    unique: {
        tenant_handle: [tenant, handle]
    }
```

```jadpo check project=e-duplicate-rule-fix-1 hidden
entity Account {
    id: Uuid
    tenant: Text
    handle: Text

    unique: {
        tenant_handle: [tenant, handle]
    }
}
```

A write that breaks the rule now has one name to map in `conflict:`.

**If an index gives a setting twice**, keep the value you want:

```jadpo fragment
        by_meaning: {
            fields: [meaning]
            kind: IndexKind.hnsw
            m: 24
        }
```

```jadpo check project=e-duplicate-rule-fix-2 hidden
type ArticleMeaning = Vector {
    dimensions: 1536
}

entity Article {
    id: Uuid
    meaning: ArticleMeaning

    indexes: {
        by_meaning: {
            fields: [meaning]
            kind: IndexKind.hnsw
            m: 24
        }
    }
}
```

The index is built with the one value of `m` it now has.

## Related

- [Unique rules](../language/05-data.md#unique-rules)
- [Indexes](../language/05-data.md#indexes)
