# `translation.placeholder_unknown`

A wording writes only the fields its message has.

```jadpo check project=e-tr-placeholder-unknown file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-placeholder-unknown file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-placeholder-unknown file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```jadpo check project=e-tr-placeholder-unknown file=domains/shipping/translations/fr_fr.jadpo expect=translation.placeholder_unknown
translations {
    order_shipped: "La commande de {nom} a été expédiée."
}
```

```text output
error: `{nom}` isn't a field of `OrderShipped`
 --> domains/shipping/translations/fr_fr.jadpo:2:36
  |
2 |     order_shipped: "La commande de {nom} a été expédiée."
  |                                    ^^^^^ no such field
  reason: `OrderShipped` has no field `nom` a wording can write; this wording may write `{name}`.
  next: Remove `{nom}`, or add the field to `OrderShipped` (agent)
  rule: translation.placeholder_unknown
  help: https://jadpo.dev/docs/errors/translation.placeholder_unknown

check failed: 1 error, 4 files
```
