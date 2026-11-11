# `schedule.settings_invalid`

The application's `schedules:` holds a `store:` that isn't one of its databases, a `trigger:` that isn't a `ScheduleTrigger`, or a setting it doesn't take.

## Why Jadpo refuses it

`application { schedules: }` holds two settings. `store:` names the
database that keeps each schedule's next due time and the lease that
stops two copies of the app running it at once. `trigger:` says what
ticks a native deployable: its own timer, `ScheduleTrigger.native`, or
another scheduler running its `tick` command, `ScheduleTrigger.external`.
A store that isn't a database has no table to keep the state in, and
a trigger Jadpo doesn't know would leave the schedules never ticking, so
it refuses either rather than guess.

## What it looks like

app.jadpo:

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
  help: https://jadpo.dev/docs/errors/schedule.settings_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A `trigger:` that isn't one of `ScheduleTrigger`'s is the same error:

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
  help: https://jadpo.dev/docs/errors/schedule.settings_invalid

check failed: 1 error, 1 file
```

## How to fix it

**If `store:` names the wrong thing**, name one of the application's
databases. In `app.jadpo`:

```jadpo fragment
    schedules: {
        store: main
    }
```

```jadpo check project=e-schedules-settings-fix-1 file=app.jadpo hidden
application {
    schedules: {
        store: main
    }
}
```

Each schedule's state and lease are now kept in `main`.

**If the server should tick on another scheduler's timer**, such as a
Kubernetes CronJob or a systemd timer, write `ScheduleTrigger.external`. In `app.jadpo`:

```jadpo fragment
    schedules: {
        trigger: ScheduleTrigger.external
    }
```

```jadpo check project=e-schedules-settings-fix-2 file=app.jadpo hidden
application {
    schedules: {
        trigger: ScheduleTrigger.external
    }
}
```

The native deployable no longer ticks on its own timer; schedules run
when the other scheduler runs the deployable's `tick` command. On Workers it changes
nothing, since a Worker always ticks on Cloudflare's cron.

## Related

- [Where a schedule's state is kept](../language/12-jobs-and-events.md#where-a-schedules-state-is-kept)
- [The tick](../language/12-jobs-and-events.md#the-tick)
- [Ticking from another scheduler: `ScheduleTrigger.external`](../language/12-jobs-and-events.md#ticking-from-another-scheduler-scheduletriggerexternal)
