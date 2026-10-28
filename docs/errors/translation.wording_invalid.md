# `translation.wording_invalid`

A wording is a plain string whose only syntax is `{field}`, and a literal brace is doubled; here a brace is never closed.

```jadpo check project=e-tr-wording-invalid file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-wording-invalid file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-wording-invalid file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```jadpo check project=e-tr-wording-invalid file=domains/shipping/translations/fr_fr.jadpo expect=translation.wording_invalid
translations {
    order_shipped: "La commande de {name a été expédiée."
}
```

```text output
error: This wording isn't a plain string with `{field}` placeholders
 --> domains/shipping/translations/fr_fr.jadpo:2:36
  |
2 |     order_shipped: "La commande de {name a été expédiée."
  |                                    ^^^^^ this brace is never closed
  reason: A `{` starts a field, `{field}`, and a literal brace is doubled: `{{` writes `{`.
  next: Close the field with `}`, or write `{{` for a literal brace (agent)
  rule: translation.wording_invalid
  help: docs/language/10-configuration-and-time.html#translation-wording-invalid

check failed: 1 error, 4 files
```
