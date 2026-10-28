# `publish.sample_missing`

Each recipe is compiled in the package's `sample/` app before it is
published, so a package that ships recipes ships a sample app with one
domain to copy them into:

```text
$ jadpo package check
error: `Couriers.v1`'s recipes have no sample app to compile in
  reason: Each recipe is compiled before it is published, copied into the one domain of the app in the package's `sample/` folder as `jadpo package recipe` copies it into an app's, so a recipe that no longer fits the package is caught there; this package has no `sample/app.jadpo`.
  next: Write a small app in `sample/`, its `app.jadpo` connecting to `Couriers.v1` and one domain under `sample/domains/` that the connection lists (agent)
  rule: publish.sample_missing
  help: docs/language/14-tooling.html#command-errors
```

A recipe that doesn't compile there is shown at its own path,
`recipes/<recipe>/<file>`, and the bar's row names it:

```text
Publishing bar
  compiles      passed                        no errors
  described     passed                        every public declaration
  examples      passed                        no functions to show
  tests         passed                        no tests
  recipes       failed                        track_parcels doesn't compile
  version       passed                        1.0.0, the first release
  changelog     passed                        1.0.0 says why
  licence       passed                        mit, from Jadpo 1.0
```
