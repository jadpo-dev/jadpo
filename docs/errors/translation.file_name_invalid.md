# `translation.file_name_invalid`

A language file is named for the locale it words, the tag written as `Locale` writes it: lower case, with `_` for `-`.

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

```jadpo check project=e-tr-file-name-invalid file=domains/shipping/translations/pt-BR.jadpo expect=translation.file_name_invalid
translations {
    order_shipped: "Your order for {name} has shipped."
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
  help: docs/language/10-configuration-and-time.html#translation-file-name-invalid

check failed: 1 error, 5 files
```
