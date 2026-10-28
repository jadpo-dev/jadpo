# `translation.message_unknown`

A call names a message by its domain and its name; here the name is misspelt.

```jadpo check project=e-tr-message-unknown file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-message-unknown file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-message-unknown file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```jadpo check project=e-tr-message-unknown file=domains/shipping/translations/fr_fr.jadpo hidden
translations {
    order_shipped: "La commande de {name} a été expédiée."
}
```

```jadpo check project=e-tr-message-unknown file=domains/shipping/types/shipping_text.jadpo hidden
/// What a customer is told.
type ShippingText = Output {
    subject: PresentationText
}
```

```jadpo check project=e-tr-message-unknown file=domains/shipping/actions/describe_order.jadpo expect=translation.message_unknown
use shipping/types { ShippingText }

/// What a customer is told about their order.
action describe_order(reader_locale: Locale) -> ShippingText {
    return ShippingText {
        subject: translations.shipping.order_shiped(name: "Ana", locale: reader_locale)
    }
}
```

```text output
error: `translations.shipping.order_shiped` is not a message
 --> domains/shipping/actions/describe_order.jadpo:6:18
  |
6 |         subject: translations.shipping.order_shiped(name: "Ana", locale: reader_locale)
  |                  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ no such message
  reason: `order_shiped` is not a message or enum `shipping` words. Did you mean `order_shipped`?
  next: Use `translations.shipping.order_shipped` (agent)
  rule: translation.message_unknown
  help: docs/errors/translation.message_unknown.md

check failed: 1 error, 6 files
```
