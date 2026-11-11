# `publishing.invalid`

A connection in `publishing:` isn't one events are published through, or lists an event that can't leave the app.

## Why Jadpo refuses it

`publishing:` sends chosen events to other systems once the action that
emits them commits. Each entry is named after a connection with
`driver: Driver.webhook`, the one address its events are posted to,
signed. A database's connection posts nothing. The entry lists, in
`events:`, the events it publishes, and each becomes another system's
contract: a receiver writes code against its fields. So only an event a
domain offers as `public` leaves, or the built-in `AuditEntryRecorded`
while an entity keeps an audit trail, and each is named outside by its
`wire_name:`, or its domain and name, no two alike through one
connection. Jadpo never guesses which events you meant to share.

## What it looks like

```jadpo check project=e-publishing-invalid file=config.jadpo hidden
config {
    main_url: PostgresUrl
}
```

```jadpo check project=e-publishing-invalid file=domains/shipping/events/parcel_shipped.jadpo hidden
/// A parcel left the depot.
public type ParcelShipped = Event {
    tracking_code: Text {
        max_length: 20
    }
}
```

`app.jadpo`:

```jadpo check project=e-publishing-invalid file=app.jadpo expect=publishing.invalid
use shipping/events { ParcelShipped }

application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.main_url
        }
    }
    events: {
        store: main
    }
    publishing: {
        main: {
            events: [ParcelShipped]
        }
    }
}
```

```text output
error: `main` isn't a connection events are published through
 --> app.jadpo:14:9
   |
14 |         main: {
   |         ^^^^ not a webhook's connection
  reason: `publishing:` names each connection with `driver: Driver.webhook` that events leave the app through, and lists, in `events:`, the events it publishes: each a `public` event of one of the app's domains, or `AuditEntryRecorded` while an entity keeps an audit trail. Every event listed becomes an outside system's contract, so only events a domain offers as `public` leave.
  next: Name a connection with `driver: Driver.webhook` (agent)
  rule: publishing.invalid
  help: https://jadpo.dev/docs/errors/publishing.invalid

check failed: 1 error, 3 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An event that isn't `public` is the same error:

```jadpo check project=e-publishing-invalid-private file=config.jadpo hidden
config {
    main_url: PostgresUrl
    partner_hook_secret: Text {
        secret: true
    }
    partner_hook_url: Url
}
```

```jadpo check project=e-publishing-invalid-private file=domains/shipping/events/parcel_shipped.jadpo hidden
/// A parcel left the depot.
type ParcelShipped = Event {
    tracking_code: Text {
        max_length: 20
    }
}
```

```jadpo check project=e-publishing-invalid-private file=app.jadpo expect=publishing.invalid
use shipping/events { ParcelShipped }

application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.main_url
        }
        partner_hooks: {
            driver: Driver.webhook
            url: config.partner_hook_url
            secret: config.partner_hook_secret
        }
    }
    events: {
        store: main
    }
    publishing: {
        partner_hooks: {
            events: [ParcelShipped]
        }
    }
}
```

```text output
error: `ParcelShipped` isn't public
 --> app.jadpo:20:22
   |
20 |             events: [ParcelShipped]
   |                      ^^^^^^^^^^^^^ not public
  reason: `publishing:` names each connection with `driver: Driver.webhook` that events leave the app through, and lists, in `events:`, the events it publishes: each a `public` event of one of the app's domains, or `AuditEntryRecorded` while an entity keeps an audit trail. Every event listed becomes an outside system's contract, so only events a domain offers as `public` leave.
  next: Declare it `public type ParcelShipped = Event { ... }`, offering it to whatever receives it (agent)
  rule: publishing.invalid
  help: https://jadpo.dev/docs/errors/publishing.invalid

check failed: 1 error, 3 files
```

## How to fix it

**If the event should reach another system**, give its address and the
secret each event is signed with to a connection with `driver:
Driver.webhook`, name that connection in `publishing:`, and make the
event `public`.
In `app.jadpo`:

```jadpo fragment
    connections: {
        ...
        partner_hooks: {
            driver: Driver.webhook
            url: config.partner_hook_url
            secret: config.partner_hook_secret
        }
    }
    publishing: {
        partner_hooks: {
            events: [ParcelShipped]
        }
    }
```

```jadpo check project=e-publishing-invalid-fix file=app.jadpo hidden
use shipping/events { ParcelShipped }

application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.main_url
        }
        partner_hooks: {
            driver: Driver.webhook
            url: config.partner_hook_url
            secret: config.partner_hook_secret
        }
    }
    events: {
        store: main
    }
    publishing: {
        partner_hooks: {
            events: [ParcelShipped]
        }
    }
}
```

```jadpo check project=e-publishing-invalid-fix file=config.jadpo hidden
config {
    main_url: PostgresUrl
    partner_hook_secret: Text {
        secret: true
    }
    partner_hook_url: Url
}
```

And in `domains/shipping/events/parcel_shipped.jadpo`:

```jadpo check project=e-publishing-invalid-fix file=domains/shipping/events/parcel_shipped.jadpo
/// A parcel left the depot.
public type ParcelShipped = Event {
    tracking_code: Text {
        max_length: 20
    }
}
```

Each `ParcelShipped` is now posted to the address in
`config.partner_hook_url` once the action that emits it commits, signed
with `config.partner_hook_secret`, and tried again while the partner's
server is down.

## Related

- [Publishing events outward](../language/12-jobs-and-events.md#publishing-events-outward)
- [Where events travel](../language/12-jobs-and-events.md#where-events-travel)
- [Outgoing webhooks](../language/16-built-in-services.md#outgoing-webhooks)
