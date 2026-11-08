# `semantic.revocation_delay_invalid`

An application's `revocation_delay:` isn't a duration above zero.

## Why Jadpo refuses it

`revocation_delay:` is how long a disabled caller or a revoked session may
keep working. A delay of zero means nothing to wait for, which is what
leaving the setting out already does, by checking the caller's record on
every request.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/semantic.revocation_delay_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A value that isn't a duration is refused the same way:

```jadpo check project=e-revocation-delay-text expect=semantic.revocation_delay_invalid
application {
    revocation_delay: "five minutes"
}
```

```text output
error: `"five minutes"` can't be a revocation delay
 --> main.jadpo:2:23
  |
2 |     revocation_delay: "five minutes"
  |                       ^^^^^^^^^^^^^^ not a duration
  reason: `revocation_delay:` is how long a disabled caller or a revoked session may keep working, a duration above zero such as `5m`; leaving it out checks the caller's record on every request.
  next: Write a duration such as `5m`, or leave `revocation_delay:` out (agent)
  rule: semantic.revocation_delay_invalid
  help: https://jadpo.dev/docs/errors/semantic.revocation_delay_invalid

check failed: 1 error, 1 file
```

## How to fix it

**If you want the delay**, write a duration above zero:

```jadpo check project=e-revocation-delay-fix-1
application {
    revocation_delay: 5m
}
```

A disabled caller can now keep working for up to five minutes, and most
requests don't read the database to check them.

**If every request should check the caller's record**, leave it out:

```jadpo check project=e-revocation-delay-fix-2
application {}
```

Disabling a caller now takes effect on their next request.

## Related

- [Fewer database reads: `revocation_delay:`](../language/08-identity.md#fewer-database-reads-revocation_delay)
