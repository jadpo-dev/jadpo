# `schedule.settings_invalid`

`schedules: { store: }` names a database:

```jadpo check project=e-schedules-settings file=app.jadpo expect=schedule.settings_invalid
application {
    schedules: {
        store: archive
    }
}
```

```text output
error: `store: archive` isn't a database
 --> app.jadpo:3:16
  |
3 |         store: archive
  |                ^^^^^^^ not a database
  reason: A schedule's next due time and its lease are kept in a database; this application's databases are `main`.
  next: Name a database connection (agent)
  rule: schedule.settings_invalid
  help: docs/language/12-jobs-and-events.html#schedule-settings-invalid

check failed: 1 error, 1 file
```

`trigger:` is a `ScheduleTrigger`:

```jadpo check project=e-schedules-trigger file=app.jadpo expect=schedule.settings_invalid
application {
    schedules: {
        trigger: ScheduleTrigger.cron
    }
}
```

```text output
error: `ScheduleTrigger.cron` isn't a schedule trigger
 --> app.jadpo:3:18
  |
3 |         trigger: ScheduleTrigger.cron
  |                  ^^^^^^^^^^^^^^^^^^^^ not a `ScheduleTrigger`
  reason: A native deployable ticks on the server's own timer, `ScheduleTrigger.native`, or when another scheduler runs its `tick` command, `ScheduleTrigger.external`; a Worker always ticks on Cloudflare's cron.
  next: Use `ScheduleTrigger.external` instead (agent)
  rule: schedule.settings_invalid
  help: docs/language/12-jobs-and-events.html#schedule-settings-invalid

check failed: 1 error, 1 file
```
