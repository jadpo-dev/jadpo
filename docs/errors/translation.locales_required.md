# `translation.locales_required`

Each language file is for one of the locales the application supports, and each supported locale words every message. With no `locales { }` there is nothing to check the wording against.

```jadpo check project=e-tr-locales-required file=app.jadpo hidden
application {}
```

```jadpo check project=e-tr-locales-required file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-locales-required file=domains/shipping/translations/en_gb.jadpo expect=translation.locales_required
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```text output
error: Translations need the application's locales
 --> domains/shipping/translations/en_gb.jadpo:1:1
  |
1 | translations {
  | ^^^^^^^^^^^^ no locales declared
  reason: Each language file is for one of the locales the application supports, and each supported locale words every message. None is declared, so there is nothing to check the wording against.
  next: Declare them in `app.jadpo`, for instance `locales { default: Locale.en_gb supported: [Locale.en_gb] }` (agent)
  rule: translation.locales_required
  help: https://jadpo.dev/docs/errors/translation.locales_required

check failed: 1 error, 3 files
```
