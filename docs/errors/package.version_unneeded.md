# `package.version_unneeded`

With one version held, a name never carries it, so code reads the same
before an upgrade and after it; the edit drops the version:

```jadpo check project=p71c2-unneeded file=app.jadpo hidden
application {}
```

```jadpo check project=p71c2-unneeded file=shared/providers/postcodes/v3/package.jadpo hidden
/// UK postcodes: reading them as people write them.
package Postcodes.v3 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71c2-unneeded file=shared/providers/postcodes/v3/types/uk_postcode.jadpo hidden
/// A UK postcode in standard form, such as "SW1A 1AA".
public type UkPostcode = Text {
    min_length: 5
    max_length: 8
}
```

```jadpo check project=p71c2-unneeded file=domains/addresses/actions/keep_postcode.jadpo expect=package.version_unneeded
use shared/providers/postcodes

/// Keeps a customer's postcode as it is.
action keep_postcode(postcode: Postcodes.v3.UkPostcode) -> Postcodes.UkPostcode {
    return postcode
}
```

```text output
error: `Postcodes` is held in one version, so its names don't carry it
 --> domains/addresses/actions/keep_postcode.jadpo:4:41
  |
4 | action keep_postcode(postcode: Postcodes.v3.UkPostcode) -> Postcodes.UkPostcode {
  |                                         ^^^ not needed
  reason: A name carries its package's version only while two versions sit side by side, to say which one it means; with one version of `Postcodes` held, `Postcodes.UkPostcode` is the one way to write it.
  next: Write `Postcodes.UkPostcode` (agent)
  rule: package.version_unneeded
  help: docs/errors/package.version_unneeded.md

check failed: 1 error, 4 files
```
