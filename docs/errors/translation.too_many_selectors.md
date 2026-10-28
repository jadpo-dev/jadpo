# `translation.too_many_selectors`

An enum field gives one wording for each of its values and a `PluralCount` field one for each plural form; together they would be a grid for a translator to fill.

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
  help: docs/errors/translation.too_many_selectors.md

check failed: 1 error, 3 files
```
