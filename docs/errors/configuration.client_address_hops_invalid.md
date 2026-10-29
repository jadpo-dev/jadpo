# `configuration.client_address_hops_invalid`

`hops:` counts proxies from 1, a whole number or an `Int` setting that declares `min:` of 1 or more:

```jadpo check project=e-client-address-hops-invalid expect=configuration.client_address_hops_invalid file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.forwarded_for
        hops: 0
    }
}
```

```text output
error: `0` can't be a number of proxies
 --> app.jadpo:4:15
  |
4 |         hops: 0
  |               ^ below 1
  reason: `hops:` is how many proxies of the deployment's own sit in front of the app: a whole number from 1, or an `Int` setting whose type declares `min:` of at least 1.
  next: Write a whole number from 1, such as `hops: 1`, or read an `Int` setting that declares `min: 1` (agent)
  rule: configuration.client_address_hops_invalid
  help: docs/errors/configuration.client_address_hops_invalid.md

check failed: 1 error, 1 file
```
