# `data.privacy_setting_invalid`

A setting under `privacy:` isn't one of its settings, or isn't valid.

## Why Jadpo refuses it

`privacy:` holds `erasure_log:`, `self_service:`, `path:`,
`generate_routes:`, `emails:` and `exports:`. Anything else is a typo or a
setting from somewhere else, and Jadpo would rather say so than leave a person
unable to make the request the setting was meant to allow. A value has to be
of the right shape too: `path:` and `self_service`'s pages are paths on your
site, and `generate_routes:` is `true` or `false`.

## What it looks like

```jadpo check project=e-privacy-setting file=app.jadpo hidden
application {}
```

```jadpo check project=e-privacy-setting file=domains/accounts/entities/user.jadpo hidden migrate=initial
entity User {
    id: Uuid
    decline data.personal_unread: "Read by code this example leaves out"
    email: Email {
        personal: PersonalData.contact
    }

    person: true
}
```

app.jadpo:

```jadpo check project=e-privacy-setting file=app.jadpo expect=data.privacy_setting_invalid
application {
    privacy: {
        export_link: "/data"
    }
}
```

```text output
error: `export_link` isn't a setting of `privacy:`
 --> app.jadpo:3:9
  |
3 |         export_link: "/data"
  |         ^^^^^^^^^^^ not a setting
  reason: `privacy:` says where the erasure log is kept and how a person makes a data request: `erasure_log:`, `self_service:`, `path:`, `generate_routes:`, `emails:` and `exports:`.
  next: Remove `export_link:` (agent)
  rule: data.privacy_setting_invalid
  help: https://jadpo.dev/docs/errors/data.privacy_setting_invalid

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make the
fix itself.

## How to fix it

**If the setting is a typo or from somewhere else,** remove it, or name one
`privacy:` holds. To put the routes somewhere else, say so with `path:`:

```jadpo check project=e-privacy-setting-fix file=app.jadpo
use accounts/entities { User }

application {
    privacy: {
        path: "/data"
    }
}

principal {
    user: {
        subject: User.email
    }
}
```

```jadpo check project=e-privacy-setting-fix file=domains/accounts/entities/user.jadpo hidden migrate=initial
entity User {
    id: Uuid
    email: Email {
        unique: true
        personal: PersonalData.contact
    }

    person: true
}
```

The routes now answer under `/data`, and every setting is one `privacy:`
holds.

## Related

- [Data requests](../language/15-personal-data.md#data-requests)
- [`data.privacy_person_required`](data.privacy_person_required.md)
