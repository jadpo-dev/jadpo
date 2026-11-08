# `type.suggestions_value_invalid`

A setting of `suggestions { }` has a value it can't take, such as a `project_token:` that isn't 32 hex digits.

## Why Jadpo refuses it

A project token is 32 random hex digits, tied to nothing, so Jadpo HQ can
tell one project from another without knowing which it is. A name that
says what the project is, such as a repository's, would give that away.
`share:` is `true` or `false`, and nothing else.

## What it looks like

main.jadpo:

```jadpo check project=e-suggestions-value-invalid expect=type.suggestions_value_invalid
suggestions {
    project_token: "acme-shop"
}
```

```text output
error: `project_token: "acme-shop"` isn't a value `project_token` takes
 --> main.jadpo:2:20
  |
2 |     project_token: "acme-shop"
  |                    ^^^^^^^^^^^ not a token
  reason: A project token is 32 random hex digits, tied to nothing, so Jadpo HQ can tell one project from another without knowing which it is.
  next: Remove `project_token:`; `jadpo suggest share` writes a new one (agent)
  rule: type.suggestions_value_invalid
  help: https://jadpo.dev/docs/errors/type.suggestions_value_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you want sharing to work**, remove the setting, and `jadpo suggest
share` writes a new token the next time it sends:

```jadpo fragment
suggestions {}
```

```jadpo check project=e-suggestions-value-invalid-fix-1 hidden
suggestions {}
```

The project has no token until `share` writes one.

**If you want sharing off**, say so with `share:` in place of the token:

```jadpo fragment
suggestions {
    share: false
}
```

```jadpo check project=e-suggestions-value-invalid-fix-2 hidden
suggestions {
    share: false
}
```

`jadpo suggest share` now sends nothing for this project.

## Related

- [The project token](../language/14-tooling.md#the-project-token)
- [Turning it off](../language/14-tooling.md#turning-it-off)
