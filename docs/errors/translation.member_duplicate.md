# `translation.member_duplicate`

A member is written twice in a language file.

## Why Jadpo refuses it

A member words one message, enum or email once. If a file words it twice,
one of the two would be dropped, and a reader of the file could believe
either. Jadpo asks you to keep the one you mean. A group in the root
language file is refused the same way when it is written twice.

## What it looks like

The French file words `order_shipped` twice.

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

`domains/shipping/translations/fr_fr.jadpo`:

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
  help: https://jadpo.dev/docs/errors/translation.member_duplicate

check failed: 1 error, 4 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A value or plural form inside a member is written twice in the same way:

```jadpo check project=e-tr-member-duplicate-form file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-member-duplicate-form file=domains/shipping/translations/parcels.jadpo hidden
/// How many parcels an order has.
type Parcels = Message {
    count: PluralCount
}
```

```jadpo check project=e-tr-member-duplicate-form file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    parcels: {
        one: "{count} parcel"
        other: "{count} parcels"
    }
}
```

`domains/shipping/translations/fr_fr.jadpo`:

```jadpo check project=e-tr-member-duplicate-form file=domains/shipping/translations/fr_fr.jadpo expect=translation.member_duplicate
translations {
    parcels: {
        one: "{count} colis"
        one: "{count} paquet"
        many: "{count} de colis"
        other: "{count} colis"
    }
}
```

```text output
error: `one` is written twice in `parcels`
 --> domains/shipping/translations/fr_fr.jadpo:4:9
  |
3 |         one: "{count} colis"
  |         --- first written here
4 |         one: "{count} paquet"
  |         ^^^ written again
  reason: Each value or plural form has one wording.
  next: Remove one `one` (agent)
  rule: translation.member_duplicate
  help: https://jadpo.dev/docs/errors/translation.member_duplicate

check failed: 1 error, 4 files
```

## How to fix it

**If one of them is the wording you want**, remove the other. In `domains/shipping/translations/fr_fr.jadpo`:

```jadpo check project=e-tr-member-duplicate-fix file=domains/shipping/translations/fr_fr.jadpo
translations {
    order_shipped: "La commande de {name} a été expédiée."
}
```

```jadpo check project=e-tr-member-duplicate-fix file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-member-duplicate-fix file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-member-duplicate-fix file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

The message has one French wording.

## Related

- [Mistakes in a language file](../language/10-configuration-and-time.md#mistakes-in-a-language-file)
- [Languages](../language/10-configuration-and-time.md#languages)
