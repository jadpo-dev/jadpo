# `translation.message_unknown`

A translation call names no message or enum.

## Why Jadpo refuses it

A call names a message by the domain it belongs to and its name, so a
misspelt or moved name would otherwise have no wording to write. Jadpo
refuses it where the call is written, and suggests the nearest name when
there is one.

## What it looks like

The call misspells `order_shipped`.

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

`domains/shipping/actions/describe_order.jadpo`:

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
  help: https://jadpo.dev/docs/errors/translation.message_unknown

check failed: 1 error, 6 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A library package's messages are called through the package,
`postcodes.translations.<message>(...)`, so the same error tells you when a
call has written a package as if it were a domain, or names something the
package doesn't offer.

## How to fix it

**If the name is misspelt**, call the message by its name. In `domains/shipping/actions/describe_order.jadpo`:

```jadpo fragment
        subject: translations.shipping.order_shipped(name: "Ana", locale: reader_locale)
```

```jadpo check project=e-tr-message-unknown-fix file=domains/shipping/actions/describe_order.jadpo hidden
use shipping/types { ShippingText }

/// What a customer is told about their order.
action describe_order(reader_locale: Locale) -> ShippingText {
    return ShippingText {
        subject: translations.shipping.order_shipped(name: "Ana", locale: reader_locale)
    }
}
```

```jadpo check project=e-tr-message-unknown-fix file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-message-unknown-fix file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-message-unknown-fix file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```jadpo check project=e-tr-message-unknown-fix file=domains/shipping/translations/fr_fr.jadpo hidden
translations {
    order_shipped: "La commande de {name} a été expédiée."
}
```

```jadpo check project=e-tr-message-unknown-fix file=domains/shipping/types/shipping_text.jadpo hidden
/// What a customer is told.
type ShippingText = Output {
    subject: PresentationText
}
```

The call now writes `order_shipped` in the reader's language.

## Related

- [Using a message](../language/10-configuration-and-time.md#using-a-message)
- [Messages](../language/10-configuration-and-time.md#messages)
- [Where translations live](../language/10-configuration-and-time.md#where-translations-live)
