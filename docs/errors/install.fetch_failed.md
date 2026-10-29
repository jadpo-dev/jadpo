# `install.fetch_failed`

A git repository that can't be read, or holds no release to install:

```text
$ jadpo package add github.com/acme/postcodes@v9.0.0
error: `github.com/acme/postcodes` has no tag `v9.0.0`
  reason: Its release tags are `v2.0.0` and `v2.1.0`.
  next: Give one of its release tags, or tag the release in the package's repository after `jadpo package check` passes (human)
  rule: install.fetch_failed
  help: docs/language/14-tooling.html#command-errors
```
