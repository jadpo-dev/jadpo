# `acceptance.base_missing`

`jadpo check` compares a change with the default branch at its merge base,
read from git. A clone made without that commit, such as CI's depth-1
checkout, can't say what needs a person's acceptance, so it fails rather
than pass unchecked:

```text
error: The base to compare with, the default branch, isn't in this clone
  reason: `jadpo check` compares the change with the default branch at its merge base, and this clone was made without that commit, so what needs a person's acceptance can't be worked out.
  next: Fetch the history with `git fetch --unshallow`; in CI, check out with `fetch-depth: 0` (agent)
  rule: acceptance.base_missing
  help: https://jadpo.dev/docs/errors/acceptance.base_missing
```

[Comparing two builds](../language/18-records-and-manifest.md#comparing-two-builds)
says how the base is found.
