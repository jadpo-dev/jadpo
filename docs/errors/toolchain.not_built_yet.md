# `toolchain.not_built_yet`

The language declares some operations that aren't built yet, such as the
generated `oauth.approve` (area 8). Calling one is an error rather than a
program that can't run:

```jadpo check project=e-not-built-yet expect=toolchain.not_built_yet
type ConsentId = Uuid {}

action approve_consent(consent: ConsentId) -> Unit {
    oauth.approve(consent)
}
```

```text output
error: `oauth.approve` isn't built yet
 --> main.jadpo:4:5
  |
4 |     oauth.approve(consent)
  |     ^^^^^^^^^^^^^ declared, not built
  reason: The language declares `oauth.approve`, but this compiler has nothing to run it with yet, so a program that calls it couldn't run.
  next: Do without `oauth.approve` for now (agent)
  rule: toolchain.not_built_yet
  help: docs/language/03-logic.html#toolchain-not-built-yet

check failed: 1 error, 1 file
```
