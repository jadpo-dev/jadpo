# `type.variant_retired`

A closed list keeps a code its authority withdraws, so
stored data and old messages still read, but new code that writes one
is refused: a shipment can't be sent to Yugoslavia. It is declinable,
since code that reads old records names the code on purpose, with
`decline type.variant_retired: "..."` and its reason above the
declaration. A `match` arm on one is reading, and isn't refused.

```jadpo check project=e-variant-retired expect=type.variant_retired
type Consignment = Object {
    delivered_to: Country
}

/// A consignment to a country by its old code.
function to_belgrade() -> Consignment {
    return Consignment {
        delivered_to: Country.yu
    }
}
```

```text output
error: `Country.yu` is a retired code
 --> main.jadpo:8:23
  |
8 |         delivered_to: Country.yu
  |                       ^^^^^^^^^^ only read
  reason: ISO 3166-1 withdrew `YU` (Yugoslavia). `Country.yu` stays so that stored data and old messages still read, but new code doesn't write it.
  next: Use the code that replaced it, or write `decline type.variant_retired: "<why>"` above it if this is deliberate (agent)
  rule: type.variant_retired
  help: https://jadpo.dev/docs/errors/type.variant_retired

check failed: 1 error, 1 file
```
