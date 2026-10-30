# `package.provider_translations`

A provider package shows people no text, since its failures are mapped to
the app's own:

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
