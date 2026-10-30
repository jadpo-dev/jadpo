# `install.surface_mismatch`

A package is installed only when its committed surface is what its code
offers. Here a type was added after the surface was written:

```text
$ jadpo package add ../postcodes
error: `Postcodes.v2`'s surface.jadpo_record doesn't match its code
  reason: Its code has `type InwardCode`, which its committed surface doesn't. A package is installed only when its surface, what reviewers read, is what its code offers.
  next: Run `jadpo package check` in the package's repository, commit its surface.jadpo_record, and release it again (human)
  rule: install.surface_mismatch
  help: https://jadpo.dev/docs/errors/install.surface_mismatch
```
