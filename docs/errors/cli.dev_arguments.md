# `cli.dev_arguments`

A `jadpo dev` command names an action it doesn't have, `token` without a user's id, or a `--url` the project's several stores can't share.

## Why Jadpo refuses it

`jadpo dev` serves the app, and `jadpo dev token <user-id>` signs a token for one user from its local issuer. A token for the wrong user would test the wrong access, so it signs none until it is told whose.

## What it looks like

The user's id is left out:

```jadpo check project=e-dev-arguments file=app.jadpo hidden
application {
    jadpo: "0.2"
}
```

```sh run project=e-dev-arguments expect=cli.dev_arguments
jadpo dev token
```

```text output
error: `jadpo dev token` needs a user's id
  reason: It signs a token for that user's subject, from the local issuer `jadpo dev` runs in the provider's place.
  next: Run `jadpo dev token <user-id>`, with the user's `id` (agent)
  rule: cli.dev_arguments
  help: https://jadpo.dev/docs/errors/cli.dev_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

A word `jadpo dev` doesn't take is refused too:

```sh run project=e-dev-arguments expect=cli.dev_arguments
jadpo dev serve
```

```text output
error: `jadpo dev` has no `serve`
  reason: `jadpo dev` serves the app; `jadpo dev token <user-id>` signs a token from its local issuer.
  next: Run `jadpo dev`, or `jadpo dev token <user-id>` (agent)
  rule: cli.dev_arguments
  help: https://jadpo.dev/docs/errors/cli.dev_arguments
```

## How to fix it

**If you want a token**, give the user's `id`:

```sh
jadpo dev token user_1
```

It prints a token for that user's subject, which `jadpo dev` accepts.

**If you want to serve the app**, run it with nothing after it:

```sh
jadpo dev
```

It serves the app and restarts it on each save.

## Related

- [`jadpo dev`](../language/14-tooling.md#jadpo-dev)
- [In development and tests](../language/08-identity.md#in-development-and-tests)
