# `translation.member_unknown`

A member of `translations { }` words one message, enum or email of the folder's scope, by its name in lower_snake_case.

```jadpo check project=e-tr-member-unknown file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-member-unknown file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-member-unknown file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```jadpo check project=e-tr-member-unknown file=domains/shipping/translations/fr_fr.jadpo hidden
translations {
    order_shipped: "La commande de {name} a été expédiée."
}
```

```jadpo check project=e-tr-member-unknown file=domains/shipping/translations/fr_fr.jadpo expect=translation.member_unknown
translations {
    order_shipped: "La commande de {name} a été expédiée."
    order_cancelled: "La commande est annulée."
}
```

```text output
error: `order_cancelled` words nothing
 --> domains/shipping/translations/fr_fr.jadpo:3:5
  |
3 |     order_cancelled: "La commande est annulée."
  |     ^^^^^^^^^^^^^^^ no such message, enum or email
  reason: A member of `translations { }` words one message, enum or email of this folder's scope, by its name in lower_snake_case, or a failure by its code. `order_cancelled` is none of them; the scope has `order_shipped`.
  next: Remove `order_cancelled` from `domains/shipping/translations/fr_fr.jadpo`, or declare the message it words beside the language files (agent)
  rule: translation.member_unknown
  help: docs/language/10-configuration-and-time.html#translation-member-unknown

check failed: 1 error, 4 files
```
