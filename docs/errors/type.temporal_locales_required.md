# `type.temporal_locales_required`

A project that formats text for people declares the locales it supports:

```jadpo check project=e-locales-required file=app.jadpo hidden
application {}
```

```jadpo check project=e-locales-required file=domains/shop/types/ordered_at.jadpo hidden
/// When an order was placed.
type OrderedAt = Instant {}
```

```jadpo check project=e-locales-required file=domains/shop/types/placed_text.jadpo hidden
/// An order's placing, shown to its customer.
type PlacedText = Output {
    placed: PresentationText
}
```

```jadpo check project=e-locales-required file=domains/shop/actions/show_placed.jadpo expect=type.temporal_locales_required
use shop/types { OrderedAt, PlacedText }

/// The placing, in the customer's language.
action show_placed(ordered_at: OrderedAt, reader_locale: Locale) -> PlacedText {
    return PlacedText {
        placed: text.format(
            ordered_at,
            zone: Zone.utc,
            locale: reader_locale,
            format: TimeFormat.date_long
        )
    }
}
```

```text output
error: `text.format` needs the application's locales
 --> domains/shop/actions/show_placed.jadpo:6:17
  |
6 |         placed: text.format(
  |                 ^^^^^^^^^^^ no locales declared
  reason: Text for a person is written in one of the locales the application supports. None is declared, so there is nothing to check `locale:` against, and nothing for a request with no usable `Accept-Language` to fall back to.
  next: Declare them in `app.jadpo`, for instance `locales { default: Locale.en_gb supported: [Locale.en_gb] }` (agent)
  rule: type.temporal_locales_required
  help: docs/language/10-configuration-and-time.html#type-temporal-locales-required

check failed: 1 error, 4 files
```

The same rule covers choosing a locale from a request, since there is
nothing to choose among and nothing to fall back to:

```jadpo check project=e-locales-required-choose file=app.jadpo hidden
application {}
```

```jadpo check project=e-locales-required-choose file=domains/shop/types/browser_languages.jadpo hidden
/// What a browser says its reader's languages are.
type BrowserLanguages = Text {
    max_length: 2000
}
```

```jadpo check project=e-locales-required-choose file=domains/shop/actions/locale_for_reader.jadpo expect=type.temporal_locales_required
use shop/types { BrowserLanguages }

/// The language the customer's browser asks for.
action locale_for_reader(languages_header: BrowserLanguages?) -> Locale {
    return locale.from_accept_language(languages_header)
}
```

```text output
error: `locale.from_accept_language` needs the application's locales
 --> domains/shop/actions/locale_for_reader.jadpo:5:12
  |
5 |     return locale.from_accept_language(languages_header)
  |            ^^^^^^^^^^^^^^^^^^^^^^^^^^^ no locales declared
  reason: `locale.from_accept_language` chooses among the locales the application supports and falls back to its `default:`. None is declared, so there is nothing to choose among and nothing to fall back to.
  next: Declare them in `app.jadpo`, for instance `locales { default: Locale.en_gb supported: [Locale.en_gb] }` (agent)
  rule: type.temporal_locales_required
  help: docs/language/10-configuration-and-time.html#type-temporal-locales-required

check failed: 1 error, 3 files
```

And sorting by a locale's alphabet, since a deployable carries collation
for the declared locales only:

```jadpo check project=e-locales-required-sort file=app.jadpo hidden
application {}
```

```jadpo check project=e-locales-required-sort file=domains/shop/types/loaf_name.jadpo hidden
/// A loaf's name on the shelf.
type LoafName = Text {
    min_length: 1
    max_length: 60
}
```

```jadpo check project=e-locales-required-sort file=domains/shop/actions/in_name_order.jadpo expect=type.temporal_locales_required
use shop/types { LoafName }

/// The loaves in the order the shopper's alphabet puts them.
action in_name_order(loaves: List<LoafName>, shopper: Locale) -> List<LoafName> {
    return list.sort_by(loaves, loaf -> loaf, locale: shopper)
}
```

```text output
error: `list.sort_by` needs the application's locales
 --> domains/shop/actions/in_name_order.jadpo:5:12
  |
5 |     return list.sort_by(loaves, loaf -> loaf, locale: shopper)
  |            ^^^^^^^^^^^^ no locales declared
  reason: `locale:` orders text by the alphabet of one of the locales the application supports, and a deployable carries only those locales' collation. None is declared, so there is no alphabet to sort by.
  next: Declare them in `app.jadpo`, for instance `locales { default: Locale.en_gb supported: [Locale.en_gb] }` (agent)
  rule: type.temporal_locales_required
  help: docs/language/10-configuration-and-time.html#type-temporal-locales-required

check failed: 1 error, 3 files
```
