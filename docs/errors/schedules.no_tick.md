# `schedules.no_tick`

A schedule was run by hand against a deployment, and no tick of the app picked the run up in time, so nothing ran.

## Why Jadpo refuses it

Against a deployment's database, `jadpo schedules run` runs nothing
itself. It writes a request on the schedule's state row and waits for
the app's own tick to run it, so the deployment's settings and secrets
never leave its servers. A tick comes every period the app ticks at,
while the app runs; when none takes the request within twice that, the
command takes the request back and stops. Otherwise the run would happen
whenever the app next starts, perhaps much later.

Jadpo won't run the schedule on your machine instead: that would need
the deployment's settings and secrets, and would run beside its own
instances. Whether the app should be running, and where, only you know.

## What it looks like

A project whose `heartbeat` runs every second, so the app ticks every
second:

```jadpo check project=e-schedules-no-tick file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

```jadpo check project=e-schedules-no-tick file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=e-schedules-no-tick file=domains/health/actions/beat.jadpo hidden
/// Does nothing: its runs show the app is ticking.
action beat() -> Unit {}
```

domains/health/schedules/heartbeat.jadpo:

```jadpo check project=e-schedules-no-tick file=domains/health/schedules/heartbeat.jadpo migrate=initial
use health/actions { beat }

/// Shows the app is ticking, every second.
schedule heartbeat {
    every: 1s
    run: beat()
}
```

`jadpo schedules run` against a database the migrations are applied to,
with no app running against it:

```sh run project=e-schedules-no-tick database expect=schedules.no_tick
jadpo migrate apply
jadpo schedules run heartbeat
```

```text output
Applied 1 migration to `main`
    20261020T1400_initial  applied
error: No tick picked up `heartbeat`'s run
  reason: The app's own server runs it on its next tick, which comes every 1 s while the app runs; none came, so the request was taken back, and nothing ran.
  next: Check the app is running, then run `jadpo schedules run heartbeat` again (human)
  rule: schedules.no_tick
  help: https://jadpo.dev/docs/errors/schedules.no_tick
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides where the schedule runs.

## How to fix it

The question is whether the schedule should run on the deployment or on
your machine.

**If it should run on the deployment**, start the app there, then run it
again:

```sh
jadpo schedules run heartbeat
```

The app's next tick takes the request and runs `heartbeat`, and the
command shows how the run went.

**If it should run on your machine**, start `jadpo dev` in another
terminal, and run it against `jadpo dev`'s own database:

```sh
unset DATABASE_URL
jadpo schedules run heartbeat
```

The command runs `heartbeat` itself, straight away, and shows how the
run went.

## Related

- [Running a schedule by hand](../language/12-jobs-and-events.md#running-a-schedule-by-hand)
- [The tick](../language/12-jobs-and-events.md#the-tick)
- [`jadpo schedules`](../language/14-tooling.md#jadpo-schedules)
