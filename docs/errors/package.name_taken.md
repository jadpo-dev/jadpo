# `package.name_taken`

A package has the same name as one of the app's domains, libraries, declarations or other packages.

## Why Jadpo refuses it

A package's name is how code reaches it, as in `Couriers.Parcel`, and
it shares one set of names with the app's domains and libraries. If two
of them had the same name, a reader couldn't tell which one `Couriers`
means. So each name means one thing.

Jadpo won't choose which of the two to rename. A name is how people find
the code, so the choice is yours.

## What it looks like

```jadpo check project=p71-taken file=app.jadpo hidden
application {}
```

The app has a domain called `couriers`, with an action in
`domains/couriers/actions/book.jadpo`. Its own integration for a courier's
API is called `Couriers` too.

```jadpo check project=p71-taken file=domains/couriers/actions/book.jadpo hidden
action book() -> Unit {}
```

shared/providers/couriers/v1/package.jadpo:

```jadpo check project=p71-taken file=shared/providers/couriers/v1/package.jadpo expect=package.name_taken
/// A courier's booking API.
package Couriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```text output
error: Package `Couriers`'s name is already taken
 --> shared/providers/couriers/v1/package.jadpo
  reason: A package's name joins the names of the app's domains and libraries, which are unique among themselves, and two packages never share a name in code, so `Couriers.` means one thing; it is already domain `couriers`.
  next: Rename one of them (human)
  rule: package.name_taken
  help: https://jadpo.dev/docs/errors/package.name_taken

check failed: 1 error, 3 files
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides what the package and the domain are called.

## How to fix it

The question is which of the two keeps the name.

**If the domain keeps it**, rename the package after its provider, in a
folder of the same name, `shared/providers/acme_couriers/v1/`:

```jadpo check project=p71-taken-package file=shared/providers/acme_couriers/v1/package.jadpo
/// A courier's booking API.
package AcmeCouriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71-taken-package file=app.jadpo hidden
application {}
```

```jadpo check project=p71-taken-package file=domains/couriers/actions/book.jadpo hidden
action book() -> Unit {}
```

Code reaches the package as `AcmeCouriers`, and `couriers` stays the
domain. Each `use shared/providers/couriers` line becomes
`use shared/providers/acme_couriers`.

**If the package keeps it**, rename the domain by moving its folder, here
to `domains/deliveries/`:

```jadpo check project=p71-taken-domain file=domains/deliveries/actions/book.jadpo
action book() -> Unit {}
```

```jadpo check project=p71-taken-domain file=app.jadpo hidden
application {}
```

```jadpo check project=p71-taken-domain file=shared/providers/couriers/v1/package.jadpo hidden
/// A courier's booking API.
package Couriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

The domain is now `deliveries`, and `Couriers` means only the package.
Every `use` line and call that named the domain changes with it.

## Related

- [Names](../language/11-integrations.md#names)
- [Where packages live](../language/11-integrations.md#where-packages-live)
- [Naming](../language/01-project-layout-and-modules.md#naming)
