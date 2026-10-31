# `translation.placeholder_unknown`

A wording writes a placeholder, such as `{nom}`, that isn't one of its message's fields.

## Why Jadpo refuses it

A message's fields are the only values its wording can write, each as a
`{field}` placeholder. A placeholder that names no field has no value to
fill it, so the sentence could never be written. It is most often a
field's name that was translated or mistyped along with the text.

## What it looks like

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

The message `OrderShipped` has one field, `name`, and the English file
writes it as `{name}`. The French file translates the placeholder too.

domains/shipping/translations/fr_fr.jadpo:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the placeholder means a field the message has**, write that field's
name, untranslated. In `domains/shipping/translations/fr_fr.jadpo`:

```jadpo fragment
    order_shipped: "La commande de {name} a été expédiée."
```

```jadpo check project=e-tr-placeholder-unknown-fix file=domains/shipping/translations/fr_fr.jadpo hidden
translations {
    order_shipped: "La commande de {name} a été expédiée."
}
```

```jadpo check project=e-tr-placeholder-unknown-fix file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-placeholder-unknown-fix file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-placeholder-unknown-fix file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

The French wording now writes the customer's name where `{name}` stands.

A wording that needs a value the message doesn't carry needs a new field
on the message, `OrderShipped` here. Each call of the message then passes
it, and each language's file writes it, since a field a language never
writes is [`translation.field_unused`](../errors/translation.field_unused.md).

## Related

- [Messages](../language/10-configuration-and-time.md#messages)
- [Languages](../language/10-configuration-and-time.md#languages)
- [Mistakes in a language file](../language/10-configuration-and-time.md#mistakes-in-a-language-file)
