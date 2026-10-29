# `translation.placeholder_style`

An amount of money is written with its style, `{price, number,
currency}`, so a translator sees it is a price, and a percentage with
its own, `{rate, number, percent}`; a style on a field of another kind
is the same mistake the other way round.

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
  help: docs/errors/translation.placeholder_style.md

check failed: 1 error, 5 files
```
