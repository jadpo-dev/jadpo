# `schedule.store_required`

An app has a schedule but no database named to keep its state in, because it has several databases or none.

## Why Jadpo refuses it

Each schedule keeps its next due time and its lease, the claim that
stops two copies of your app running it at once, in a database. With one
database that is the one. With several, any of them could hold it, and
Jadpo won't choose one for you: `application { schedules: { store: } }`
names it, and with no database at all there is nowhere to keep it.

## What it looks like

domains/planning/schedules/tidy_up.jadpo, in an app with two databases,
`archive` and `main`:

```jadpo check project=e-schedule-store file=app.jadpo hidden
application {
    connections: {
        archive: {
            driver: Driver.postgres
            url: config.archive_url
        }
        main: {
            driver: Driver.postgres
            url: config.main_url
        }
    }
}
```

```jadpo check project=e-schedule-store file=config.jadpo hidden
config {
    archive_url: PostgresUrl
    main_url: PostgresUrl
}
```

```jadpo check project=e-schedule-store file=domains/planning/actions/tidy.jadpo hidden
action tidy() -> Unit {}
```

```jadpo check project=e-schedule-store file=domains/planning/schedules/tidy_up.jadpo expect=schedule.store_required
use planning/actions { tidy }

/// Tidies up every quarter of an hour.
schedule tidy_up {
    every: 15m
    run: tidy()
}
```

```text output
error: `tidy_up` has no database named to keep its state in
 --> domains/planning/schedules/tidy_up.jadpo:4:10
  |
4 | schedule tidy_up {
  |          ^^^^^^^ no store for its state
  reason: Each schedule keeps its next due time and the lease that lets one instance run it at a time in a database; this application has `archive` and `main`, and `app.jadpo` names none with `schedules: { store: }`.
  next: Add `schedules: { store: archive }` to `application { }` in `app.jadpo` (human)
  rule: schedule.store_required
  help: https://jadpo.dev/docs/errors/schedule.store_required

check failed: 1 error, 4 files
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which database keeps the state of every schedule in the app.

An entity whose lifecycle has `purge:` gets a schedule that Jadpo writes
for it, which deletes its old records every hour. It keeps its state in
the same database as every other schedule, so with several databases the
lifecycle's `purge:` is refused the same way. Here `Todo` is kept in
`main`:

```jadpo check project=e-purge-schedule-store file=app.jadpo hidden
application {
    connections: {
        archive: {
            driver: Driver.postgres
            url: config.archive_url
        }
        main: {
            driver: Driver.postgres
            url: config.main_url
        }
    }
}
```

```jadpo check project=e-purge-schedule-store file=config.jadpo hidden
config {
    archive_url: PostgresUrl
    main_url: PostgresUrl
}
```

```jadpo check project=e-purge-schedule-store file=domains/planning/entities/todo.jadpo expect=schedule.store_required
/// Something to do.
entity Todo {
    id: Uuid
    deleted_at: Instant?

    store: main

    lifecycle: {
        visible: deleted_at == none
        transitions: {
            delete: {
                when: deleted_at == none
                set: {
                    deleted_at: clock.now
                }
            }
        }
        purge: {
            after: 30d
            since: deleted_at
        }
    }
}
```

```text output
error: `Todo`'s purge has no database named to keep its state in
 --> domains/planning/entities/todo.jadpo:18:9
   |
18 |         purge: {
   |         ^^^^^ a schedule with no store for its state
  reason: Purge runs as a built-in schedule every hour, and each schedule keeps its next due time and the lease that lets one instance run it at a time in a database; this application has `archive` and `main`, and `app.jadpo` names none with `schedules: { store: }`.
  next: Add `schedules: { store: main }` to `application { }` in `app.jadpo` (human)
  rule: schedule.store_required
  help: https://jadpo.dev/docs/errors/schedule.store_required

check failed: 1 error, 3 files
```

## How to fix it

The question is which database should keep the schedules' state. It
holds every schedule's state, the purge schedules' included, so one
database answers it for the whole app.

**If one database should keep it**, name it in `schedules:` in
`app.jadpo`. The next step suggests the first of them, here `archive`:

```jadpo fragment
    schedules: {
        store: archive
    }
```

```jadpo check project=e-schedule-store-fix-1 file=app.jadpo hidden
application {
    connections: {
        archive: {
            driver: Driver.postgres
            url: config.archive_url
        }
        main: {
            driver: Driver.postgres
            url: config.main_url
        }
    }
    schedules: {
        store: archive
    }
}
```

```jadpo check project=e-schedule-store-fix-1 file=config.jadpo hidden
config {
    archive_url: PostgresUrl
    main_url: PostgresUrl
}
```

```jadpo check project=e-schedule-store-fix-1 file=domains/planning/actions/tidy.jadpo hidden
action tidy() -> Unit {}
```

```jadpo check project=e-schedule-store-fix-1 file=domains/planning/schedules/tidy_up.jadpo hidden migrate=initial
use planning/actions { tidy }

/// Tidies up every quarter of an hour.
schedule tidy_up {
    every: 15m
    run: tidy()
}
```

Each schedule's next due time, lease and run records are now kept in
`archive`, in system tables its first migration creates.

**If the schedule is an entity's purge**, the next step suggests the
entity's own database. For `Todo`, kept in `main`:

```jadpo fragment
    schedules: {
        store: main
    }
```

```jadpo check project=e-purge-schedule-store-fix-1 file=app.jadpo hidden
application {
    connections: {
        archive: {
            driver: Driver.postgres
            url: config.archive_url
        }
        main: {
            driver: Driver.postgres
            url: config.main_url
        }
    }
    schedules: {
        store: main
    }
}
```

```jadpo check project=e-purge-schedule-store-fix-1 file=config.jadpo hidden
config {
    archive_url: PostgresUrl
    main_url: PostgresUrl
}
```

```jadpo check project=e-purge-schedule-store-fix-1 file=domains/planning/entities/todo.jadpo hidden migrate=initial
/// Something to do.
entity Todo {
    id: Uuid
    deleted_at: Instant?

    store: main

    lifecycle: {
        visible: deleted_at == none
        transitions: {
            delete: {
                when: deleted_at == none
                set: {
                    deleted_at: clock.now
                }
            }
        }
        purge: {
            after: 30d
            since: deleted_at
        }
    }
}
```

`Todo.purge`'s state and run records are now kept in `main`, beside the
`Todo` records it deletes.

## Related

- [Where a schedule's state is kept](../language/12-jobs-and-events.md#where-a-schedules-state-is-kept)
- [The purge schedule](../language/12-jobs-and-events.md#the-purge-schedule)
- [Schedules](../language/12-jobs-and-events.md#schedules)
- [Stores](../language/05-data.md#stores)
