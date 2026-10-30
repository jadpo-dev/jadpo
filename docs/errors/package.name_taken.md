# `package.name_taken`

A package's name joins the app's domain and library names:

```jadpo check project=p71-taken file=app.jadpo hidden
application {}
```

```jadpo check project=p71-taken file=domains/couriers/actions/book.jadpo hidden
action book() -> Unit {}
```

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
