# `semantic.multiple_suggestions`

A project declares more than one `suggestions { }`.

## Why Jadpo refuses it

A project has one token and one answer to whether it shares its
suggestions, so both are in one block. Two could give two answers, and
sharing is a choice that shouldn't depend on which one Jadpo read first.

## What it looks like

main.jadpo:

```jadpo check project=e-suggestions-multiple expect=semantic.multiple_suggestions
suggestions {
    share: false
}

suggestions {
    share: true
}
```

```text output
error: `suggestions { }` is declared twice
 --> main.jadpo:5:1
  |
1 | suggestions {
  | ----------- first declared here
5 | suggestions {
  | ^^^^^^^^^^^ declared again
  reason: A project has one token and one answer to whether it shares, in one `suggestions { }` in `app.jadpo`.
  next: Move what this `suggestions` holds into the first, and remove it (agent)
  rule: semantic.multiple_suggestions
  help: https://jadpo.dev/docs/errors/semantic.multiple_suggestions

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you've decided whether to share**, keep one block with that answer:

```jadpo check project=e-suggestions-multiple-fix
suggestions {
    share: false
}
```

The project now has one answer about sharing.

## Related

- [Suggestions](../language/14-tooling.md#suggestions)
