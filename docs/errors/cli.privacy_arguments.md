# `cli.privacy_arguments`

A `jadpo privacy` command names an action it doesn't have, or a state or page `requests` doesn't list.

## Why Jadpo refuses it

`jadpo privacy requests` lists the data requests people have made, each with its legal deadline. A state or a page it doesn't know would show an empty list, and someone reading that would think there was nothing to do, so it is refused before the database is touched.

## What it looks like

A state that isn't one:

```jadpo check project=e-privacy-arguments file=app.jadpo hidden
application {}
```

```sh run project=e-privacy-arguments expect=cli.privacy_arguments
jadpo privacy requests --state closed
```

```text output
error: `closed` isn't a state
  reason: The states listed are `received`, `erasing`, `stalled`, `ready`, `downloaded`, `expired`, `completed` and `cancelled`.
  next: Use `--state received`, or leave it out (agent)
  rule: cli.privacy_arguments
  help: https://jadpo.dev/docs/errors/cli.privacy_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If you meant a state the list has,** name it, or leave `--state` out to list
what is still to finish:

```sh run project=e-privacy-arguments expect=cli.privacy_arguments
jadpo privacy requests
```

```text output
error: This application takes no data requests
  reason: `privacy:` in `application { }` turns them on, once it says more than `erasure_log:`.
  next: Add `privacy:` to `application { }` in `app.jadpo` (agent)
  rule: cli.privacy_arguments
  help: https://jadpo.dev/docs/errors/cli.privacy_arguments
```

An application that takes data requests lists them instead.

## Related

- [`jadpo privacy`](../language/14-tooling.md#jadpo-privacy)
- [`privacy.unavailable`](privacy.unavailable.md)
