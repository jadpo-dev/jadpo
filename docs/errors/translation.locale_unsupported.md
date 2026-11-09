# `translation.locale_unsupported`

A language file is for a locale the application does not support.

## Why Jadpo refuses it

Wording is written for the locales `locales { supported: }` lists, and the
server never guesses a language. A file for any other locale would be read
by nothing, so its translator's work would quietly never reach anyone.

## What it looks like

The application supports English and French, and a German file has been
added.

```jadpo check project=e-tr-locale-unsupported file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-locale-unsupported file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-locale-unsupported file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```jadpo check project=e-tr-locale-unsupported file=domains/shipping/translations/fr_fr.jadpo hidden
translations {
    order_shipped: "La commande de {name} a été expédiée."
}
```

`domains/shipping/translations/de_de.jadpo`:

```jadpo check project=e-tr-locale-unsupported file=domains/shipping/translations/de_de.jadpo expect=translation.locale_unsupported
translations {
    order_shipped: "Ihre Bestellung für {name} wurde versandt."
}
```

```text output
error: `domains/shipping/translations/de_de.jadpo` is for a locale this application doesn't support
 --> domains/shipping/translations/de_de.jadpo:1:1
  |
1 | translations {
  | ^^^^^^^^^^^^ `Locale.de_de` isn't supported
  reason: Wording is written for the locales listed in `locales { supported: }`, in `app.jadpo`; a file for another is never used, and the server never guesses a language.
  next: Add `Locale.de_de` to `supported:` in `app.jadpo`, or remove the file (agent)
  rule: translation.locale_unsupported
  help: https://jadpo.dev/docs/errors/translation.locale_unsupported

check failed: 1 error, 5 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the application should offer the language**, add it to `supported:` in
`app.jadpo`:

```jadpo check project=e-tr-locale-unsupported-fix file=app.jadpo
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr, Locale.de_de]
}
```

```jadpo check project=e-tr-locale-unsupported-fix file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-locale-unsupported-fix file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```jadpo check project=e-tr-locale-unsupported-fix file=domains/shipping/translations/fr_fr.jadpo hidden
translations {
    order_shipped: "La commande de {name} a été expédiée."
}
```

```jadpo check project=e-tr-locale-unsupported-fix file=domains/shipping/translations/de_de.jadpo hidden
translations {
    order_shipped: "Ihre Bestellung für {name} wurde versandt."
}
```

German is now one of the application's languages, so `locale:` may name
it, and every message needs German wording.

**If it shouldn't**, delete the file:

```sh
rm domains/shipping/translations/de_de.jadpo
```

```jadpo check project=e-tr-locale-unsupported-remove file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-locale-unsupported-remove file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-locale-unsupported-remove file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```jadpo check project=e-tr-locale-unsupported-remove file=domains/shipping/translations/fr_fr.jadpo hidden
translations {
    order_shipped: "La commande de {name} a été expédiée."
}
```

The application keeps the languages it had.

## Related

- [Locales](../language/10-configuration-and-time.md#locales)
- [Languages](../language/10-configuration-and-time.md#languages)
- [Mistakes in a language file](../language/10-configuration-and-time.md#mistakes-in-a-language-file)
