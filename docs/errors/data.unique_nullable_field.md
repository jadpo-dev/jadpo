# `data.unique_nullable_field`

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
