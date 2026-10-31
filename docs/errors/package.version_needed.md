# `package.version_needed`

Your code names something through a package that the app holds in two versions, without saying which version it means.

## Why Jadpo refuses it

While callers move from one version of a package to the next, the app
can hold both side by side, such as `Postcodes.v2` and `Postcodes.v3`.
Each is its own package, so `Postcodes.v2.UkPostcode` and
`Postcodes.v3.UkPostcode` are different types. A name with no version,
`Postcodes.UkPostcode`, could mean either, so each name written through
the package says which.

## What it looks like

```jadpo check project=p71c2-needed file=app.jadpo hidden
application {}
```

```jadpo check project=p71c2-needed file=shared/providers/postcodes/v2/package.jadpo hidden
/// UK postcodes: reading them as people write them.
package Postcodes.v2 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71c2-needed file=shared/providers/postcodes/v2/types/uk_postcode.jadpo hidden
/// A UK postcode in standard form, such as "SW1A 1AA".
public type UkPostcode = Text {
    min_length: 5
    max_length: 8
}
```

```jadpo check project=p71c2-needed file=shared/providers/postcodes/v3/package.jadpo hidden
/// UK postcodes: reading them as people write them.
package Postcodes.v3 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71c2-needed file=shared/providers/postcodes/v3/types/uk_postcode.jadpo hidden
/// A UK postcode in standard form, such as "SW1A 1AA".
public type UkPostcode = Text {
    min_length: 5
    max_length: 8
}
```

The app holds its own `Postcodes` package at `v2` and at `v3`, each with
a `UkPostcode` type. The action's parameter writes `Postcodes.UkPostcode`,
with no version.

domains/addresses/actions/keep_postcode.jadpo:

```jadpo check project=p71c2-needed file=domains/addresses/actions/keep_postcode.jadpo expect=package.version_needed
use shared/providers/postcodes

/// Keeps a customer's postcode as it is.
action keep_postcode(postcode: Postcodes.UkPostcode) -> Postcodes.v3.UkPostcode {
    return postcode
}
```

```text output
error: `Postcodes` is held in two versions, so a name through it says which
 --> domains/addresses/actions/keep_postcode.jadpo:4:32
  |
4 | action keep_postcode(postcode: Postcodes.UkPostcode) -> Postcodes.v3.UkPostcode {
  |                                ^^^^^^^^^ which version?
  reason: The project holds `Postcodes.v2` and `Postcodes.v3` side by side while callers move across, so a name written through the package carries its version, and each use says which one it means.
  next: Write `Postcodes.v2.UkPostcode` or `Postcodes.v3.UkPostcode` (agent)
  rule: package.version_needed
  help: https://jadpo.dev/docs/errors/package.version_needed

check failed: 1 error, 6 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the code has moved to the new version**, write `v3`, as the action's
result already does:

```jadpo fragment
action keep_postcode(postcode: Postcodes.v3.UkPostcode) -> Postcodes.v3.UkPostcode {
    ...
}
```

```jadpo check project=p71c2-needed-v3 file=domains/addresses/actions/keep_postcode.jadpo hidden
use shared/providers/postcodes

/// Keeps a customer's postcode as it is.
action keep_postcode(postcode: Postcodes.v3.UkPostcode) -> Postcodes.v3.UkPostcode {
    return postcode
}
```

```jadpo check project=p71c2-needed-v3 file=app.jadpo hidden
application {}
```

```jadpo check project=p71c2-needed-v3 file=shared/providers/postcodes/v2/package.jadpo hidden
/// UK postcodes: reading them as people write them.
package Postcodes.v2 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71c2-needed-v3 file=shared/providers/postcodes/v2/types/uk_postcode.jadpo hidden
/// A UK postcode in standard form, such as "SW1A 1AA".
public type UkPostcode = Text {
    min_length: 5
    max_length: 8
}
```

```jadpo check project=p71c2-needed-v3 file=shared/providers/postcodes/v3/package.jadpo hidden
/// UK postcodes: reading them as people write them.
package Postcodes.v3 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71c2-needed-v3 file=shared/providers/postcodes/v3/types/uk_postcode.jadpo hidden
/// A UK postcode in standard form, such as "SW1A 1AA".
public type UkPostcode = Text {
    min_length: 5
    max_length: 8
}
```

The action takes and gives a `v3` postcode. A value of the other version
can't be passed for it, because the two are different types.

**If the code still uses the old version**, write `v2` for both:

```jadpo fragment
action keep_postcode(postcode: Postcodes.v2.UkPostcode) -> Postcodes.v2.UkPostcode {
    ...
}
```

```jadpo check project=p71c2-needed-v2 file=domains/addresses/actions/keep_postcode.jadpo hidden
use shared/providers/postcodes

/// Keeps a customer's postcode as it is.
action keep_postcode(postcode: Postcodes.v2.UkPostcode) -> Postcodes.v2.UkPostcode {
    return postcode
}
```

```jadpo check project=p71c2-needed-v2 file=app.jadpo hidden
application {}
```

```jadpo check project=p71c2-needed-v2 file=shared/providers/postcodes/v2/package.jadpo hidden
/// UK postcodes: reading them as people write them.
package Postcodes.v2 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71c2-needed-v2 file=shared/providers/postcodes/v2/types/uk_postcode.jadpo hidden
/// A UK postcode in standard form, such as "SW1A 1AA".
public type UkPostcode = Text {
    min_length: 5
    max_length: 8
}
```

```jadpo check project=p71c2-needed-v2 file=shared/providers/postcodes/v3/package.jadpo hidden
/// UK postcodes: reading them as people write them.
package Postcodes.v3 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71c2-needed-v2 file=shared/providers/postcodes/v3/types/uk_postcode.jadpo hidden
/// A UK postcode in standard form, such as "SW1A 1AA".
public type UkPostcode = Text {
    min_length: 5
    max_length: 8
}
```

The action stays on `v2` until it moves across. Once the app holds only
one version, a name that still carries the version is
[`package.version_unneeded`](../errors/package.version_unneeded.md).

## Related

- [Two versions side by side](../language/11-integrations.md#two-versions-side-by-side)
- [Where packages live](../language/11-integrations.md#where-packages-live)
