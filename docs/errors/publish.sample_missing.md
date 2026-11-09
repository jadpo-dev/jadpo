# `publish.sample_missing`

A package with recipes has no sample app with one domain to compile them in.

## Why Jadpo refuses it

A recipe is code a package hands to an app, so nobody finds out it no
longer fits the package until someone copies it in. `jadpo package check`
compiles each recipe first, copied into the one domain of the app in the
package's `sample/` folder, just as `jadpo package recipe` copies it into
yours. With no such app there is nowhere to compile it, and the release
would ship recipes nobody has checked.

## What it looks like

A provider package started with `jadpo package new`, its `sample/` app
deleted:

```sh run project=e-sample-missing expect=publish.sample_missing
jadpo package new Couriers --provider > /dev/null
cd couriers
rm -r sample
jadpo package check
```

```text output
error: `Couriers.v1`'s recipes have no sample app to compile in
  reason: Each recipe is compiled before it is published, copied into the one domain of the app in the package's `sample/` folder as `jadpo package recipe` copies it into an app's, so a recipe that no longer fits the package is caught there; this package has no `sample/app.jadpo`.
  next: Write a small app in `sample/`, its `app.jadpo` connecting to `Couriers.v1` and one domain under `sample/domains/` that the connection lists (agent)
  rule: publish.sample_missing
  help: https://jadpo.dev/docs/errors/publish.sample_missing

Facts
  kind: provider
  hosts: https://api.couriers.example
         https://sandbox.couriers.example
  changes: none
  erasure: not declared
  storable: AccountId
  failures: none
  contract: 0 of 1 operation confirmed

Publishing bar
  compiles      passed                        no errors
  described     passed                        every public declaration
  reviewed      passed                        every public type
  examples      passed                        no functions to show
  tests         passed                        no tests
  recipes       failed                        no sample/ app
  version       passed                        1.0.0, the first release
  changelog     passed                        1.0.0 says why
  publisher     left out                      publishing needs it
  licence       passed                        proprietary, registry refuses it

package check failed: 1 failed, 8 passed
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A sample app with no domain, or more than one, is refused the same way,
since a recipe is copied into its one domain. A recipe that doesn't compile
there is shown at its own path, `recipes/<recipe>/<file>`, and the bar's
`recipes` row names it.

## How to fix it

**If the package keeps its recipes**, write a small app in `sample/`,
connecting to the package from one domain:

```sh run project=e-sample-missing-write
jadpo package new Couriers --provider > /dev/null
cd couriers
rm -r sample
mkdir -p sample/domains/sample_app/types sample/domains/sample_app/actions
cat > sample/app.jadpo <<'DOC'
application {
    connections: {
        service: {
            package: Couriers.v1
            domains: [sample_app]
            credential: config.service_key
            environment: config.service_environment
        }
    }
}
DOC
cat > sample/config.jadpo <<'DOC'
config {
    service_environment: ProviderEnvironment
    service_key: Text {
        secret: true
    }
}
DOC
cat > sample/domains/sample_app/types/sample_note.jadpo <<'DOC'
/// A note about something.
type SampleNote = Text {
    max_length: 200
}
DOC
cat > sample/domains/sample_app/actions/noted_sample.jadpo <<'DOC'
use sample_app/types { SampleNote }

/// Gives a note back as it was written.
action noted_sample(note: SampleNote) -> SampleNote {
    return note
}
DOC
jadpo package check
```

```text output
Facts
  kind: provider
  hosts: https://api.couriers.example
         https://sandbox.couriers.example
  changes: none
  erasure: not declared
  storable: AccountId
  failures: none
  contract: 0 of 1 operation confirmed

Publishing bar
  compiles      passed                        no errors
  described     passed                        every public declaration
  reviewed      passed                        every public type
  examples      passed                        no functions to show
  tests         passed                        no tests
  recipes       passed                        1 recipe compiles in sample/
  version       passed                        1.0.0, the first release
  changelog     passed                        1.0.0 says why
  publisher     left out                      publishing needs it
  licence       passed                        proprietary, registry refuses it

package check passed: Couriers.v1 1.0.0, 9 passed
```

Each recipe now compiles in that app before a release, and nothing runs it.

**If the package doesn't need its recipes**, remove them:

```sh run project=e-sample-missing-remove
jadpo package new Couriers --provider > /dev/null
cd couriers
rm -r sample
rm -r recipes
jadpo package check
```

```text output
Facts
  kind: provider
  hosts: https://api.couriers.example
         https://sandbox.couriers.example
  changes: none
  erasure: not declared
  storable: AccountId
  failures: none
  contract: 0 of 1 operation confirmed

Publishing bar
  compiles      passed                        no errors
  described     passed                        every public declaration
  reviewed      passed                        every public type
  examples      passed                        no functions to show
  tests         passed                        no tests
  version       passed                        1.0.0, the first release
  changelog     passed                        1.0.0 says why
  publisher     left out                      publishing needs it
  licence       passed                        proprietary, registry refuses it

Wrote surface.jadpo_record

package check passed: Couriers.v1 1.0.0, 8 passed
```

With no recipes to compile, the package needs no sample app.

## Related

- [Package subscribers and recipes in `sample/`](../language/11-integrations.md#package-subscribers-and-recipes-in-sample)
- [Recipes](../language/11-integrations.md#recipes)
- [What publishing requires](../language/11-integrations.md#what-publishing-requires)
