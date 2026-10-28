# `deployable.empty`

A deployable that serves no API and runs no background work would only
answer its health checks:

```jadpo check project=e-deployable-empty expect=deployable.empty file=app.jadpo
application {
    deployables: {
        kiosk: {
            target: Target.native
        }
    }
}
```

```text output
error: `kiosk` serves nothing and runs nothing
 --> app.jadpo:3:9
  |
3 |         kiosk: {
  |         ^^^^^ does nothing
  reason: A deployable serves APIs, `serves: [main]`, runs background work, `runs: [Trigger.schedules]`, or both; one that does neither would only answer its health checks.
  next: Give `kiosk` `serves:` or `runs:`, or remove it (agent)
  rule: deployable.empty
  help: docs/errors/deployable.empty.md

check failed: 1 error, 1 file
```
