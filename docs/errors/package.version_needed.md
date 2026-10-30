# `package.version_needed`

While two versions of a package sit side by side, a name written through
it says which one it means:

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
