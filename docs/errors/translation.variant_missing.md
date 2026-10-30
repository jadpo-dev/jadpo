# `translation.variant_missing`

French counts a million as `many`, so a wording for `many` is part of a French plural.

```jadpo check project=e-tr-variant-missing file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-variant-missing file=domains/shipping/translations/parcels.jadpo hidden
/// How many parcels an order has.
type Parcels = Message {
    count: PluralCount
}
```

```jadpo check project=e-tr-variant-missing file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    parcels: {
        one: "{count} parcel"
        other: "{count} parcels"
    }
}
```

```jadpo check project=e-tr-variant-missing file=domains/shipping/translations/fr_fr.jadpo expect=translation.variant_missing
translations {
    parcels: {
        one: "{count} colis"
        other: "{count} colis"
    }
}
```

```text output
error: `parcels` in `domains/shipping/translations/fr_fr.jadpo` has no wording for the plural form `many`
 --> domains/shipping/translations/fr_fr.jadpo:2:5
  |
2 |     parcels: {
  |     ^^^^^^^ no `many`
  reason: `Locale.fr_fr` needs a wording for each plural form CLDR gives it, so a count in any of them has words to show.
  next: Add a wording for `many` to `parcels`, each able to write `{count}` (agent)
  rule: translation.variant_missing
  help: https://jadpo.dev/docs/errors/translation.variant_missing

check failed: 1 error, 4 files
```
