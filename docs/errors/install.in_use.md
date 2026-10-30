# `install.in_use`

A package the app's code still brings in stays installed:

```text
$ jadpo package remove postcodes
error: The app still uses `Postcodes.v2`
  reason: `domains/intake/actions/read_postcode.jadpo` brings it in with `use packages/postcodes`, so removing it would leave that file unable to compile.
  next: Take `use packages/postcodes` and what it brings in out of the app's code, then run `jadpo package remove postcodes` again (agent)
  rule: install.in_use
  help: https://jadpo.dev/docs/errors/install.in_use
```
