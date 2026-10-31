# `translation.variant_missing`

A wording that changes with a count or with an enum's value leaves out one of the plural forms or values.

## Why Jadpo refuses it

A message with a count is worded once for each plural form its language
uses, and a message with an enum field once for each of the enum's
values. Each language has its own plural forms, from Unicode CLDR:
English has `one` and `other`, and French also has `many`, which it uses
for a million. A count or value with no wording would have no words to
show.

## What it looks like

```jadpo check project=e-tr-variant-missing file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-variant-missing file=domains/shipping/translations/parcels.jadpo hidden
/// How many parcels an order has.
type Parcels = Message {
    count: PluralCount
}
```

```jadpo check project=e-tr-variant-missing file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    parcels: {
        one: "{count} parcel"
        other: "{count} parcels"
    }
}
```

The message `Parcels` takes a count, and the English file words its two
English forms, `one` and `other`. The French file gives the same two.

domains/shipping/translations/fr_fr.jadpo:

```jadpo check project=e-tr-variant-missing file=domains/shipping/translations/fr_fr.jadpo expect=translation.variant_missing
translations {
    parcels: {
        one: "{count} colis"
        other: "{count} colis"
    }
}
```

```text output
error: `parcels` in `domains/shipping/translations/fr_fr.jadpo` has no wording for the plural form `many`
 --> domains/shipping/translations/fr_fr.jadpo:2:5
  |
2 |     parcels: {
  |     ^^^^^^^ no `many`
  reason: `Locale.fr_fr` needs a wording for each plural form CLDR gives it, so a count in any of them has words to show.
  next: Add a wording for `many` to `parcels`, each able to write `{count}` (agent)
  rule: translation.variant_missing
  help: https://jadpo.dev/docs/errors/translation.variant_missing

check failed: 1 error, 4 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A message with an enum field is worded once for each value. Here
`OrderShipped` changes with how formally the customer is addressed, and
the French file words only the formal value.

```jadpo check project=e-tr-variant-missing-enum file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-variant-missing-enum file=domains/shipping/types/formality.jadpo hidden
/// How formally a customer is addressed.
type Formality = Enum {
    formal
    casual
}
```

```jadpo check project=e-tr-variant-missing-enum file=domains/shipping/translations/order_shipped.jadpo hidden
use shipping/types { Formality }

/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    formality: Formality
    name: Text
}
```

```jadpo check project=e-tr-variant-missing-enum file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: {
        formal: "Good day {name}, your order has shipped."
        casual: "Hi {name}, your order is on its way!"
    }
}
```

domains/shipping/translations/fr_fr.jadpo:

```jadpo check project=e-tr-variant-missing-enum file=domains/shipping/translations/fr_fr.jadpo expect=translation.variant_missing
translations {
    order_shipped: {
        formal: "Bonjour {name}, votre commande a été expédiée."
    }
}
```

```text output
error: `order_shipped` in `domains/shipping/translations/fr_fr.jadpo` has no wording for the value `casual`
 --> domains/shipping/translations/fr_fr.jadpo:2:5
  |
2 |     order_shipped: {
  |     ^^^^^^^^^^^^^ no `casual`
  reason: `OrderShipped` is worded once for each value of `Formality`, so every value has words to show.
  next: Add a wording for `casual` to `order_shipped`, each able to write `{name}` (agent)
  rule: translation.variant_missing
  help: https://jadpo.dev/docs/errors/translation.variant_missing

check failed: 1 error, 5 files
```

## How to fix it

**If a plural form is missing**, add a wording for it. In
`domains/shipping/translations/fr_fr.jadpo`:

```jadpo fragment
        many: "{count} colis"
```

```jadpo check project=e-tr-variant-missing-plural file=domains/shipping/translations/fr_fr.jadpo hidden
translations {
    parcels: {
        one: "{count} colis"
        many: "{count} colis"
        other: "{count} colis"
    }
}
```

```jadpo check project=e-tr-variant-missing-plural file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-variant-missing-plural file=domains/shipping/translations/parcels.jadpo hidden
/// How many parcels an order has.
type Parcels = Message {
    count: PluralCount
}
```

```jadpo check project=e-tr-variant-missing-plural file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    parcels: {
        one: "{count} parcel"
        other: "{count} parcels"
    }
}
```

A French count of a million parcels now has its own wording.

**If an enum's value is missing**, add a wording for that value:

```jadpo fragment
        casual: "Salut {name}, ta commande est en route !"
```

```jadpo check project=e-tr-variant-missing-value file=domains/shipping/translations/fr_fr.jadpo hidden
translations {
    order_shipped: {
        formal: "Bonjour {name}, votre commande a été expédiée."
        casual: "Salut {name}, ta commande est en route !"
    }
}
```

```jadpo check project=e-tr-variant-missing-value file=domains/shipping/types/formality.jadpo hidden
/// How formally a customer is addressed.
type Formality = Enum {
    formal
    casual
}
```

```jadpo check project=e-tr-variant-missing-value file=domains/shipping/translations/order_shipped.jadpo hidden
use shipping/types { Formality }

/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    formality: Formality
    name: Text
}
```

```jadpo check project=e-tr-variant-missing-value file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: {
        formal: "Good day {name}, your order has shipped."
        casual: "Hi {name}, your order is on its way!"
    }
}
```

```jadpo check project=e-tr-variant-missing-value file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

A French customer addressed casually now gets the casual wording.

## Related

- [Languages](../language/10-configuration-and-time.md#languages)
- [Messages](../language/10-configuration-and-time.md#messages)
- [Mistakes in a language file](../language/10-configuration-and-time.md#mistakes-in-a-language-file)
