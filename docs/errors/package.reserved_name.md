# `package.reserved_name`

An app's own integration takes a name kept for an official package.

## Why Jadpo refuses it

Jadpo keeps a few names, such as `Stripe`, `Twilio` and `GitHub`, for
official packages, none of which exists yet. An app's own integration can't
take one, so `Stripe` in code can't come to mean two things when the
official package arrives.

## What it looks like

An app's own integration named `Stripe`:

```jadpo check project=p71-reserved file=app.jadpo hidden
application {}
```

`shared/providers/stripe/v1/package.jadpo`:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the integration is your own**, give the folder and the index a name of
your own, such as `stripe_api`:

```jadpo check project=p71-reserved-fix file=shared/providers/stripe_api/v1/package.jadpo
/// The app's own Stripe calls.
package StripeApi.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71-reserved-fix file=app.jadpo hidden
application {}
```

Code now writes `StripeApi`, which can't be mistaken for an official
package.

## Related

- [Names](../language/11-integrations.md#names)
- [Jadpo's own packages](../language/11-integrations.md#jadpos-own-packages)
- [Where packages live](../language/11-integrations.md#where-packages-live)
