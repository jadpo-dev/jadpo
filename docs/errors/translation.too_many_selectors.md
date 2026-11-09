# `translation.too_many_selectors`

A message has more than one enum or plural field.

## Why Jadpo refuses it

An enum field gives one wording for each of its values, and a `PluralCount`
field one for each plural form of the language. Together they would need a
wording for every pairing of a value and a form, a grid a translator has to
fill in, for every language. Jadpo keeps each message to one choice.

## What it looks like

`Combo` chooses its wording by a `Formality` and by a count.

```jadpo check project=e-tr-too-many-selectors file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-too-many-selectors file=domains/shipping/types/formality.jadpo hidden
type Formality = Enum {
    formal
    casual
}
```

`domains/shipping/translations/combo.jadpo`:

```jadpo check project=e-tr-too-many-selectors file=domains/shipping/translations/combo.jadpo expect=translation.too_many_selectors
use shipping/types { Formality }

/// A combo of wordings.
type Combo = Message {
    formality: Formality
    count: PluralCount
}
```

```text output
error: `Combo` has two fields that choose its wording, `formality` and `count`
 --> domains/shipping/translations/combo.jadpo:6:5
  |
5 |     formality: Formality
  |     --------- the first
6 |     count: PluralCount
  |     ^^^^^ a second choice
  reason: An enum field gives one wording for each of its values, and a `PluralCount` field one for each plural form of the language. Both at once would need a wording for every pairing of a value and a form, which a translator would have to fill in.
  next: Split `Combo` into two messages, one for each choice, and call both (agent)
  rule: translation.too_many_selectors
  help: https://jadpo.dev/docs/errors/translation.too_many_selectors

check failed: 1 error, 3 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the message has two independent choices**, split it into two messages
and call both. In `domains/shipping/translations/greeting_style.jadpo`, with `ParcelCount` the same in a
file of its own:

```jadpo check project=e-tr-too-many-selectors-fix file=domains/shipping/translations/greeting_style.jadpo
use shipping/types { Formality }

/// How a parcel notice greets the reader.
type GreetingStyle = Message {
    formality: Formality
}
```

```jadpo check project=e-tr-too-many-selectors-fix file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-too-many-selectors-fix file=domains/shipping/types/formality.jadpo hidden
type Formality = Enum {
    formal
    casual
}
```

```jadpo check project=e-tr-too-many-selectors-fix file=domains/shipping/translations/parcel_count.jadpo hidden
/// How many parcels an order has.
type ParcelCount = Message {
    count: PluralCount
}
```

```jadpo check project=e-tr-too-many-selectors-fix file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    greeting_style: {
        formal: "Dear customer"
        casual: "Hi"
    }
    parcel_count: {
        one: "{count} parcel"
        other: "{count} parcels"
    }
}
```

```jadpo check project=e-tr-too-many-selectors-fix file=domains/shipping/translations/fr_fr.jadpo hidden
translations {
    greeting_style: {
        formal: "Cher client"
        casual: "Salut"
    }
    parcel_count: {
        one: "{count} colis"
        many: "{count} de colis"
        other: "{count} colis"
    }
}
```

Each language words the two messages on their own, and the code writes
them side by side.

## Related

- [Messages](../language/10-configuration-and-time.md#messages)
- [Enums with wording](../language/10-configuration-and-time.md#enums-with-wording)
