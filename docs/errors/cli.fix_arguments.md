# `cli.fix_arguments`

A `jadpo fix` command names a declaration with nothing to fix, or a fix it doesn't have.

## Why Jadpo refuses it

`jadpo fix` makes the fixes `jadpo check` offers for a declaration that clashes with a standard type, or repeats one. It acts on a finding the check made, so a declaration with none is left as it is.

## What it looks like

A new project, whose declarations clash with nothing:

```sh run project=e-fix-arguments expect=cli.fix_arguments
jadpo new todo > /dev/null
cd todo
jadpo fix greet
```

```text output
error: `greet` has nothing to fix
  reason: `jadpo fix` fixes a declaration that `type.standard_clash` or `type.standard_exists` names, and the project has neither finding.
  next: Run `jadpo check` to see the project's errors (agent)
  rule: cli.fix_arguments
  help: https://jadpo.dev/docs/errors/cli.fix_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If you meant a finding `jadpo check` reports**, run the check and use the name and fix its error gives:

```sh
jadpo check
```

An error such as `type.standard_exists` names the declaration and the `jadpo fix` command that fixes it.

## Related

- [`jadpo fix`](../language/14-tooling.md#jadpo-fix)
- [The standard types](../language/19-standard-library.md#the-standard-types)
