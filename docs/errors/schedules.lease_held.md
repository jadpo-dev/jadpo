# `schedules.lease_held`

A schedule was run by hand while another run of it still holds its lease, so it wasn't run again.

## Why Jadpo refuses it

A schedule runs once at a time. A run takes the schedule's lease, a
claim with an end time on its state row, and keeps it until it ends.
`jadpo schedules run` takes the same lease, so it can't start a run
beside one still going, whichever instance of the app started that one.

Jadpo won't take the lease from the run that holds it, or run beside it:
two runs of one schedule could each do its work, such as sending the
digest twice. Whether to wait for it, or find out why it is taking so
long, is yours to decide.

## What it looks like

A project whose `morning_digest` runs `send_digest` once a day:

```jadpo check project=e-schedules-lease-held file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

```jadpo check project=e-schedules-lease-held file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=e-schedules-lease-held file=domains/news/failures/no_mail_service.jadpo hidden
type NoMailService = Failure {
    kind: FailureKind.unavailable
    message: "No mail service is set up yet."
}
```

```jadpo check project=e-schedules-lease-held file=domains/news/actions/send_digest.jadpo hidden
use news/failures { NoMailService }

/// Sends the morning digest, once a mail service is set up.
action send_digest() fails NoMailService -> Unit {
    reject NoMailService
}
```

domains/news/schedules/morning_digest.jadpo:

```jadpo check project=e-schedules-lease-held file=domains/news/schedules/morning_digest.jadpo migrate=initial
use news/actions { send_digest }

/// Sends the digest every morning.
schedule morning_digest {
    every: 1d
    run: send_digest()
}
```

No run here lasts long enough to catch it holding the lease, so this
output is copied rather than checked. It is `jadpo schedules run` on a
Mac, against `jadpo dev`'s database, with a run of `morning_digest` that
started a minute before written into its tables by hand, as a run still
going leaves them:

```sh
jadpo schedules run morning_digest
```

```text
error: `morning_digest` is already running
  reason: A run that started at 21:13:00 holds its lease until 21:29:00, and only one run of a schedule happens at a time.
  next: Wait for it to end, or see it with `jadpo why morning_digest` (human)
  rule: schedules.lease_held
  help: https://jadpo.dev/docs/errors/schedules.lease_held
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether a second run of `morning_digest` happens.

## How to fix it

The question is whether the run holding the lease is still working.

**If it is still working**, wait for it to end, and see how it went:

```sh
jadpo why morning_digest
```

`jadpo why` shows the run while it is going, and its outcome once it
has ended; the schedule is then free to run again.

**If its worker was lost**, as when the app stopped during the run,
wait for the lease to end, then run it again:

```sh
jadpo schedules run morning_digest
```

The next tick after the lease ends records the lost run as
`lease_expired`, and the run by hand goes ahead.

## Related

- [Running a schedule by hand](../language/12-jobs-and-events.md#running-a-schedule-by-hand)
- [How a schedule runs](../language/12-jobs-and-events.md#how-a-schedule-runs)
- [`jadpo schedules`](../language/14-tooling.md#jadpo-schedules)
