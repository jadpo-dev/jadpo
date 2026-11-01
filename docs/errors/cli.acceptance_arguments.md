# `cli.acceptance_arguments`

A `jadpo acceptance` command names no action, one it doesn't have, or no purpose for `new`.

## Why Jadpo refuses it

An acceptance's file is named after the decision it records, so `acceptances/` reads as a list of decisions a person made. `jadpo acceptance new` can't make that name up, and has no other action to guess at.

## What it looks like

The purpose is left out:

```jadpo check project=e-acceptance-arguments file=app.jadpo hidden
application {
    jadpo: "0.1"
}
```

```sh run project=e-acceptance-arguments expect=cli.acceptance_arguments
jadpo acceptance new
```

```text output
error: `jadpo acceptance new` needs a purpose
  reason: An acceptance's file is named after the decision it records, so `acceptances/` reads as a list of decisions.
  next: Add one, such as `jadpo acceptance new open_the_route` (agent)
  rule: cli.acceptance_arguments
  help: https://jadpo.dev/docs/errors/cli.acceptance_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

An action it doesn't have is refused the same way:

```sh run project=e-acceptance-arguments expect=cli.acceptance_arguments
jadpo acceptance open_the_route
```

```text output
error: `open_the_route` isn't something `jadpo acceptance` does
  reason: `jadpo acceptance new <purpose>` writes a draft acceptance of every change waiting for a person.
  next: Run `jadpo acceptance new <purpose>` (agent)
  rule: cli.acceptance_arguments
  help: https://jadpo.dev/docs/errors/cli.acceptance_arguments
```

## How to fix it

**If you meant to draft an acceptance**, give it the decision it records, in `snake_case`:

```sh
jadpo acceptance new open_the_route
```

It writes `acceptances/open_the_route.jadpo`, listing every change waiting for a person, for them to read and give a reason.

## Related

- [`jadpo acceptance new`](../language/14-tooling.md#jadpo-acceptance-new)
- [Acceptances](../language/18-records-and-manifest.md#acceptances)
