# `data.privacy_person_required`

A data request has no signed-in person to act for, or no address to find them by.

## Why Jadpo refuses it

A data request exports or erases one person's own records, chosen by who is
signed in or by their email address. If `principal { }` declares no `user`, or
that `user` isn't an entity marked `person: true`, Jadpo can't say whose
records a request is about. A request by email also needs an `Email` field to
find the person by: the one `magic_link:` names, or else the `user`'s own
`subject:`.

Jadpo won't choose for you, since it decides who can ask for what.

## What it looks like

```jadpo check project=e-privacy-person file=app.jadpo hidden
application {}
```

```jadpo check project=e-privacy-person file=domains/accounts/entities/user.jadpo hidden migrate=initial
entity User {
    id: Uuid
    decline data.personal_unread: "Read by code this example leaves out"
    email: Email {
        unique: true
        personal: PersonalData.contact
    }

    person: true
}
```

app.jadpo:

```jadpo check project=e-privacy-person file=app.jadpo expect=data.privacy_person_required
application {
    privacy: {
        self_service: {}
    }
}
```

```text output
error: `privacy:` needs a `user` who is a person
 --> app.jadpo:2:5
  |
2 |     privacy: {
  |     ^^^^^^^ no user to act for
  reason: A data request acts on one person's own records, by who is signed in or by their email address, so `principal { }` must declare who a `user` is, on an entity marked `person: true`.
  next: Declare `principal { user: { subject: <entity>.<field> } }` on the entity marked `person: true` (human)
  rule: data.privacy_person_required
  help: https://jadpo.dev/docs/errors/data.privacy_person_required

check failed: 1 error, 2 files
```

`(human)` at the end of the `next:` line tells your coding agent to stop and
ask you which fix you want, because the choice is yours to make: it decides
who a request acts for.

## How to fix it

**If your app has people who sign in,** declare who a `user` is, on the entity
that is a person, so a request can find them by their address:

```jadpo check project=e-privacy-person-fix file=config.jadpo hidden
config {
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

```jadpo check project=e-privacy-person-fix file=authentication.jadpo hidden
authentication {
    browser_session: {
        cookie: "notes_session"
        origin: config.site
    }
}
```

```jadpo check project=e-privacy-person-fix file=app.jadpo
use accounts/entities { User }

application {
    display_name: "Notes"
    connections: {
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
        path: "/privacy"
    }
}

principal {
    user: {
        subject: User.email
    }
}
```

```jadpo check project=e-privacy-person-fix file=domains/accounts/entities/user.jadpo hidden migrate=initial
entity User {
    id: Uuid
    email: Email {
        unique: true
        personal: PersonalData.contact
    }

    person: true
}
```

A data request also needs somewhere to keep exports, an erasure log, an origin
and a way to tell the person; each says so when it is missing.

## Related

- [Data requests](../language/15-personal-data.md#data-requests)
- [`email.connection_required`](email.connection_required.md)
