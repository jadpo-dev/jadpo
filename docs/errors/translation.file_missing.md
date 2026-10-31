# `translation.file_missing`

The app supports a locale, but a folder of messages has no language file for it.

## Why Jadpo refuses it

Each folder with messages has one language file per supported locale,
such as `fr_fr.jadpo` for French. A call of a message in a locale with no
file would have no words to show. Jadpo won't quietly show English
instead, because a supported locale is meant to be complete.

A locale that isn't finished yet says so in `in_progress:`, and only such
a locale falls back to the default where its wording is missing.

## What it looks like

app.jadpo:

```jadpo check project=e-tr-file-missing file=app.jadpo
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-file-missing file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

The shipping domain words its one message in English, and has no French
file.

domains/shipping/translations/en_gb.jadpo:

```jadpo check project=e-tr-file-missing file=domains/shipping/translations/en_gb.jadpo expect=translation.file_missing
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```text output
error: `Locale.fr_fr` is supported but has no language file for `order_shipped`
 --> app.jadpo:5:31
  |
5 |     supported: [Locale.en_gb, Locale.fr_fr]
  |                               ^^^^^^^^^^^^ no `domains/shipping/translations/fr_fr.jadpo`
  reason: Every supported locale has a language file for each folder that has messages, so a call with `locale: Locale.fr_fr` has wording to show. A locale that isn't finished yet is listed in `in_progress:`, and falls back to the default until it is.
  next: Add `domains/shipping/translations/fr_fr.jadpo` with a `translations { }` that words `order_shipped` (agent)
  rule: translation.file_missing
  help: https://jadpo.dev/docs/errors/translation.file_missing

check failed: 1 error, 3 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If French is ready**, add the French file, wording each message of the
folder. `domains/shipping/translations/fr_fr.jadpo`:

```jadpo check project=e-tr-file-missing-add file=domains/shipping/translations/fr_fr.jadpo
translations {
    order_shipped: "La commande de {name} a été expédiée."
}
```

```jadpo check project=e-tr-file-missing-add file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-file-missing-add file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-file-missing-add file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

A call of `order_shipped` with `locale: Locale.fr_fr` now writes the
French wording.

**If French is still being translated**, list it in `in_progress:` in
`app.jadpo`:

```jadpo fragment
    in_progress: [Locale.fr_fr]
```

```jadpo check project=e-tr-file-missing-progress file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
    in_progress: [Locale.fr_fr]
}
```

```jadpo check project=e-tr-file-missing-progress file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-file-missing-progress file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

A French call falls back to the English wording until the French file
exists. Removing French from `in_progress:` makes the gap an error again.

## Related

- [Missing wording](../language/10-configuration-and-time.md#missing-wording)
- [Locales still being translated: `in_progress:`](../language/10-configuration-and-time.md#locales-still-being-translated-in_progress)
- [Where translations live](../language/10-configuration-and-time.md#where-translations-live)
- [Languages](../language/10-configuration-and-time.md#languages)
