# `publish.changelog_missing`

`jadpo package check` writes the new version's list of changes into
`CHANGELOG.md`; the author says why beneath it:

```text
$ jadpo package check
error: CHANGELOG.md doesn't say why 2.1.0 is released
 --> CHANGELOG.md:3:1
  |
3 | ## 2.1.0
  | ^^^^^^^^ no why beneath the list
  reason: `jadpo package check` writes what changed from the surface; whoever releases it writes why, beneath it. A release reviewers can't tell the purpose of is one they can't judge.
  next: Write why 2.1.0 is released under `## 2.1.0` in CHANGELOG.md, after the list (agent)
  rule: publish.changelog_missing
  help: docs/language/14-tooling.html#command-errors
```
