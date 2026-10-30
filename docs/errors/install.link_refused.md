# `install.link_refused`

A link compiles the app against a checkout on one machine, so it never
ships:

```text
$ CI=true jadpo check
error: `Postcodes.v2` is linked to `../postcodes`
 --> jadpo.lock:4:1
  |
4 | package Postcodes.v2 link ../postcodes
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ a live checkout
  reason: A link compiles the app against a checkout on one machine, so CI (the `CI` variable is set) and production builds refuse it, and a link never ships.
  next: Run `jadpo package unlink postcodes`, then commit `jadpo.lock` (agent)
  rule: install.link_refused
  help: https://jadpo.dev/docs/errors/install.link_refused

check failed: 1 error, 7 files
```
