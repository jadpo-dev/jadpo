# `publish.breaking_change`

A version keeps every promise of the release before it in the same major
version: here `uk.tidy` takes a `UkPostcode` instead of an
`EnteredPostcode`, which went:

```text
$ jadpo package check
error: `Postcodes.v2` changed in ways that break the apps that use it
  reason: Since 2.0.0, `function uk.tidy` changed and `type EnteredPostcode` was removed, which an app using 2.0.0 may rely on. A breaking change is a new major version: a new package beside this one.
  next: Put back what changed, or publish the change as `Postcodes.v3` in a folder of its own (human)
  rule: publish.breaking_change
  help: docs/language/14-tooling.html#command-errors

Publishing bar
  compiles      passed                        no errors
  described     passed                        every public declaration
  examples      passed                        1 example, every failure shown
  tests         passed                        no tests
  version       failed                        2 breaking changes since 2.0.0
  changelog     not checked yet               waits for an allowed version
  licence       passed                        mit, from Jadpo 1.0

package check failed: 1 failed, 5 passed, 1 not checked yet
```
