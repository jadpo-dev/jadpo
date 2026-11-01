# `cli.routes_arguments`

A `jadpo routes` command is given an argument, which it doesn't take.

## Why Jadpo refuses it

`jadpo routes` lists every route the project serves, so it takes no name, and refuses a word rather than ignore it.

## What it looks like

A route is named:

```jadpo check project=e-routes-arguments file=app.jadpo hidden
application {
    jadpo: "0.2"
}
```

```sh run project=e-routes-arguments expect=cli.routes_arguments
jadpo routes greet
```

```text output
error: `jadpo routes` takes no arguments
  reason: It was given `greet`.
  next: Run `jadpo routes` (agent)
  rule: cli.routes_arguments
  help: https://jadpo.dev/docs/errors/cli.routes_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If you want the routes**, leave the argument out:

```sh
jadpo routes
```

It lists each route with its method, path and who may call it.

## Related

- [`jadpo routes`](../language/14-tooling.md#jadpo-routes)
