# `translation.file_missing`

Every supported locale has a language file for each folder that has messages, unless it is listed in `in_progress:`; here French has none.

```jadpo check project=e-tr-file-missing file=app.jadpo hidden
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
