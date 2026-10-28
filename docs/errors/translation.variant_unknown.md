# `translation.variant_unknown`

A language gives one wording for each plural form CLDR gives it. English has `one` and `other`, so `few` is a form it never uses.

```jadpo check project=e-tr-variant-unknown file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-variant-unknown file=domains/shipping/translations/parcels.jadpo hidden
/// How many parcels an order has.
type Parcels = Message {
    count: PluralCount
}
```

```jadpo check project=e-tr-variant-unknown file=domains/shipping/translations/fr_fr.jadpo hidden
translations {
    parcels: {
        one: "{count} colis"
        many: "{count} de colis"
        other: "{count} colis"
    }
}
```

```jadpo check project=e-tr-variant-unknown file=domains/shipping/translations/en_gb.jadpo expect=translation.variant_unknown
translations {
    parcels: {
        one: "{count} parcel"
        few: "{count} parcels, a few"
        other: "{count} parcels"
    }
}
```

```text output
error: `few` isn't a plural form of `Locale.en_gb`
 --> domains/shipping/translations/en_gb.jadpo:4:9
  |
4 |         few: "{count} parcels, a few"
  |         ^^^ no such wording to give
  reason: `Locale.en_gb` has no use for the plural form `few`: CLDR gives it `one` and `other`, so those are the forms a wording is written for.
  next: Remove `few` from `parcels`, or move its wording into one of the forms the language uses (agent)
  rule: translation.variant_unknown
  help: docs/language/10-configuration-and-time.html#translation-variant-unknown

check failed: 1 error, 4 files
```
