# `translation.field_unused`

No wording in a language uses one of a message's fields.

## Why Jadpo refuses it

A call gives every field of a message a value, so a language that never
writes one of them has either dropped a placeholder by mistake or has no
need of that word. The two look the same to Jadpo, so it stops rather than
letting a forgotten customer name ship in one language.

## What it looks like

`OrderShipped` has a field `name`. The English wording writes it, and the
French one leaves it out.

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

`domains/shipping/translations/fr_fr.jadpo`:

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
  help: https://jadpo.dev/docs/errors/translation.field_unused

check failed: 1 error, 4 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the placeholder was forgotten**, write it. In `domains/shipping/translations/fr_fr.jadpo`:

```jadpo check project=e-tr-field-unused-fix file=domains/shipping/translations/fr_fr.jadpo
translations {
    order_shipped: "La commande de {name} a été expédiée."
}
```

```jadpo check project=e-tr-field-unused-fix file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-field-unused-fix file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-field-unused-fix file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

The French subject now names the order, as the English one does.

**If no language needs the field**, remove it from the message and from
the wording that writes it. The message no longer takes that value, so
each call stops passing it:

```jadpo check project=e-tr-field-unused-remove file=domains/shipping/translations/order_shipped.jadpo
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {}
```

```jadpo check project=e-tr-field-unused-remove file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-field-unused-remove file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order has shipped."
}
```

```jadpo check project=e-tr-field-unused-remove file=domains/shipping/translations/fr_fr.jadpo hidden
translations {
    order_shipped: "La commande a été expédiée."
}
```

Every language now words the message without a name.

**If this language really needs no such word**, keep it and say why, with
a `decline` line above the member, in `domains/shipping/translations/fr_fr.jadpo`:

```jadpo check project=e-tr-field-unused-decline file=domains/shipping/translations/fr_fr.jadpo
translations {
    decline translation.field_unused: "French subject lines leave out the customer's name."
    order_shipped: "La commande a été expédiée."
}
```

```jadpo check project=e-tr-field-unused-decline file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-field-unused-decline file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-field-unused-decline file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

The reason stays in the code for a reviewer to read, and the check passes.

## Related

- [A field no wording uses](../language/10-configuration-and-time.md#a-field-no-wording-uses)
- [Messages](../language/10-configuration-and-time.md#messages)
- [Keeping code as it is, with a reason](../language/01-project-layout-and-modules.md#keeping-code-as-it-is-with-a-reason)
