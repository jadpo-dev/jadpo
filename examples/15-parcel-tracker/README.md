# 15 · A parcel tracker in four languages

A courier's tracking page and returns page, written in English, German,
Arabic and, still being translated, Polish. It teaches translations:
messages as types, a file of wordings per language, plural forms from
each language's own rules, enums with wording, and the text Jadpo itself
writes.

Text for people is translated in Jadpo itself. Each message is a type
whose fields are its placeholders, in the domain that uses it; each
language is one file of plain wordings beside it. The compiler checks
every wording against its message in every supported language, so a
missing translation, a misspelt placeholder or a plural form a language
needs is a compile error, never a blank on the page. Code calls a
message as a typed function and gets `PresentationText`, to show and
never to store.

## What it teaches

1. **Locales, one still in progress.** `locales { }` lists the languages, and `in_progress:` names Polish, the one allowed to fall back to English while it is unfinished. See `app.jadpo` and [area 10, Locales](../../docs/language/10-configuration-and-time.md#locales).
2. **Messages.** A type built on `Message`, one per file in the domain's `translations/` folder, its fields the placeholders and its `///` the translator's context. See `domains/tracking/translations/delivery_estimate.jadpo` and [area 10, Messages](../../docs/language/10-configuration-and-time.md#messages).
3. **Languages and plural forms.** Each language is one `translations { }` file named after its locale; a `PluralCount` field gives one wording per plural form the language has, from CLDR: English two, Polish four, Arabic six. See `domains/tracking/translations/parcels_on_the_way.jadpo`, `domains/tracking/translations/en_gb.jadpo`, `domains/tracking/translations/ar_eg.jadpo` and [area 10, Languages](../../docs/language/10-configuration-and-time.md#languages).
4. **Enums with wording.** `ParcelStatus` is worded value by value in each language file, a value's fields as placeholders; an enum has no labels of its own. See `domains/tracking/types/parcel_status.jadpo` and [area 10, Enums with wording](../../docs/language/10-configuration-and-time.md#enums-with-wording).
5. **Using a message.** `translations.<domain>.<message>(...)` takes the fields by name and `locale:`, and gives `PresentationText`; a date is formatted in code and passed in. See `domains/tracking/actions/show_tracking.jadpo` and [area 10, Using a message](../../docs/language/10-configuration-and-time.md#using-a-message).
6. **The reader's language and direction.** `locale.from_accept_language` picks a supported locale from the browser's header, falling back to the default, and `locale.direction` says which way its script runs. Same file; [area 10, Choosing a locale from a request](../../docs/language/10-configuration-and-time.md#choosing-a-locale-from-a-request).
7. **A message two domains share.** The help line lives in `shared/translations/`, is `public`, and is called `translations.shared.help_line`. See `shared/translations/help_line.jadpo`, `domains/aftercare/actions/show_returns.jadpo` and [area 10, Where translations live](../../docs/language/10-configuration-and-time.md#where-translations-live).
8. **Jadpo's own text.** The root `translations/` holds the app's wording of what Jadpo writes, grouped under `jadpo:`; with locales declared, that is `TextDirection`'s wording, in every complete language Jadpo doesn't ship. See `translations/de_de.jadpo` and [area 10, Jadpo's and packages' text](../../docs/language/10-configuration-and-time.md#jadpos-and-packages-text).
9. **A language still in progress.** Polish words only the parcel count, so its page shows English for the rest, with no error, because `in_progress:` says so. See `domains/tracking/translations/pl_pl.jadpo` and `domains/tracking/actions/show_tracking.test.jadpo`.

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/15-parcel-tracker
target/release/jadpo test examples/15-parcel-tracker
target/release/jadpo fmt --check examples/15-parcel-tracker
cd examples/15-parcel-tracker && ../../target/release/jadpo translations missing
```

- `check` ends with `checked: <n> files` and no errors.
- `test` ends with `tested: 6 passed, 6 action tests`.
- `translations missing` lists the wording Polish still lacks, message by
  message, for a translator to fill in.

## Try next

- In `domains/tracking/translations/de_de.jadpo`, delete the `delivered` wording. `jadpo check` reports `translation.variant_missing`: every value of a worded enum needs words in every complete language.
  <!-- try {"file": "domains/tracking/translations/de_de.jadpo", "replace": "        delivered: \"Zugestellt\"\n", "with": "", "check": ["translation.variant_missing"]} -->
- In `domains/tracking/translations/en_gb.jadpo`, write `{count}` for `{parcels}` in the `other` form. `jadpo check` reports `translation.placeholder_unknown`: a placeholder is a field of the message, and `ParcelsOnTheWay` has no `count`.
  <!-- try {"file": "domains/tracking/translations/en_gb.jadpo", "replace": "other: \"{parcels} parcels on their way\"", "with": "other: \"{count} parcels on their way\"", "check": ["translation.placeholder_unknown"]} -->
- In `domains/tracking/translations/ar_eg.jadpo`, delete the `two:` form. `jadpo check` reports `translation.variant_missing`: Arabic has a form for exactly two, and CLDR says which forms each language needs.
  <!-- try {"file": "domains/tracking/translations/ar_eg.jadpo", "replace": "        two: \"طردان في الطريق\"\n", "with": "", "check": ["translation.variant_missing"]} -->
- In `app.jadpo`, take `in_progress:` out. `jadpo check` reports `translation.wording_missing` for each message Polish doesn't word yet, and `translation.file_missing` for each folder with no Polish file at all: a complete language has no fallback.
  <!-- try {"file": "app.jadpo", "replace": "    in_progress: [Locale.pl_pl]\n", "with": "", "check": ["translation.file_missing", "translation.wording_missing"]} -->
- That is the last example so far. The [language pages](../../docs/language/README.md) hold the full specification.
