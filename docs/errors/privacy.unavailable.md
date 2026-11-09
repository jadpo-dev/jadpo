# `privacy.unavailable`

The app's data requests couldn't be read from their database, because it can't be reached or has no table for them yet.

## Why Jadpo refuses it

`jadpo privacy requests` lists the data requests people have made, with the
legal deadline of each. Jadpo keeps them in a table of its own in the app's
database, which comes with the project's migrations. When the database can't
be reached, or has no such table, an empty list would say nothing is due, so
the command stops instead.

Jadpo won't apply the migrations for you here or try another address. The
database may be production's, and whether it is the right one only you know.

## What it looks like

A project that takes data requests:

```jadpo check project=e-privacy-unavailable file=config.jadpo hidden
config {
    database_url: PostgresUrl
    mail_sender: Email {
        default: "hello@notes.example"
    }
    site: Url {
        default: "https://notes.example"
    }
    smtp_host: Hostname {
        default: "localhost"
    }
}
```

```jadpo check project=e-privacy-unavailable file=authentication.jadpo hidden
authentication {
    browser_session: {
        cookie: "notes_session"
        origin: config.site
    }
}
```

```jadpo check project=e-privacy-unavailable file=domains/notes/entities/member.jadpo hidden
/// Someone with notes.
entity Member {
    id: Uuid
    email: Email {
        unique: true
        personal: PersonalData.contact
    }

    policy: {
        create: [Access.public]
        read: [id]
    }

    person: true
}
```

```jadpo check project=e-privacy-unavailable file=app.jadpo migrate=initial
use notes/entities { Member }

application {
    display_name: "Notes"
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
        mail: {
            driver: Driver.smtp
            host: config.smtp_host
        }
        vault: {
            driver: Driver.disk
            path: "/var/lib/notes/vault"
        }
    }
    email: {
        connection: mail
        from: config.mail_sender
    }
    privacy: {
        erasure_log: {
            store: vault
        }
        exports: {
            store: vault
        }
    }
}

principal {
    user: {
        subject: Member.email
    }
}
```

`jadpo privacy requests` against a new database, to which the project's
migrations haven't been applied:

```sh run project=e-privacy-unavailable database expect=privacy.unavailable
jadpo privacy requests
```

```text output
error: The database `main` has no table for data requests yet
  reason: `jadpo_privacy_requests` comes with the project's migrations, so either the database hasn't applied them, or it isn't this project's database.
  next: Run `jadpo migrate apply` if it is this project's database, then run the command again (human)
  rule: privacy.unavailable
  help: https://jadpo.dev/docs/errors/privacy.unavailable
```

`(human)` at the end of the `next:` line tells your coding agent to stop and
ask you which fix you want, because the choice is yours to make: it decides
which database the command reads, and whether its schema changes.

## How to fix it

**If it is this project's database,** apply the project's migrations, then run
the command again:

```sh
jadpo migrate apply
```

The database then has the table, and `jadpo privacy requests` lists what is in
it.

**If the database is another one,** name the right one with `--url`:

```sh
jadpo privacy requests --url postgres://localhost/notes
```

The list comes from that database instead.

## Related

- [Deadlines and events](../language/15-personal-data.md#deadlines-and-events)
- [`jadpo privacy`](../language/14-tooling.md#jadpo-privacy)
