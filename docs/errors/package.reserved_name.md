# `package.reserved_name`

`Stripe`, `Twilio` and `GitHub` are kept for official packages, none of which
exists yet. An app's own integration can't take one, so `Stripe` in code can't
come to mean two things:

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
  reason: Jadpo keeps a few provider names for official packages, none of which exists yet, so `Stripe` in code can't come to mean two things. The app's own integration takes a name of its own.
  next: Rename the folder `shared/providers/stripe/v1` to a name of the app's own, such as `stripe_api`, and the index with it (agent)
  rule: package.reserved_name
  help: https://jadpo.dev/docs/errors/package.reserved_name

check failed: 1 error, 2 files
```
