# `schedule.store_required`

With several databases, `schedules: { store: }` says which keeps every schedule's state and lease; with one, it is that one:

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

An entity's built-in purge schedule keeps its state in the same store, so
with several databases a lifecycle that purges needs `schedules: { store: }`
too:

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
