# `translation.locale_unsupported`

A language file for a locale the application doesn't list in `locales { supported: }` would never be used.

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

```jadpo check project=e-tr-locale-unsupported file=domains/shipping/translations/de_de.jadpo expect=translation.locale_unsupported
translations {
    order_shipped: "Your order for {name} has shipped."
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
  help: docs/language/10-configuration-and-time.html#translation-locale-unsupported

check failed: 1 error, 5 files
```
