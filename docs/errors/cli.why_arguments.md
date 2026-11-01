# `cli.why_arguments`

A `jadpo why` command names no schedule, one the project doesn't declare, or more than one.

## Why Jadpo refuses it

`jadpo why` says why one schedule didn't run, or how its last run went. It explains one, by its name, and a name it doesn't know could be a typo for another.

## What it looks like

A project with no schedules:

```jadpo check project=e-why-arguments file=app.jadpo hidden
application {
    jadpo: "0.1"
}
```

```sh run project=e-why-arguments expect=cli.why_arguments
jadpo why nightly_digest
```

```text output
error: `nightly_digest` isn't a schedule
  reason: The project declares no schedules.
  next: Declare one in a domain's `schedules/` folder (agent)
  rule: cli.why_arguments
  help: https://jadpo.dev/docs/errors/cli.why_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If you meant a schedule the project declares**, name it as its declaration does:

```sh
jadpo schedules
jadpo why <schedule>
```

`jadpo schedules` lists each schedule's name.

## Related

- [Commands for debugging, editors and help](../language/14-tooling.md#commands-for-debugging-editors-and-help)
- [Watching schedules](../language/12-jobs-and-events.md#watching-schedules)
