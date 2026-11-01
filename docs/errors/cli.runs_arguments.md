# `cli.runs_arguments`

A `jadpo runs` command names an action other than `unfinished`, or leaves out its `--build`.

## Why Jadpo refuses it

`jadpo runs unfinished --build <build id>` lists the unfinished runs a new build changes, before it is deployed. It compares them with one build, so it needs that build's id.

## What it looks like

The build is left out:

```jadpo check project=e-runs-arguments file=app.jadpo hidden
application {
    jadpo: "0.2"
}
```

```sh run project=e-runs-arguments expect=cli.runs_arguments
jadpo runs unfinished
```

```text output
error: `jadpo runs unfinished` needs `--build <build id>`
  reason: It compares the unfinished runs with the build about to be deployed, which the checkout makes.
  next: Add `--build <build id>` (agent)
  rule: cli.runs_arguments
  help: https://jadpo.dev/docs/errors/cli.runs_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If you are about to deploy a build**, give its id from `jadpo build`:

```sh
jadpo runs unfinished --build <build id>
```

It lists each unfinished run the build would change.

## Related

- [`jadpo runs unfinished`](../language/14-tooling.md#jadpo-runs-unfinished)
