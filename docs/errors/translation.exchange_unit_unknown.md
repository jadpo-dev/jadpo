# `translation.exchange_unit_unknown`

A file given to `jadpo translations import` holds a unit of wording the project doesn't have.

## Why Jadpo refuses it

`jadpo translations import` writes what a translation service sends back
into the project's language files, one unit for each message, named as
`jadpo translations export` wrote it. A unit with a name the project
doesn't have would word nothing, and is usually a name the service
changed or a message renamed since the export. The import reads the
whole file first, so it stops, points at the unit, and writes nothing.

## What it looks like

A project words an email subject in British English and is being
translated into French:

```jadpo check project=e-exchange-unit file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
    in_progress: [Locale.fr_fr]
}
```

```jadpo check project=e-exchange-unit file=domains/shipping/translations/order_shipped.jadpo hidden
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    name: Text
}
```

```jadpo check project=e-exchange-unit file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    order_shipped: "Your order for {name} has shipped."
}
```

The JSON the service sent back has the unit's name misspelt:

```sh run project=e-exchange-unit expect=translation.exchange_unit_unknown
cat > fr_fr.json <<'EOF2'
{
  "shipping.order_shiped": "Votre commande pour {name} a été expédiée."
}
EOF2
jadpo translations import fr_fr.json
```

```text output
error: `shipping.order_shiped` isn't a unit of wording in this project
 --> fr_fr.json:2:3
  |
2 |   "shipping.order_shiped": "Votre commande pour {name} a été expédiée."
  |   ^ no such unit
  reason: A unit is named `<domain>.<message>`, with `.<value>` or `.<plural form>` for a message whose wording is chosen by one, as `jadpo translations export` writes them. Did you mean `shipping.order_shipped`?
  next: Rename the unit to `shipping.order_shipped` in the file, or export again with `jadpo translations export fr_fr` (agent)
  rule: translation.exchange_unit_unknown
  help: https://jadpo.dev/docs/errors/translation.exchange_unit_unknown

Import stopped: nothing written, 1 error
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

A unit no name of the project is near, such as one for a message
deleted since the export, is refused with the export to make again.

## How to fix it

**If the unit is one of the project's, misspelt**, rename it in the
file as the `next:` line says, then import it:

```sh
sed -i.bak 's/shipping.order_shiped/shipping.order_shipped/' fr_fr.json
jadpo translations import fr_fr.json
```

The French wording goes into
`domains/shipping/translations/fr_fr.jadpo`.

**If the project's messages changed since the export**, export again,
send the new file to the service, and import what it returns:

```sh
jadpo translations export fr_fr --format json
```

The new file has a unit for each message the project has now.

## Related

- [Exchanging translations with a service](../language/10-configuration-and-time.md#exchanging-translations-with-a-service)
- [Import](../language/10-configuration-and-time.md#import)
