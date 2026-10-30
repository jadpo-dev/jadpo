# `type.password_hash_exposed`

A `PasswordHash` is stored and checked, and never leaves the server: an
`Output`, a request's `Input` and a failure's fields refuse one, however
deep, as `json.write` and `json.read` do. A client that needs to know
whether a password is set is sent that instead.

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
