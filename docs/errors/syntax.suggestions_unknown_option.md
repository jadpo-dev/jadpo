# `syntax.suggestions_unknown_option`

A `suggestions { }` block has a setting other than `project_token:` or `share:`.

## Why Jadpo refuses it

`suggestions { }` takes `project_token:` and `share:`, nothing else. A
setting Jadpo doesn't know would be ignored, and a misspelt `share:` left in
place would look as if it had turned sharing off when it hadn't.

## What it looks like

main.jadpo:

```jadpo check project=e-suggestions-unknown-option expect=syntax.suggestions_unknown_option
suggestions {
    shared: false
}
```

```text output
error: `suggestions` has no option `shared:`
 --> main.jadpo:2:5
  |
2 |     shared: false
  |     ^^^^^^ no such option
  reason: `suggestions` takes `project_token:` and `share:`. Did you mean `share`?
  next: Use `share:` instead of `shared:` (agent)
  rule: syntax.suggestions_unknown_option
  help: https://jadpo.dev/docs/errors/syntax.suggestions_unknown_option

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you misspelt `share:`**, use the name the error gives:

```jadpo check project=e-suggestions-unknown-option-fix
suggestions {
    share: false
}
```

Sharing is now off for this project.

## Related

- [Turning it off](../language/14-tooling.md#turning-it-off)
- [The project token](../language/14-tooling.md#the-project-token)
