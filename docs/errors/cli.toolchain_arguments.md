# `cli.toolchain_arguments`

A `jadpo toolchain` command names an action it doesn't have, or a release that isn't kept.

## Why Jadpo refuses it

`jadpo toolchain` lists the releases kept for projects, `remove` deletes one, and `pin` and `unpin` write the release a project runs with into `jadpo.lock`. Deleting is done to the version you name, never another.

## What it looks like

The release named isn't kept:

```sh run project=e-toolchain-arguments expect=cli.toolchain_arguments
jadpo toolchain remove 0.0.9
```

```text output
error: Jadpo 0.0.9 isn't kept
  reason: The cache holds only the releases projects needed, and this isn't one of them.
  next: Run `jadpo toolchain list` to see the releases kept (agent)
  rule: cli.toolchain_arguments
  help: https://jadpo.dev/docs/errors/cli.toolchain_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

An action it doesn't have is refused with the ones it does:

```sh run project=e-toolchain-arguments expect=cli.toolchain_arguments
jadpo toolchain clean
```

```text output
error: `clean` isn't a toolchain action
  reason: `jadpo toolchain` lists the kept releases, `remove` deletes one, and `pin` and `unpin` write the release a project runs with into `jadpo.lock`.
  next: Use `list`, `remove`, `pin` or `unpin` (agent)
  rule: cli.toolchain_arguments
  help: https://jadpo.dev/docs/errors/cli.toolchain_arguments
```

## How to fix it

**If you meant a kept release**, list them first, then remove one by its version:

```sh
jadpo toolchain list
jadpo toolchain remove <version>
```

A project that needs a removed release fetches it again when it runs.

## Related

- [Which jadpo runs](../language/14-tooling.md#which-jadpo-runs)
- [The releases kept](../language/14-tooling.md#the-releases-kept)
