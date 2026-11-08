# `publish.description_missing`

A public type, failure, library or library function of a package has no `///` description.

## Why Jadpo refuses it

A package's public declarations are what apps build on. Their `///`
descriptions are what `jadpo package show` and the registry's page say
of each one, and what an agent reads before it opens the file. A public
declaration with none leaves whoever installs the package to guess what
it is for, so `jadpo package check` fails its `described` row until each
has one.

## What it looks like

A package started with `jadpo package new`, with a public type added
and no description above it:

```sh run project=e-description-missing expect=publish.description_missing
jadpo package new Postcodes --library > /dev/null
cd postcodes
cat > types/outward_code.jadpo <<'DOC'
public type OutwardCode = Text {
    max_length: 4
}
DOC
jadpo package check
```

```text output
error: Public type `OutwardCode` has no description
 --> types/outward_code.jadpo:1:13
  |
1 | public type OutwardCode = Text {
  |             ^^^^^^^^^^^ no `///` description
  reason: Every public type, failure, library and library function of a published package has a `///` description: it is what `jadpo package show` and the registry's page say of it, and what an agent reads before it opens the file.
  next: Add a `///` line above `public type OutwardCode` saying what it is for (agent)
  rule: publish.description_missing
  help: https://jadpo.dev/docs/errors/publish.description_missing

Facts
  kind: library, pure
  storable: EnteredText, OutwardCode, TidyText
  failures: starter.tidied: NothingEntered

Publishing bar
  compiles      passed                        no errors
  described     failed                        1 has no description
  examples      passed                        1 example, every failure shown
  tests         passed                        1 test
  recipes       passed                        1 recipe compiles in sample/
  version       passed                        1.0.0, the first release
  changelog     passed                        1.0.0 says why
  publisher     left out                      publishing needs it
  licence       passed                        proprietary, registry refuses it

Wrote surface.jadpo_record

package check failed: 1 failed, 7 passed
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A public library, a public failure and a function of a public library
are held to the same rule, each named in its own error.

## How to fix it

**If the type is meant to be public**, say what it is for in a `///`
line above it:

```jadpo fragment
/// The first half of a UK postcode, before the space, such as `SW1A`.
public type OutwardCode = Text {
    ...
}
```

```jadpo check project=e-description-missing-fixed file=shared/providers/postcodes/v1/types/outward_code.jadpo hidden
/// The first half of a UK postcode, before the space, such as `SW1A`.
public type OutwardCode = Text {
    max_length: 4
}
```

```jadpo check project=e-description-missing-fixed file=shared/providers/postcodes/v1/package.jadpo hidden
/// UK postcodes, read and tidied.
package Postcodes.v1 {
    licence: Licence.mit
    jadpo: "X.Y"
}
```

```jadpo check project=e-description-missing-fixed file=app.jadpo hidden
application {}
```

The `described` row passes, and `jadpo package show` and the registry
give the line as what `OutwardCode` is.

## Related

- [What publishing requires](../language/11-integrations.md#what-publishing-requires)
- [A package's surface](../language/11-integrations.md#a-packages-surface)
- [`jadpo package check`](../language/14-tooling.md#jadpo-package-check)
