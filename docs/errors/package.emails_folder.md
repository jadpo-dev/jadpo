# `package.emails_folder`

Email templates are the app's own, so a package has no `emails/`:

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
