# `type.password_hash_exposed`

An `Output`, a request's `Input` or a failure's fields hold a `PasswordHash`, which never leaves the server.

## Why Jadpo refuses it

A `PasswordHash` is stored so that `crypto.verify_password` can check a
password against it, and no client ever needs one. Anyone who gets a hash
can guess passwords against it offline, at their own pace. So an `Output`,
a request's `Input` and a failure's fields refuse one, however deep it is
inside them, and so do `json.write` and `json.read`.

## What it looks like

main.jadpo:

```jadpo check project=e-password-hash-exposed expect=type.password_hash_exposed
type StaffName = Text {
    max_length: 100
}

/// What the staff page shows of a member of staff.
type StaffProfile = Output {
    name: StaffName
    hashed_password: PasswordHash
}
```

```text output
error: `StaffProfile.hashed_password` holds a password's hash
 --> main.jadpo:8:22
  |
8 |     hashed_password: PasswordHash
  |                      ^^^^^^^^^^^^ sent to a client
  reason: A `PasswordHash` is stored and checked with `crypto.verify_password`, and never leaves the server. A client never needs a password's hash, and anyone who gets one can guess passwords against it offline, at their own pace.
  next: Remove `hashed_password`, and send what the client needs instead, such as whether a password is set (agent)
  rule: type.password_hash_exposed
  help: https://jadpo.dev/docs/errors/type.password_hash_exposed

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is what the client needs to know.

**If the client needs to know whether a password is set**, send that
instead:

```jadpo fragment
type StaffProfile = Output {
    name: StaffName
    password_set: Bool
}
```

```jadpo check project=e-password-hash-exposed-fix-1 hidden
type StaffName = Text {
    max_length: 100
}

/// What the staff page shows of a member of staff.
type StaffProfile = Output {
    name: StaffName
    password_set: Bool
}
```

The client now gets `true` or `false`, and never the hash.

**If the client doesn't need it at all**, remove the field:

```jadpo fragment
type StaffProfile = Output {
    name: StaffName
}
```

```jadpo check project=e-password-hash-exposed-fix-2 hidden
type StaffName = Text {
    max_length: 100
}

/// What the staff page shows of a member of staff.
type StaffProfile = Output {
    name: StaffName
}
```

`StaffProfile` now holds only what the page shows.

## Related

- [Password hashes](../language/19-standard-library.md#password-hashes)
- [Where a secret can go](../language/19-standard-library.md#where-a-secret-can-go)
