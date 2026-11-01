# `cli.declines_arguments`

A `jadpo declines` command names no action, one it doesn't have, or a rule or path that isn't the project's.

## Why Jadpo refuses it

`jadpo declines list` shows the findings declined with a reason, and `clear` removes them so each comes back. Clearing is done to the declines you name, so a rule that doesn't exist is refused rather than treated as none.

## What it looks like

The rule given to `--rule` is misspelt:

```jadpo check project=e-declines-arguments file=app.jadpo hidden
application {
    jadpo: "0.1"
}
```

```sh run project=e-declines-arguments expect=cli.declines_arguments
jadpo declines list --rule type.standard_exist
```

```text output
error: `type.standard_exist` isn't a rule
  reason: Jadpo has no rule `type.standard_exist`. Did you mean `type.standard_exists`?
  next: Use `--rule type.standard_exists` (agent)
  rule: cli.declines_arguments
  help: https://jadpo.dev/docs/errors/cli.declines_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

An action it doesn't have is refused with the ones it does:

```sh run project=e-declines-arguments expect=cli.declines_arguments
jadpo declines remove
```

```text output
error: `remove` isn't something `jadpo declines` does
  reason: `jadpo declines` can `list` or `clear`.
  next: Run `jadpo declines list` to see every decline (agent)
  rule: cli.declines_arguments
  help: https://jadpo.dev/docs/errors/cli.declines_arguments
```

## How to fix it

**If you meant the rule it suggests**, use it:

```sh
jadpo declines list --rule type.standard_exists
```

It lists that rule's declines with their reasons.

## Related

- [`jadpo declines list` and `clear`](../language/14-tooling.md#jadpo-declines-list-and-clear)
- [Listing and clearing declines](../language/01-project-layout-and-modules.md#listing-and-clearing-declines)
