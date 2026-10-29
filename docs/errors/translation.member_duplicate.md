# `translation.member_duplicate`

A member words one message once; a second would be dropped, or the first, whichever a reader of the file believed.

```jadpo check project=e-tr-member-duplicate file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-member-duplicate file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-member-duplicate file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```jadpo check project=e-tr-member-duplicate file=domains/shipping/translations/fr_fr.jadpo expect=translation.member_duplicate
translations {
    order_shipped: "La commande de {name} a été expédiée."
    order_shipped: "Expédiée."
}
```

```text output
error: `order_shipped` is written twice in `domains/shipping/translations/fr_fr.jadpo`
 --> domains/shipping/translations/fr_fr.jadpo:3:5
  |
2 |     order_shipped: "La commande de {name} a été expédiée."
  |     ------------- first written here
3 |     order_shipped: "Expédiée."
  |     ^^^^^^^^^^^^^ written again
  reason: A member words one message, enum or email, once; the second would be dropped or the first, whichever a reader of the file believed.
  next: Remove one `order_shipped` (agent)
  rule: translation.member_duplicate
  help: docs/errors/translation.member_duplicate.md

check failed: 1 error, 4 files
```
