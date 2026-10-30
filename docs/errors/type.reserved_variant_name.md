# `type.reserved_variant_name`

`.all` lists every variant of an enum whose variants carry no data, so
no enum declares a variant called `all`. A closed list
whose code is `all` writes it with a trailing `_`, as Albania's lek is
`Currency.all_`.

```jadpo check project=e-reserved-variant-name expect=type.reserved_variant_name
type Newsletter = Enum {
    subscribed
    all
}
```

```text output
error: `all` can't be a variant of `Newsletter`
 --> main.jadpo:3:5
  |
3 |     all
  |     ^^^ reserved
  reason: `Newsletter.all` lists every variant of `Newsletter`, so no variant takes the name `all`.
  next: Rename the variant (agent)
  rule: type.reserved_variant_name
  help: https://jadpo.dev/docs/errors/type.reserved_variant_name

check failed: 1 error, 1 file
```
