# `cli.package_arguments`

A `jadpo package` command names no action, one it doesn't have, or a package the project doesn't hold.

## Why Jadpo refuses it

`jadpo package` adds, updates, shows and checks packages, and each action acts on one package by its name and version. Installing or showing the wrong one would mislead, so a name that isn't there is refused.

## What it looks like

A project with no packages, asked to show one:

```jadpo check project=e-package-arguments file=app.jadpo hidden
application {}
```

```sh run project=e-package-arguments expect=cli.package_arguments
jadpo package show Stripe.v2024_06_20
```

```text output
error: `Stripe.v2024_06_20` isn't a package in this project
  reason: The project holds no package: none in `packages/` or `shared/providers/`.
  next: Install one with `jadpo package add`, or write the app's own in `shared/providers/<name>/<version>/` (agent)
  rule: cli.package_arguments
  help: https://jadpo.dev/docs/errors/cli.package_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

An action it doesn't have is refused with the ones it does:

```sh run project=e-package-arguments expect=cli.package_arguments
jadpo package install Stripe.v2024_06_20
```

```text output
error: `install` isn't something `jadpo package` does
  reason: `jadpo package` can `show`, `check`, `verify`, `import`, `record`, `conform`, `add`, `update`, `remove`, `link`, `unlink` or `recipe`.
  next: Run `jadpo package show <name>` or `jadpo package check` (agent)
  rule: cli.package_arguments
  help: https://jadpo.dev/docs/errors/cli.package_arguments
```

## How to fix it

**If the package isn't installed yet**, add it from its git repository, then show it:

```sh
jadpo package add <git-url>@<tag>
jadpo package show Stripe.v2024_06_20
```

It prints the package's surface: what it offers the app.

## Related

- [Commands for packages](../language/14-tooling.md#commands-for-packages)
- [`jadpo package show`](../language/14-tooling.md#jadpo-package-show)
