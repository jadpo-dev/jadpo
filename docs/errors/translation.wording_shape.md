# `translation.wording_shape`

A member is written in the wrong shape for what it words.

## Why Jadpo refuses it

How a message is worded follows its fields. With no choice in it, it is a
plain string. With one enum field or one `PluralCount` field, it is a block
with a wording for each value or plural form. Written the other way, there
is no way to choose which words to show.

## What it looks like

`Parcels` has a plural count, and French words it as a single string.

```jadpo check project=e-tr-wording-shape file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-wording-shape file=domains/shipping/translations/parcels.jadpo hidden
/// How many parcels an order has.
type Parcels = Message {
    count: PluralCount
}
```

```jadpo check project=e-tr-wording-shape file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    parcels: {
        one: "{count} parcel"
        other: "{count} parcels"
    }
}
```

```jadpo check project=e-tr-wording-shape file=domains/shipping/translations/fr_fr.jadpo hidden
translations {
    order_shipped: "La commande de {name} a été expédiée."
}
```

`domains/shipping/translations/fr_fr.jadpo`:

```jadpo check project=e-tr-wording-shape file=domains/shipping/translations/fr_fr.jadpo expect=translation.wording_shape
translations {
    parcels: "{count} colis"
}
```

```text output
error: `parcels` is a string, but needs one wording for each value or plural form
 --> domains/shipping/translations/fr_fr.jadpo:2:5
  |
2 |     parcels: "{count} colis"
  |     ^^^^^^^ needs a block with a wording for each plural form: `one`, `many` and `other`
  reason: How a message is worded follows its type: no choice, a plain string; one enum field, a wording for each value of the enum; one `PluralCount` field, a wording for each plural form of the language.
  next: Write `parcels` as a block with a wording for each plural form: `one`, `many` and `other` (agent)
  rule: translation.wording_shape
  help: https://jadpo.dev/docs/errors/translation.wording_shape

check failed: 1 error, 4 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

The other way round, a message with no choice has one wording, so a block
is the wrong shape for it:

```jadpo check project=e-tr-wording-shape-block file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

```jadpo check project=e-tr-wording-shape-block file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

`domains/shipping/translations/en_gb.jadpo`:

```jadpo check project=e-tr-wording-shape-block file=domains/shipping/translations/en_gb.jadpo expect=translation.wording_shape
translations {
    order_shipped: {
        other: "Your order for {name} has shipped."
    }
}
```

```text output
error: `order_shipped` is a block, but needs one wording, a string
 --> domains/shipping/translations/en_gb.jadpo:2:5
  |
2 |     order_shipped: {
  |     ^^^^^^^^^^^^^ needs one wording, a string
  reason: How a message is worded follows its type: no choice, a plain string; one enum field, a wording for each value of the enum; one `PluralCount` field, a wording for each plural form of the language.
  next: Write `order_shipped` as one wording, a string (agent)
  rule: translation.wording_shape
  help: https://jadpo.dev/docs/errors/translation.wording_shape

check failed: 1 error, 3 files
```

The same error says when something in `translations { }` isn't a name
and its wording at all, or a member of the root file isn't inside a group.

## How to fix it

**If the message has a plural count or an enum field**, write a block with
a wording for each form or value. In `domains/shipping/translations/fr_fr.jadpo`:

```jadpo check project=e-tr-wording-shape-fix file=domains/shipping/translations/fr_fr.jadpo
translations {
    parcels: {
        one: "{count} colis"
        many: "{count} de colis"
        other: "{count} colis"
    }
}
```

```jadpo check project=e-tr-wording-shape-fix file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-wording-shape-fix file=domains/shipping/translations/parcels.jadpo hidden
/// How many parcels an order has.
type Parcels = Message {
    count: PluralCount
}
```

```jadpo check project=e-tr-wording-shape-fix file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    parcels: {
        one: "{count} parcel"
        other: "{count} parcels"
    }
}
```

French now has a wording for each of its plural forms.

**If it has no choice**, write one wording, a string. In `domains/shipping/translations/en_gb.jadpo`:

```jadpo check project=e-tr-wording-shape-block-fix file=domains/shipping/translations/en_gb.jadpo
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```jadpo check project=e-tr-wording-shape-block-fix file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

```jadpo check project=e-tr-wording-shape-block-fix file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

The message has its one wording.

## Related

- [Languages](../language/10-configuration-and-time.md#languages)
- [Mistakes in a language file](../language/10-configuration-and-time.md#mistakes-in-a-language-file)
- [Enums with wording](../language/10-configuration-and-time.md#enums-with-wording)
