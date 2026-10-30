# `publish.description_missing`

Every public type, failure, library and library function of a published
package has a `///` description:

```text
$ jadpo package check
error: Public type `OutwardCode` has no description
 --> types/outward_code.jadpo:1:13
  |
1 | public type OutwardCode = Text {
  |             ^^^^^^^^^^^ no `///` description
  reason: Every public type, failure, library and library function of a published package has a `///` description: it is what `jadpo package show` and the registry's page say of it, and what an agent reads before it opens the file.
  next: Add a `///` line above `public type OutwardCode` saying what it is for (agent)
  rule: publish.description_missing
  help: https://jadpo.dev/docs/errors/publish.description_missing

Publishing bar
  compiles      passed                        no errors
  described     failed                        1 has no description
  examples      passed                        1 example, every failure shown
  tests         passed                        no tests
  version       passed                        2.1.0, 1 added since 2.0.0
  changelog     passed                        2.1.0 says why
  licence       passed                        mit, from Jadpo 1.0

Wrote surface.jadpo_record

package check failed: 1 failed, 6 passed
```
