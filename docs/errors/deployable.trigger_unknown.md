# `deployable.trigger_unknown`

A deployable's `runs:` lists something that isn't a trigger.

## Why Jadpo refuses it

`runs:` decides which background work a program does, and work no program
runs simply waits. There are two kinds: `Trigger.schedules`, the schedules,
purges and polls, and `Trigger.subscribers`, the deliveries of events and
emails. Jadpo can't tell which you meant by another name.

## What it looks like

```jadpo check project=e-deployable-trigger-unknown expect=deployable.trigger_unknown file=app.jadpo
application {
    deployables: {
        kiosk: {
            serves: [main]
            runs: [Trigger.jobs]
            target: Target.native
        }
    }
}
```

```text output
error: `Trigger.jobs` isn't background work a deployable runs
 --> app.jadpo:5:20
  |
5 |             runs: [Trigger.jobs]
  |                    ^^^^^^^^^^^^ not a `Trigger`
  reason: `runs:` lists `Trigger.schedules`, the schedules, purges and polls, and `Trigger.subscribers`, the deliveries of events and emails.
  next: Use `Trigger.schedules` or `Trigger.subscribers` (agent)
  rule: deployable.trigger_unknown
  help: https://jadpo.dev/docs/errors/deployable.trigger_unknown

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If it should run timed work**, list `Trigger.schedules`:

```jadpo check project=e-deployable-trigger-schedules file=app.jadpo
application {
    deployables: {
        kiosk: {
            serves: [main]
            runs: [Trigger.schedules]
            target: Target.native
        }
    }
}
```

`kiosk` now runs the app's schedules, each entity's purge and each poll.

**If it should deliver events and emails**, list `Trigger.subscribers`:

```jadpo check project=e-deployable-trigger-subscribers file=app.jadpo
application {
    deployables: {
        kiosk: {
            serves: [main]
            runs: [Trigger.subscribers]
            target: Target.native
        }
    }
}
```

`kiosk` now delivers each event and email after its action commits.

## Related

- [`runs:`, the background work it runs](../language/14-tooling.md#runs-the-background-work-it-runs)
- [Declared deployables](../language/14-tooling.md#declared-deployables)
