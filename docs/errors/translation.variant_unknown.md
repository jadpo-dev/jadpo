# `translation.variant_unknown`

A wording is for a value or plural form that does not exist.

## Why Jadpo refuses it

A language gives one wording for each plural form CLDR gives it, and an
enum one for each of its values. A wording for anything else would never be
shown, and it is usually an enum value or form written by mistake.

## What it looks like

English has the plural forms `one` and `other`, so `few` is one it never
uses.

```jadpo check project=e-tr-variant-unknown file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-variant-unknown file=domains/shipping/translations/parcels.jadpo hidden
/// How many parcels an order has.
type Parcels = Message {
    count: PluralCount
}
```

```jadpo check project=e-tr-variant-unknown file=domains/shipping/translations/fr_fr.jadpo hidden
translations {
    parcels: {
        one: "{count} colis"
        many: "{count} de colis"
        other: "{count} colis"
    }
}
```

`domains/shipping/translations/en_gb.jadpo`:

```jadpo check project=e-tr-variant-unknown file=domains/shipping/translations/en_gb.jadpo expect=translation.variant_unknown
translations {
    parcels: {
        one: "{count} parcel"
        few: "{count} parcels, a few"
        other: "{count} parcels"
    }
}
```

```text output
error: `few` isn't a plural form of `Locale.en_gb`
 --> domains/shipping/translations/en_gb.jadpo:4:9
  |
4 |         few: "{count} parcels, a few"
  |         ^^^ no such wording to give
  reason: `Locale.en_gb` has no use for the plural form `few`: CLDR gives it `one` and `other`, so those are the forms a wording is written for.
  next: Remove `few` from `parcels`, or move its wording into one of the forms the language uses (agent)
  rule: translation.variant_unknown
  help: https://jadpo.dev/docs/errors/translation.variant_unknown

check failed: 1 error, 4 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An enum field gives one wording for each of its values, so a value the
enum doesn't have is refused the same way:

```jadpo check project=e-tr-variant-unknown-enum file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

```jadpo check project=e-tr-variant-unknown-enum file=domains/shipping/types/formality.jadpo hidden
type Formality = Enum {
    formal
    casual
}
```

```jadpo check project=e-tr-variant-unknown-enum file=domains/shipping/translations/greeting.jadpo hidden
use shipping/types { Formality }

/// How a notice greets the reader.
type Greeting = Message {
    formality: Formality
}
```

`domains/shipping/translations/en_gb.jadpo`:

```jadpo check project=e-tr-variant-unknown-enum file=domains/shipping/translations/en_gb.jadpo expect=translation.variant_unknown
translations {
    greeting: {
        formal: "Dear customer"
        casual: "Hi"
        friendly: "Hello"
    }
}
```

```text output
error: `friendly` isn't a value of the enum `Formality`
 --> domains/shipping/translations/en_gb.jadpo:5:9
  |
5 |         friendly: "Hello"
  |         ^^^^^^^^ no such wording to give
  reason: `Formality` has no value `friendly`. Its values are `formal` and `casual`.
  next: Remove `friendly` from `greeting` (agent)
  rule: translation.variant_unknown
  help: https://jadpo.dev/docs/errors/translation.variant_unknown

check failed: 1 error, 4 files
```

## How to fix it

**If the language has no such form**, remove it, moving its wording into
one the language uses if you need it. In `domains/shipping/translations/en_gb.jadpo`:

```jadpo check project=e-tr-variant-unknown-fix file=domains/shipping/translations/en_gb.jadpo
translations {
    parcels: {
        one: "{count} parcel"
        other: "{count} parcels"
    }
}
```

```jadpo check project=e-tr-variant-unknown-fix file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-variant-unknown-fix file=domains/shipping/translations/parcels.jadpo hidden
/// How many parcels an order has.
type Parcels = Message {
    count: PluralCount
}
```

```jadpo check project=e-tr-variant-unknown-fix file=domains/shipping/translations/fr_fr.jadpo hidden
translations {
    parcels: {
        one: "{count} colis"
        many: "{count} de colis"
        other: "{count} colis"
    }
}
```

English now gives one wording for each form it has.

## Related

- [Languages](../language/10-configuration-and-time.md#languages)
- [Mistakes in a language file](../language/10-configuration-and-time.md#mistakes-in-a-language-file)
