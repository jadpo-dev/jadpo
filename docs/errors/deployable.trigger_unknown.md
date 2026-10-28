# `deployable.trigger_unknown`

`runs:` lists `Trigger.schedules` and `Trigger.subscribers`:

```jadpo check project=e-deployable-trigger-unknown expect=deployable.trigger_unknown file=app.jadpo
application {
    deployables: {
        kiosk: {
            serves: [main]
            runs: [Trigger.jobs]
            target: Target.native
        }
    }
}
```

```text output
error: `Trigger.jobs` isn't background work a deployable runs
 --> app.jadpo:5:20
  |
5 |             runs: [Trigger.jobs]
  |                    ^^^^^^^^^^^^ not a `Trigger`
  reason: `runs:` lists `Trigger.schedules`, the schedules, purges and polls, and `Trigger.subscribers`, the deliveries of events and emails.
  next: Use `Trigger.schedules` or `Trigger.subscribers` (agent)
  rule: deployable.trigger_unknown
  help: docs/language/14-tooling.html#deployable-trigger-unknown

check failed: 1 error, 1 file
```
