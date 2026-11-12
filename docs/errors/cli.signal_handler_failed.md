# `cli.signal_handler_failed`

SIGTERM and ctrl-c couldn't be watched, so `jadpo dev` runs without them, and a signal ends it at once.

## Why Jadpo refuses it

`jadpo dev` watches SIGTERM and ctrl-c so that stopping it is clean: the
server finishes the requests it has, and its background work gives back
what it holds, so the next session runs that work straight away. Watching
them needs a thread and a file descriptor of the process's own, and when
the process can't have either, as when it has every file descriptor its
limit allows open, the signals go unwatched.

Jadpo still starts the app, since you can work with it all the same, and
says so once, as it starts. Without the signals, ctrl-c or SIGTERM ends
`jadpo dev` at once: a delivery, a batch run or a schedule's run that was
under way keeps its lease until the lease ends, and a tick runs it again
after that. Whether to carry on like that, or restart where the signals
can be watched, is yours to decide.

## What it looks like

A project whose app has a schedule:

```jadpo check project=e-signal-handler-failed file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

```jadpo check project=e-signal-handler-failed file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=e-signal-handler-failed file=domains/news/failures/no_mail_service.jadpo hidden
type NoMailService = Failure {
    kind: FailureKind.unavailable
    message: "No mail service is set up yet."
}
```

```jadpo check project=e-signal-handler-failed file=domains/news/actions/send_digest.jadpo hidden
use news/failures { NoMailService }

/// Sends the morning digest, once a mail service is set up.
action send_digest() fails NoMailService -> Unit {
    reject NoMailService
}
```

domains/news/schedules/morning_digest.jadpo:

```jadpo check project=e-signal-handler-failed file=domains/news/schedules/morning_digest.jadpo migrate=initial
use news/actions { send_digest }

/// Sends the digest every morning.
schedule morning_digest {
    every: 1d
    run: send_digest()
}
```

The signals can be watched on any machine with file descriptors to
spare, so this output is copied rather than checked. It is `jadpo dev`
on a Mac, started from a shell whose open-file limit its other programs
had used up:

```sh
jadpo dev
```

```text
jadpo dev, started 09:12:03
error: SIGTERM and ctrl-c can't be watched, so `jadpo dev` runs without them
  reason: Watching them failed: Too many open files (os error 24). A signal now ends `jadpo dev` at once, and background work it was running waits for its lease to end before a tick runs it again.
  next: Stop `jadpo dev` and run it again; if the reason says too many files are open, raise the limit first with `ulimit -n 4096` (human)
  rule: cli.signal_handler_failed
  help: https://jadpo.dev/docs/errors/cli.signal_handler_failed
```

The app is served after it, as it is when the signals are watched.

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether this session stops cleanly.

## How to fix it

The question is what the reason says stopped the signals being watched.

**If too many files are open**, stop `jadpo dev`, raise the shell's
limit and start it again:

```sh
ulimit -n 4096
jadpo dev
```

The session then watches the signals, and ctrl-c stops it cleanly,
giving back what its background work holds.

**If it says anything else**, stop `jadpo dev` and start it again:

```sh
jadpo dev
```

A session that could watch the signals prints no error, and stops
cleanly at the first signal.

## Related

- [Running background work](../language/12-jobs-and-events.md#running-background-work)
- [`jadpo dev`](../language/14-tooling.md#jadpo-dev)
