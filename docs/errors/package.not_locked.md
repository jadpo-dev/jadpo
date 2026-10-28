# `package.not_locked`

`packages/` holds only installed packages, each with its line in
`jadpo.lock`:

```jadpo check project=p71-unlocked file=app.jadpo hidden
application {}
```

```jadpo check project=p71-unlocked file=packages/postcodes/v2/package.jadpo expect=package.not_locked
/// UK postcodes.
package Postcodes.v2 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```text output
error: Package `Postcodes.v2` isn't in `jadpo.lock`
 --> packages/postcodes/v2/package.jadpo
  reason: `packages/` holds only installed packages, each with its line in `jadpo.lock`: its version, digest and source. A package with no line was copied in rather than installed, so nothing says where it came from.
  next: Install it with `jadpo package add`, or, if it is the app's own code, move it to `shared/providers/postcodes/v2/` (human)
  rule: package.not_locked
  help: docs/language/11-integrations.html#package-not-locked

check failed: 1 error, 2 files
```
