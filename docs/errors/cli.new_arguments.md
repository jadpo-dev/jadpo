# `cli.new_arguments`

A `jadpo new` command names no project, or more than one.

## Why Jadpo refuses it

`jadpo new` makes a folder of the name it is given, holding the project. It makes one, so it needs one name.

## What it looks like

No name is given:

```sh run project=e-new-arguments expect=cli.new_arguments
jadpo new
```

```text output
error: `jadpo new` needs a name for the project
  reason: It makes a folder of that name holding the project.
  next: Run `jadpo new <name>`, for example `jadpo new my-app` (agent)
  rule: cli.new_arguments
  help: https://jadpo.dev/docs/errors/cli.new_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If you want a new project**, name its folder:

```sh
jadpo new todo
```

It writes the project in `todo/`, passing `jadpo check` and `jadpo test` as written.

## Related

- [`jadpo new`](../language/14-tooling.md#jadpo-new)
- [The `jadpo` command](../language/14-tooling.md#the-jadpo-command)
