# `semantic.revocation_delay_invalid`

```jadpo check project=e-revocation-delay-invalid expect=semantic.revocation_delay_invalid
application {
    revocation_delay: 0m
}
```

```text output
error: `0m` can't be a revocation delay
 --> main.jadpo:2:23
  |
2 |     revocation_delay: 0m
  |                       ^^ not above zero
  reason: `revocation_delay:` is how long a disabled caller or a revoked session may keep working, a duration above zero such as `5m`; leaving it out checks the caller's record on every request.
  next: Write a duration such as `5m`, or leave `revocation_delay:` out (agent)
  rule: semantic.revocation_delay_invalid
  help: docs/language/08-identity.html#semantic-revocation-delay-invalid

check failed: 1 error, 1 file
```
