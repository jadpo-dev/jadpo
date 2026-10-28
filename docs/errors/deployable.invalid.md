# `deployable.invalid`

`deployables:` names each deployable with a block holding `serves:`, a
list of APIs, `runs:`, a list of triggers, and `target:`, and nothing
else; an entry is listed once:

```jadpo check project=e-deployable-invalid expect=deployable.invalid file=app.jadpo
application {
    deployables: {
        kiosk: {
            serves: main
            target: Target.native
        }
    }
}
```

```text output
error: `kiosk`'s `serves:` isn't a list
 --> app.jadpo:4:21
  |
4 |             serves: main
  |                     ^^^^ not a list
  reason: `serves:` is a list, `serves: [<api>]`.
  next: Write `serves: [<api>]` (agent)
  rule: deployable.invalid
  help: docs/language/14-tooling.html#deployable-invalid

check failed: 1 error, 1 file
```
