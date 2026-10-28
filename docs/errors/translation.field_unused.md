# `translation.field_unused`

A call gives every field a value, so a language that never writes one is either right (it needs no such word) or has forgotten a placeholder. It is a judgement call: a person answers it with a reason.

```jadpo check project=e-tr-field-unused file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-field-unused file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-field-unused file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```jadpo check project=e-tr-field-unused file=domains/shipping/translations/fr_fr.jadpo expect=translation.field_unused
translations {
    order_shipped: "La commande a été expédiée."
}
```

```text output
error: No wording of `order_shipped` writes `{name}`
 --> domains/shipping/translations/fr_fr.jadpo:2:5
  |
2 |     order_shipped: "La commande a été expédiée."
  |     ^^^^^^^^^^^^^ `{name}` is never written
  reason: `OrderShipped` has a field `name`, and a call gives it a value, but this language never writes it. That is right for a language that needs no such word, and it is also what a forgotten placeholder looks like.
  next: Write `{name}` in a wording of `order_shipped`, or remove the field, or write `decline translation.field_unused: "<why>"` above it if this is deliberate (agent)
  rule: translation.field_unused
  help: docs/language/10-configuration-and-time.html#translation-field-unused

check failed: 1 error, 4 files
```
