# `data.unique_nullable_field`

A unique rule over several fields includes a field that can be `none`.

## Why Jadpo refuses it

Whether two records are duplicates has to mean the same in every engine
that checks it. A rule over several fields therefore takes required fields
only, so two records either match on every field or they don't.

## What it looks like

main.jadpo:

```jadpo check project=e-unique-nullable expect=data.unique_nullable_field
entity Account {
    id: Uuid
    tenant: Text
    handle: Text?

    unique: {
        tenant_handle: [tenant, handle]
    }
}
```

```text output
error: `Account.handle` can be `none`
 --> main.jadpo:7:33
  |
7 |         tenant_handle: [tenant, handle]
  |                                 ^^^^^^ optional field
  reason: The fields of a unique rule over several fields must be required, so every engine agrees on what a duplicate is.
  next: Make `handle` required, or leave it out of `tenant_handle` (agent)
  rule: data.unique_nullable_field
  help: https://jadpo.dev/docs/errors/data.unique_nullable_field

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If every account has a handle**, make the field required:

```jadpo fragment
    handle: Text
```

```jadpo check project=e-unique-nullable-fix hidden
entity Account {
    id: Uuid
    tenant: Text
    handle: Text

    unique: {
        tenant_handle: [tenant, handle]
    }
}
```

Within a tenant, no two accounts can now share a handle.

**If a handle really is optional**, remove the rule:

```jadpo check project=e-unique-nullable-fix-2
entity Account {
    id: Uuid
    tenant: Text
    handle: Text?
}
```

Accounts can now have no handle, and any number can share one.

## Related

- [Unique rules](../language/05-data.md#unique-rules)
