# `cli.calls_arguments`

A `jadpo calls` command names an action it doesn't have, a call the log doesn't hold, or an answer without its operation.

## Why Jadpo refuses it

`jadpo calls` reads the log of provider calls `jadpo dev` made, shows one by its id, or scripts the next answer of one operation it fakes. Each needs to know which call or operation, and it never picks one for you.

## What it looks like

Nothing has run `jadpo dev` yet, so the log is empty, and the command asks for a call:

```jadpo check project=e-calls-arguments file=app.jadpo hidden
application {
    jadpo: "0.1"
}
```

```sh run project=e-calls-arguments expect=cli.calls_arguments
jadpo calls show call_31
```

```text output
error: There is no call `call_31`
  reason: The log keeps the last 500 calls `jadpo dev` made, and holds none yet.
  next: Run `jadpo calls` to see their ids (agent)
  rule: cli.calls_arguments
  help: https://jadpo.dev/docs/errors/cli.calls_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

An action it doesn't have is refused with the ones it does:

```sh run project=e-calls-arguments expect=cli.calls_arguments
jadpo calls replay
```

```text output
error: `replay` isn't something `jadpo calls` does
  reason: `jadpo calls` lists calls, shows one or scripts an answer: `list`, `show` or `script`.
  next: Run `jadpo calls` to list the calls (agent)
  rule: cli.calls_arguments
  help: https://jadpo.dev/docs/errors/cli.calls_arguments
```

## How to fix it

**If you want to see the calls**, list them first, then show one by the id the list gives:

```sh
jadpo calls
jadpo calls show call_31
```

The list is empty until `jadpo dev` has made a call.

## Related

- [`jadpo calls`](../language/14-tooling.md#jadpo-calls)
- [`jadpo dev`](../language/14-tooling.md#jadpo-dev)
