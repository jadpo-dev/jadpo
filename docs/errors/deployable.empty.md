# `deployable.empty`

A deployable serves nothing and runs nothing.

## Why Jadpo refuses it

A deployable is a program you pay to build and run. One that lists no API
to serve and no background work to run would only answer its health checks,
so it is almost always a deployable half written. Jadpo can't tell what it
was meant to do.

## What it looks like

```jadpo check project=e-deployable-empty expect=deployable.empty file=app.jadpo
application {
    deployables: {
        kiosk: {
            target: Target.native
        }
    }
}
```

```text output
error: `kiosk` serves nothing and runs nothing
 --> app.jadpo:3:9
  |
3 |         kiosk: {
  |         ^^^^^ does nothing
  reason: A deployable serves APIs, `serves: [main]`, runs background work, `runs: [Trigger.schedules]`, or both; one that does neither would only answer its health checks.
  next: Give `kiosk` `serves:` or `runs:`, or remove it (agent)
  rule: deployable.empty
  help: https://jadpo.dev/docs/errors/deployable.empty

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If it should answer requests**, say which APIs it serves:

```jadpo check project=e-deployable-empty-serves file=app.jadpo
application {
    deployables: {
        kiosk: {
            serves: [main]
            target: Target.native
        }
    }
}
```

`kiosk` now answers the routes of `main`.

**If it should do background work**, say which:

```jadpo check project=e-deployable-empty-runs file=app.jadpo
application {
    deployables: {
        kiosk: {
            runs: [Trigger.schedules]
            target: Target.native
        }
    }
}
```

`kiosk` now runs the app's schedules, each entity's purge and each poll.

**If the app doesn't need it**, remove `deployables:`:

```jadpo check project=e-deployable-empty-removed file=app.jadpo
application {}
```

The app now builds into its one deployable, `app`, which serves every API
and runs everything.

## Related

- [Declared deployables](../language/14-tooling.md#declared-deployables)
