# `syntax.suggestions_duplicate_option`

A `suggestions { }` block gives the same setting twice.

## Why Jadpo refuses it

`suggestions { }` says each of `project_token:` and `share:` once. Two
values for one setting would leave Jadpo choosing between them, and `share:`
decides whether anything leaves your project, so it won't.

## What it looks like

main.jadpo:

```jadpo check project=e-suggestions-duplicate-option expect=syntax.suggestions_duplicate_option
suggestions {
    share: false
    share: true
}
```

```text output
error: `share:` is given twice
 --> main.jadpo:3:5
  |
2 |     share: false
  |     ----- first given here
3 |     share: true
  |     ^^^^^ given again
  reason: `suggestions` says each of `project_token:` and `share:` once.
  next: Keep one `share:` and remove the other (agent)
  rule: syntax.suggestions_duplicate_option
  help: https://jadpo.dev/docs/errors/syntax.suggestions_duplicate_option

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you gave the setting twice by mistake**, keep the one you mean and
remove the other:

```jadpo check project=e-suggestions-duplicate-option-fix
suggestions {
    share: false
}
```

Sharing is now off for this project.

## Related

- [Turning it off](../language/14-tooling.md#turning-it-off)
- [Suggestions](../language/14-tooling.md#suggestions)
