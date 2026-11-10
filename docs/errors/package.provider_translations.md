# `package.provider_translations`

A provider package carries translations.

## Why Jadpo refuses it

A provider package shows people no text of its own: its failures are always
mapped to the app's own, which the app words. So translations in one would
never be shown. Only a library package carries translations, for its own
messages and failures.

## What it looks like

A provider package has a `translations/` folder:

```jadpo check project=p71-words file=app.jadpo hidden
application {}
```

```jadpo check project=p71-words file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71-words file=shared/providers/acme_couriers/v1/provider.jadpo hidden
provider {
    protocol: Protocol.json_http
}
```

`shared/providers/acme_couriers/v1/translations/booking_hint.jadpo`:

```jadpo check project=p71-words file=shared/providers/acme_couriers/v1/translations/booking_hint.jadpo expect=package.provider_translations
/// Shown under a booking.
type BookingHint = Message {}
```

```text output
error: Provider package `AcmeCouriers.v1` carries translations
 --> shared/providers/acme_couriers/v1/translations/booking_hint.jadpo
  reason: A provider package shows no text of its own: its failures are always mapped to the app's own, which the app words. Only a library package carries translations, for its own messages and failures.
  next: Remove the package's `translations/`, and word what users see in the app's own failures (agent)
  rule: package.provider_translations
  help: https://jadpo.dev/docs/errors/package.provider_translations

check failed: 1 error, 4 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the words are for people using the app**, remove the package's
`translations/`, and word what users see in the app's own failures:

```sh
rm -r shared/providers/acme_couriers/v1/translations
```

```jadpo check project=p71-words-fix file=app.jadpo hidden
application {}
```

```jadpo check project=p71-words-fix file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71-words-fix file=shared/providers/acme_couriers/v1/provider.jadpo hidden
provider {
    protocol: Protocol.json_http
}
```

The package holds no wording, and the app says everything users read.

## Related

- [Kinds of package](../language/11-integrations.md#kinds-of-package)
- [What package code may do](../language/11-integrations.md#what-package-code-may-do)
