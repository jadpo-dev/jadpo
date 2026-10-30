# `publish.example_missing`

Every function of a public library has an example in its description, and
each failure it can raise is shown by an example or a test:

```text
$ jadpo package check
error: Function `uk.tidy` has no example
 --> libraries/uk.jadpo:6:14
  |
6 |     function tidy(entered: EnteredPostcode) fails InvalidPostcode -> UkPostcode {
  |              ^^^^ no example in its description
  reason: Every function of a published library shows how it is called with an example in its description, a ```jadpo block run as a test, so what the registry's page and `jadpo package show` say of it always runs.
  next: Add a ```jadpo block to `uk.tidy`'s `///` description, holding statements as a test does: `assert uk.tidy(...) == ...` (agent)
  rule: publish.example_missing
  help: https://jadpo.dev/docs/errors/publish.example_missing
```

A failure no example or test asserts with `fails` is reported the same way,
as "`uk.tidy` can fail with `InvalidPostcode`, which no example or test
shows".
