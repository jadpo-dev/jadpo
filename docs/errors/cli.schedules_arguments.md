# `cli.schedules_arguments`

A `jadpo schedules` command names an action it doesn't have, or a schedule the project doesn't declare.

## Why Jadpo refuses it

`jadpo schedules` lists the schedules, and `run`, `pause`, `resume` and `history` act on one by its name. Running or pausing the wrong one would do real work, so a name it doesn't know is refused.

## What it looks like

A project with no schedules, asked to run one:

```jadpo check project=e-schedules-arguments file=app.jadpo hidden
application {
    jadpo: "0.2"
}
```

```sh run project=e-schedules-arguments expect=cli.schedules_arguments
jadpo schedules run nightly_digest
```

```text output
error: `nightly_digest` isn't a schedule
  reason: The project declares no schedules.
  next: Declare one in a domain's `schedules/` folder (agent)
  rule: cli.schedules_arguments
  help: https://jadpo.dev/docs/errors/cli.schedules_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

An action it doesn't have is refused with the ones it does:

```sh run project=e-schedules-arguments expect=cli.schedules_arguments
jadpo schedules stop nightly_digest
```

```text output
error: `stop` isn't something `jadpo schedules` does
  reason: `jadpo schedules` lists the schedules, and `run`, `pause`, `resume` and `history` act on one.
  next: Run `jadpo schedules`, or `jadpo schedules history <name>` (agent)
  rule: cli.schedules_arguments
  help: https://jadpo.dev/docs/errors/cli.schedules_arguments
```

## How to fix it

**If the schedule isn't declared yet**, declare it in a domain's `schedules/` folder, then run it by name:

```sh
jadpo schedules run nightly_digest
```

It runs the schedule once, now.

## Related

- [`jadpo schedules`](../language/14-tooling.md#jadpo-schedules)
- [Running a schedule by hand](../language/12-jobs-and-events.md#running-a-schedule-by-hand)
