# `translation.wording_shape`

How a message is worded follows its fields: a plural count needs a wording for each plural form, so a plain string is the wrong shape.

```jadpo check project=e-tr-wording-shape file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-wording-shape file=domains/shipping/translations/parcels.jadpo hidden
/// How many parcels an order has.
type Parcels = Message {
    count: PluralCount
}
```

```jadpo check project=e-tr-wording-shape file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    parcels: {
        one: "{count} parcel"
        other: "{count} parcels"
    }
}
```

```jadpo check project=e-tr-wording-shape file=domains/shipping/translations/fr_fr.jadpo hidden
translations {
    order_shipped: "La commande de {name} a été expédiée."
}
```

```jadpo check project=e-tr-wording-shape file=domains/shipping/translations/fr_fr.jadpo expect=translation.wording_shape
translations {
    parcels: "{count} colis"
}
```

```text output
error: `parcels` is a string, but needs one wording for each value or plural form
 --> domains/shipping/translations/fr_fr.jadpo:2:5
  |
2 |     parcels: "{count} colis"
  |     ^^^^^^^ needs a block with a wording for each plural form: `one`, `many` and `other`
  reason: How a message is worded follows its type: no choice, a plain string; one enum field, a wording for each value of the enum; one `PluralCount` field, a wording for each plural form of the language.
  next: Write `parcels` as a block with a wording for each plural form: `one`, `many` and `other` (agent)
  rule: translation.wording_shape
  help: docs/errors/translation.wording_shape.md

check failed: 1 error, 4 files
```
