# `package.reserved_name`

The names of the providers Jadpo curates packages for are reserved, so
`Stripe` in code always means the official package:

```jadpo check project=p71-reserved file=app.jadpo hidden
application {}
```

```jadpo check project=p71-reserved file=shared/providers/stripe/v1/package.jadpo expect=package.reserved_name
/// The app's own Stripe calls.
package Stripe.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```text output
error: `Stripe` is a reserved provider name
 --> shared/providers/stripe/v1/package.jadpo
  reason: Jadpo reserves the names of the providers it curates packages for, so `Stripe` in code always means the official package. The app's own integration takes a name of its own.
  next: Rename the folder `shared/providers/stripe/v1` to a name of the app's own, such as `stripe_api`, and the index with it (agent)
  rule: package.reserved_name
  help: docs/errors/package.reserved_name.md

check failed: 1 error, 2 files
```
