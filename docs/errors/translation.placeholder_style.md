# `translation.placeholder_style`

A wording writes an amount of money or a percentage without its style, or a style on another kind of field.

## Why Jadpo refuses it

An amount of money is written with its style, `{total, number, currency}`,
and a percentage with its own, `{rate, number, percent}`, so a translator
can see what the value is and that it is formatted for the reader. A style
on a field of another kind is the same mistake the other way round, since
the value isn't what the wording claims.

## What it looks like

`OrderTotal.total` is money. The Mexican Spanish wording writes it as if it
were text.

```jadpo check project=e-tr-placeholder-style file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.es_mx]
}
```

```jadpo check project=e-tr-placeholder-style file=domains/orders/types/order_amount.jadpo hidden
/// What an order costs, in the customer's currency.
type OrderAmount = Money {}
```

```jadpo check project=e-tr-placeholder-style file=domains/orders/translations/order_total.jadpo hidden
use orders/types { OrderAmount }

/// What an order comes to, on its confirmation page.
type OrderTotal = Message {
    total: OrderAmount
}
```

```jadpo check project=e-tr-placeholder-style file=domains/orders/translations/en_gb.jadpo hidden
translations {
    order_total: "Your order comes to {total, number, currency}."
}
```

`domains/orders/translations/es_mx.jadpo`:

```jadpo check project=e-tr-placeholder-style file=domains/orders/translations/es_mx.jadpo expect=translation.placeholder_style
translations {
    order_total: "Tu pedido suma {total}."
}
```

```text output
error: `{total}` is an amount of money, written `{total, number, currency}`
 --> domains/orders/translations/es_mx.jadpo:2:34
  |
2 |     order_total: "Tu pedido suma {total}."
  |                                  ^^^^^^^ no style
  reason: `OrderTotal.total` is money, and a wording writes an amount of money with its style, `{total, number, currency}`, so the translator sees it is one; it is shown as `text.format` shows it in the reader's locale, with its symbol and the currency's places.
  next: Write `{total, number, currency}` (agent)
  rule: translation.placeholder_style
  help: https://jadpo.dev/docs/errors/translation.placeholder_style

check failed: 1 error, 5 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A style on a field that is neither money nor a percentage is refused too:

```jadpo check project=e-tr-placeholder-style-other file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.es_mx]
}
```

```jadpo check project=e-tr-placeholder-style-other file=domains/orders/translations/order_note.jadpo hidden
/// A note shown on an order.
type OrderNote = Message {
    note: Text
}
```

```jadpo check project=e-tr-placeholder-style-other file=domains/orders/translations/en_gb.jadpo hidden
translations {
    order_note: "Note: {note}."
}
```

`domains/orders/translations/es_mx.jadpo`:

```jadpo check project=e-tr-placeholder-style-other file=domains/orders/translations/es_mx.jadpo expect=translation.placeholder_style
translations {
    order_note: "Nota: {note, number, currency}."
}
```

```text output
error: `{note, number, currency}` writes text as an amount of money
 --> domains/orders/translations/es_mx.jadpo:2:24
  |
2 |     order_note: "Nota: {note, number, currency}."
  |                        ^^^^^^^^^^^^^^^^^^^^^^^^ not an amount of money
  reason: `, number, currency` is the style of an amount of money, and `OrderNote.note` is text, which a wording writes as `{note}`.
  next: Write `{note}` (agent)
  rule: translation.placeholder_style
  help: https://jadpo.dev/docs/errors/translation.placeholder_style

check failed: 1 error, 4 files
```

## How to fix it

**If the field is an amount of money**, write it with its style. In
`domains/orders/translations/es_mx.jadpo`:

```jadpo check project=e-tr-placeholder-style-fix file=domains/orders/translations/es_mx.jadpo
translations {
    order_total: "Tu pedido suma {total, number, currency}."
}
```

```jadpo check project=e-tr-placeholder-style-fix file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.es_mx]
}
```

```jadpo check project=e-tr-placeholder-style-fix file=domains/orders/types/order_amount.jadpo hidden
/// What an order costs, in the customer's currency.
type OrderAmount = Money {}
```

```jadpo check project=e-tr-placeholder-style-fix file=domains/orders/translations/order_total.jadpo hidden
use orders/types { OrderAmount }

/// What an order comes to, on its confirmation page.
type OrderTotal = Message {
    total: OrderAmount
}
```

```jadpo check project=e-tr-placeholder-style-fix file=domains/orders/translations/en_gb.jadpo hidden
translations {
    order_total: "Your order comes to {total, number, currency}."
}
```

The amount is written for the reader's locale, with its symbol and the
currency's places.

**If the field isn't money or a percentage**, write its plain name, `{note}`,
with no style.

```jadpo check project=e-tr-placeholder-style-other-fix file=domains/orders/translations/es_mx.jadpo
translations {
    order_note: "Nota: {note}."
}
```

```jadpo check project=e-tr-placeholder-style-other-fix file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.es_mx]
}
```

```jadpo check project=e-tr-placeholder-style-other-fix file=domains/orders/translations/order_note.jadpo hidden
/// A note shown on an order.
type OrderNote = Message {
    note: Text
}
```

```jadpo check project=e-tr-placeholder-style-other-fix file=domains/orders/translations/en_gb.jadpo hidden
translations {
    order_note: "Note: {note}."
}
```

The wording writes the text as it is.

## Related

- [Dates, money and percentages in a message](../language/10-configuration-and-time.md#dates-money-and-percentages-in-a-message)
- [Mistakes in a language file](../language/10-configuration-and-time.md#mistakes-in-a-language-file)
