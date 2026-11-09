# `translation.member_unknown`

A language file words something that is no message, enum or email.

## Why Jadpo refuses it

A member of `translations { }` words one message, enum or email of the
folder it is in, by its name in lower_snake_case, or a failure by its code.
A member that matches nothing is text no one can ever call, and is most
often a name that was changed in the code and left behind here.

## What it looks like

The French file words a message the folder doesn't declare.

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

`domains/shipping/translations/fr_fr.jadpo`:

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
  help: https://jadpo.dev/docs/errors/translation.member_unknown

check failed: 1 error, 4 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Inside `jadpo: { }` in the root file a member has to be one of Jadpo's own
messages that this application can show, which `jadpo translations builtin`
lists:

```jadpo check project=e-tr-member-unknown-jadpo file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

`translations/en_gb.jadpo`:

```jadpo check project=e-tr-member-unknown-jadpo file=translations/en_gb.jadpo expect=translation.member_unknown
translations {
    jadpo: {
        text_directions: {
            left_to_right: "ltr"
            right_to_left: "rtl"
        }
    }
}
```

```text output
error: `text_directions` words nothing of Jadpo's
 --> translations/en_gb.jadpo:3:9
  |
3 |         text_directions: {
  |         ^^^^^^^^^^^^^^^ no such message of Jadpo's
  reason: `jadpo: { }` words Jadpo's own messages, by their names; `jadpo translations builtin` lists the ones this application can show. `text_directions` is none of them. Did you mean `text_direction`?
  next: Rename the member to `text_direction` (agent)
  rule: translation.member_unknown
  help: https://jadpo.dev/docs/errors/translation.member_unknown

check failed: 1 error, 2 files
```

A member of a package's group in the root file has to be one of that
package's public messages, enums or failures, which `jadpo package show`
lists.

## How to fix it

**If nothing words it any more**, remove it. In `domains/shipping/translations/fr_fr.jadpo`:

```jadpo check project=e-tr-member-unknown-remove file=domains/shipping/translations/fr_fr.jadpo
translations {
    order_shipped: "La commande de {name} a été expédiée."
}
```

```jadpo check project=e-tr-member-unknown-remove file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-member-unknown-remove file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-member-unknown-remove file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

The file words only what the folder declares.

**If the message should exist**, declare it beside the language files, and
word it in each of them. In `domains/shipping/translations/order_cancelled.jadpo`:

```jadpo check project=e-tr-member-unknown-declare file=domains/shipping/translations/order_cancelled.jadpo
/// Subject of the email sent when an order is cancelled.
type OrderCancelled = Message {}
```

```jadpo check project=e-tr-member-unknown-declare file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-member-unknown-declare file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-member-unknown-declare file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
    order_cancelled: "Your order has been cancelled."
}
```

```jadpo check project=e-tr-member-unknown-declare file=domains/shipping/translations/fr_fr.jadpo hidden
translations {
    order_shipped: "La commande de {name} a été expédiée."
    order_cancelled: "La commande est annulée."
}
```

`translations.shipping.order_cancelled` can now be called, and every
language words it.

## Related

- [Mistakes in a language file](../language/10-configuration-and-time.md#mistakes-in-a-language-file)
- [Where translations live](../language/10-configuration-and-time.md#where-translations-live)
- [Messages](../language/10-configuration-and-time.md#messages)
