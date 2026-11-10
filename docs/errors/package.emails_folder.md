# `package.emails_folder`

A package holds email templates.

## Why Jadpo refuses it

Email templates, partials and layouts are the app's own, in `emails/` of
the domain that sends them. A package sends no email itself, and an app
decides the wording and look of what it sends, so a package has nowhere to
put a template.

## What it looks like

A package has an `emails/` folder:

```jadpo check project=p71-emails file=app.jadpo hidden
application {}
```

```jadpo check project=p71-emails file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

`shared/providers/acme_couriers/v1/emails/booked.jadpo`:

```jadpo check project=p71-emails file=shared/providers/acme_couriers/v1/emails/booked.jadpo expect=package.emails_folder
type Booked = EmailMessage {}
```

```text output
error: A package can't hold email templates
 --> shared/providers/acme_couriers/v1/emails/booked.jadpo
  reason: Email templates, partials and layouts are the app's own, in `emails/` of the domain that sends them; a package sends no email itself.
  next: Move the email into a domain's `emails/` folder in the app, and remove it from the package (agent)
  rule: package.emails_folder
  help: https://jadpo.dev/docs/errors/package.emails_folder

check failed: 1 error, 3 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the app should send this email**, move it to `emails/` of the domain
that sends it, with its template. `domains/shipping/emails/booked.jadpo`:

```jadpo check project=p71-emails-moved file=domains/shipping/emails/booked.jadpo
/// Sent once a collection is booked.
type Booked = EmailMessage {}
```

`domains/shipping/emails/booked.txt`:

```jadpo check project=p71-emails-moved file=domains/shipping/emails/booked.txt
Your collection is booked.
```

```jadpo check project=p71-emails-moved file=app.jadpo hidden
application {
    display_name: "Couriers"
    connections: {
        mail: {
            driver: Driver.smtp
            host: "smtp.couriers.example"
        }
    }
    email: {
        connection: mail
        from: "hello@couriers.example"
    }
    events: {
        store: main
    }
}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

```jadpo check project=p71-emails-moved file=domains/shipping/translations/en_gb.jadpo hidden
translations {
    booked: "Your collection is booked"
}
```

```jadpo check project=p71-emails-moved file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

The email is now the app's own, and the package holds only what the
provider needs.

**If nothing sends it**, delete the folder:

```sh
rm -r shared/providers/acme_couriers/v1/emails
```

```jadpo check project=p71-emails-fix file=app.jadpo hidden
application {}
```

```jadpo check project=p71-emails-fix file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

The package holds no email templates, so the check passes.

## Related

- [The package format](../language/11-integrations.md#the-package-format)
- [What package code may do](../language/11-integrations.md#what-package-code-may-do)
- [Email](../language/16-built-in-services.md#email)
