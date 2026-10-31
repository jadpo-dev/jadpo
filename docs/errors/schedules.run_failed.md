# `schedules.run_failed`

A schedule run by hand ended in a failure, or its run on the app never ended.

## Why Jadpo refuses it

`jadpo schedules run` runs a schedule now and shows how the run went.
When the schedule's action ends with a failure, as `send_digest` does
here, the run is recorded as failed, like one at its own time, and the
schedule's next due time is its retry. The command exits 1, and
`--json` gives the failure as this rule, so a script that ran it knows.
A run asked of a deployment's tick that holds its lease past the lease's
length was lost with its worker, and fails the same way.

Jadpo won't run it again, or change what the action does: whether the
cause is in the code, in a setting or in a service the action reaches,
only you can tell.

## What it looks like

A project whose `morning_digest` runs `send_digest` once a day, which
fails until a mail service is set up:

```jadpo check project=e-schedules-run-failed file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

```jadpo check project=e-schedules-run-failed file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=e-schedules-run-failed file=domains/news/failures/no_mail_service.jadpo hidden
type NoMailService = Failure {
    kind: FailureKind.unavailable
    message: "No mail service is set up yet."
}
```

domains/news/actions/send_digest.jadpo:

```jadpo check project=e-schedules-run-failed file=domains/news/actions/send_digest.jadpo
use news/failures { NoMailService }

/// Sends the morning digest, once a mail service is set up.
action send_digest() fails NoMailService -> Unit {
    reject NoMailService
}
```

domains/news/schedules/morning_digest.jadpo:

```jadpo check project=e-schedules-run-failed file=domains/news/schedules/morning_digest.jadpo migrate=initial
use news/actions { send_digest }

/// Sends the digest every morning.
schedule morning_digest {
    every: 1d
    run: send_digest()
}
```

Running a schedule in the command's own process needs `jadpo dev`
running beside it, which a checked run doesn't keep, so this output is
copied rather than checked. It is `jadpo schedules run` on a Mac, while
`jadpo dev` ran the project in another terminal:

```sh
jadpo schedules run morning_digest
```

```text
failed: `morning_digest`
  failure: no_mail_service · unavailable
  took: 4 ms

next: jadpo why morning_digest  explains the run
```

The screen shows the failed run; with `--json`, the diagnostic's next
step is to fix the cause, then run `jadpo schedules run morning_digest`
again. `(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides how `send_digest` gets the digest out.

## How to fix it

The question is where the cause is.

**If the cause is in the action**, see the run's failure and the
schedule's file, fix the action, then run the schedule again:

```sh
jadpo why morning_digest
```

`jadpo why` names the failure, its kind and the file the schedule is
declared in, which is where the fix starts.

**If the cause is a setting or a service the action reaches**, fix it
there, then run the schedule again:

```sh
jadpo schedules run morning_digest
```

The run is recorded as succeeded, and the schedule's next run is at its
own time again.

## Related

- [Running a schedule by hand](../language/12-jobs-and-events.md#running-a-schedule-by-hand)
- [When a run fails](../language/12-jobs-and-events.md#when-a-run-fails)
- [`jadpo schedules`](../language/14-tooling.md#jadpo-schedules)
