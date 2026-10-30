# `type.decimal_scale_required`

```jadpo check project=e-scale expect=type.decimal_scale_required
type ItemPrice = Decimal {
    min: 0.00
}
```

```text output
error: `ItemPrice` needs a `scale`
 --> main.jadpo:1:6
  |
1 | type ItemPrice = Decimal {
  |      ^^^^^^^^^ a `Decimal` type
  reason: A `Decimal` is exact with a fixed number of decimal places, its `scale`, which sets how it is stored and sent.
  next: Add `scale:` with the number of decimal places, such as `scale: 2` (agent)
  rule: type.decimal_scale_required
  help: https://jadpo.dev/docs/errors/type.decimal_scale_required

check failed: 1 error, 1 file
```
