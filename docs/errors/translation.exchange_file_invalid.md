# `translation.exchange_file_invalid`

A file given to `jadpo translations import` isn't XLIFF 2.x or JSON in the shape `export` writes, or can't be read as one.

## Why Jadpo refuses it

`jadpo translations import` writes what a translation service sends back
into the project's language files. It reads two formats, XLIFF 2.0 or
2.1 and the JSON `export` writes, and it reads all of a file before it
writes anything. A file in another format, or one that isn't well
formed, could put wording in the wrong place, so the whole import stops,
says where in the file it went wrong, and nothing is written.

Jadpo won't convert the file for you: what a service exports is set in
the service, and only you can change it there.

## What it looks like

A project words an email subject in British English and is being
translated into French. The service sent back XLIFF 1.2, an older
format built differently:

```jadpo check project=e-exchange-file file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
    in_progress: [Locale.fr_fr]
}
```

```jadpo check project=e-exchange-file file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-exchange-file file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

```sh run project=e-exchange-file expect=translation.exchange_file_invalid
cat > fr_fr.xlf <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<xliff xmlns="urn:oasis:names:tc:xliff:document:1.2" version="1.2">
  <file original="domains/shipping/translations" source-language="en-GB" target-language="fr-FR" datatype="plaintext">
    <body>
      <trans-unit id="order_shipped">
        <source>Your order for {name} has shipped.</source>
        <target>Votre commande pour {name} a été expédiée.</target>
      </trans-unit>
    </body>
  </file>
</xliff>
EOF
jadpo translations import fr_fr.xlf
```

```text output
error: `fr_fr.xlf` isn't a translation file Jadpo can read
 --> fr_fr.xlf:2:1
  |
2 | <xliff xmlns="urn:oasis:names:tc:xliff:document:1.2" version="1.2">
  | ^ it is XLIFF 1.2, and Jadpo reads XLIFF 2.0 and 2.1; ask the service to export 2.x
  reason: `jadpo translations import` reads XLIFF 2.0 or 2.1, and JSON in the shape `export` writes: an object with a unit for each key, each a string or `{ "message": ... }`.
  next: Export again with `jadpo translations export`, upload that file and import what the service returns (human)
  rule: translation.exchange_file_invalid
  help: https://jadpo.dev/docs/errors/translation.exchange_file_invalid

Import stopped: nothing written, 1 error
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides how the project's wording goes to the translation service and
comes back.

A file that is XLIFF 2.x or JSON but isn't well formed, holds a unit
twice or in several segments, or was translated from another default
language is refused the same way, with what is wrong at the place in
the file where it is.

## How to fix it

The question is which format the service can send back.

**If the service exports XLIFF 2.0 or 2.1**, upload the file `jadpo
translations export` writes, choose XLIFF 2.0 when you download the
translations, and import that file:

```sh
jadpo translations export fr_fr
jadpo translations import fr_fr.xlf
```

Each translated wording goes into
`domains/shipping/translations/fr_fr.jadpo`, and the screen lists what
was added.

**If it only reads and writes JSON**, export JSON instead, and import
the JSON file it sends back:

```sh
jadpo translations export fr_fr --format json
jadpo translations import fr_fr.json
```

The JSON carries each wording's description too, and is imported the
same way.

## Related

- [Exchanging translations with a service](../language/10-configuration-and-time.md#exchanging-translations-with-a-service)
- [Import](../language/10-configuration-and-time.md#import)
- [Where translations live](../language/10-configuration-and-time.md#where-translations-live)
