# `translation.wording_invalid`

A wording is not a plain string with {field} placeholders.

## Why Jadpo refuses it

A wording is a plain string whose only syntax is `{field}`, so a translator
can't break what code reads from it. A brace that is never closed leaves
Jadpo unable to tell where a placeholder ends, or which field it means. A
literal brace is written twice, `{{`.

## What it looks like

A brace in the French wording is never closed.

```jadpo check project=e-tr-wording-invalid file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-wording-invalid file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-wording-invalid file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

`domains/shipping/translations/fr_fr.jadpo`:

```jadpo check project=e-tr-wording-invalid file=domains/shipping/translations/fr_fr.jadpo expect=translation.wording_invalid
translations {
    order_shipped: "La commande de {name a été expédiée."
}
```

```text output
error: This wording isn't a plain string with `{field}` placeholders
 --> domains/shipping/translations/fr_fr.jadpo:2:36
  |
2 |     order_shipped: "La commande de {name a été expédiée."
  |                                    ^^^^^ this brace is never closed
  reason: A `{` starts a field, `{field}`, and a literal brace is doubled: `{{` writes `{`.
  next: Close the field with `}`, or write `{{` for a literal brace (agent)
  rule: translation.wording_invalid
  help: https://jadpo.dev/docs/errors/translation.wording_invalid

check failed: 1 error, 4 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the brace starts a placeholder**, close it. In `domains/shipping/translations/fr_fr.jadpo`:

```jadpo check project=e-tr-wording-invalid-fix file=domains/shipping/translations/fr_fr.jadpo
translations {
    order_shipped: "La commande de {name} a été expédiée."
}
```

```jadpo check project=e-tr-wording-invalid-fix file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-wording-invalid-fix file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-wording-invalid-fix file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

The wording writes the customer's name where `{name}` stands.

**If you meant a literal brace**, write it twice:

```jadpo check project=e-tr-wording-invalid-literal file=domains/shipping/translations/fr_fr.jadpo
translations {
    order_shipped: "La commande de {name} {{ expédiée }}."
}
```

```jadpo check project=e-tr-wording-invalid-literal file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-wording-invalid-literal file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-tr-wording-invalid-literal file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

The reader sees `{ expédiée }`, in braces, after the name.

## Related

- [Languages](../language/10-configuration-and-time.md#languages)
- [Mistakes in a language file](../language/10-configuration-and-time.md#mistakes-in-a-language-file)
