# `cli.suggest_arguments`

A `jadpo suggest` command names an action it doesn't have, or a suggestion the last scan didn't find.

## Why Jadpo refuses it

`jadpo suggest` scans the project, and `ignore`, `found` and `share` act on what the last scan found, by its number. A number from an older scan could mean another finding, so one it doesn't have is refused.

## What it looks like

Nothing has been scanned yet:

```jadpo check project=e-suggest-arguments file=app.jadpo hidden
application {}
```

```sh run project=e-suggest-arguments expect=cli.suggest_arguments
jadpo suggest ignore 1 "Kept apart on purpose"
```

```text output
error: `1` isn't a suggestion of the last scan
  reason: The last scan found nothing to ignore, or `jadpo suggest` hasn't run here yet.
  next: Run `jadpo suggest` to see the numbers (agent)
  rule: cli.suggest_arguments
  help: https://jadpo.dev/docs/errors/cli.suggest_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

An action it doesn't have is refused with the ones it does:

```sh run project=e-suggest-arguments expect=cli.suggest_arguments
jadpo suggest hide 1
```

```text output
error: `hide` isn't something `jadpo suggest` does
  reason: `jadpo suggest` scans, or can `ignore`, `found` or `share`.
  next: Run `jadpo suggest` to scan the project (agent)
  rule: cli.suggest_arguments
  help: https://jadpo.dev/docs/errors/cli.suggest_arguments
```

## How to fix it

**If you meant a finding**, scan first, then use the number it gives:

```sh
jadpo suggest
jadpo suggest ignore 1 "Kept apart on purpose"
```

The finding stays quiet, with its reason kept beside it.

## Related

- [`jadpo suggest`](../language/14-tooling.md#jadpo-suggest)
- [Ignoring a suggestion](../language/14-tooling.md#ignoring-a-suggestion)
