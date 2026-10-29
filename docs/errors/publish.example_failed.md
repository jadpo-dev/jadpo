# `publish.example_failed`

An example runs as a test, so one that doesn't hold, or doesn't compile,
is shown at its line in the description:

```text
$ jadpo package check
error: Example of `uk.tidy` failed
 --> libraries/uk.jadpo:7:9
  |
7 |     /// assert postcode == "SW1A1AA"
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ this doesn't hold
  reason: Run as a test of the package, it expected "SW1A1AA" and got "SW1A 1AA". An example runs as a test, so a description always shows what the code does.
  next: Correct the example, or the code if the example says what it should do (agent)
  rule: publish.example_failed
  help: docs/language/14-tooling.html#command-errors
```
