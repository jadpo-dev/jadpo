# `translation.file_name_invalid`

A language file is not named for a locale.

## Why Jadpo refuses it

Jadpo reads which language a file words from the file's name, so a name
that isn't a locale, such as `pt-BR` or `french`, leaves it with no language
to check the wording against. A locale tag in a file name is written as
`Locale` writes it, in lower case with `_` for `-`.

## What it looks like

The Brazilian Portuguese file is named with its tag as it is usually
written, with a hyphen and a capital.

```jadpo check project=e-tr-file-name-invalid file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-file-name-invalid file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-file-name-invalid file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```jadpo check project=e-tr-file-name-invalid file=domains/shipping/translations/fr_fr.jadpo hidden
translations {
    order_shipped: "La commande de {name} a été expédiée."
}
```

`domains/shipping/translations/pt-BR.jadpo`:

```jadpo check project=e-tr-file-name-invalid file=domains/shipping/translations/pt-BR.jadpo expect=translation.file_name_invalid
translations {
    order_shipped: "O seu pedido de {name} foi enviado."
}
```

```text output
error: `domains/shipping/translations/pt-BR.jadpo` isn't named for a locale
 --> domains/shipping/translations/pt-BR.jadpo:1:1
  |
1 | translations {
  | ^^^^^^^^^^^^ `pt-BR` is no `Locale`
  reason: A language file is named for the locale it words, the tag in lower case with `_` for `-`: `en_gb.jadpo` is `Locale.en_gb`. Did you mean `pt_br`?
  next: Rename the file to `domains/shipping/translations/pt_br.jadpo` (agent)
  rule: translation.file_name_invalid
  help: https://jadpo.dev/docs/errors/translation.file_name_invalid

check failed: 1 error, 5 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the file is for a locale the application supports**, rename it for
that locale: `domains/shipping/translations/pt_br.jadpo`.

```jadpo check project=e-tr-file-name-invalid-fix file=domains/shipping/translations/pt_br.jadpo
translations {
    order_shipped: "O seu pedido de {name} foi enviado."
}
```

```jadpo check project=e-tr-file-name-invalid-fix file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr, Locale.pt_br]
}
```

```jadpo check project=e-tr-file-name-invalid-fix file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-file-name-invalid-fix file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```jadpo check project=e-tr-file-name-invalid-fix file=domains/shipping/translations/fr_fr.jadpo hidden
translations {
    order_shipped: "La commande de {name} a été expédiée."
}
```

`Locale.pt_br` is listed in `supported:`, so Jadpo now checks this file as
its wording.

## Related

- [Languages](../language/10-configuration-and-time.md#languages)
- [Locales](../language/10-configuration-and-time.md#locales)
- [Mistakes in a language file](../language/10-configuration-and-time.md#mistakes-in-a-language-file)
