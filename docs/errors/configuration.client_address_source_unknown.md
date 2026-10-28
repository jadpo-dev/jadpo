# `configuration.client_address_source_unknown`

`source:` is one of the four `ClientAddressSource` values:

```jadpo check project=e-client-address-source-unknown expect=configuration.client_address_source_unknown file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.forward_for
        hops: 1
    }
}
```

```text output
error: `ClientAddressSource.forward_for` isn't a source of the client's address
 --> app.jadpo:3:17
  |
3 |         source: ClientAddressSource.forward_for
  |                 ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not a `ClientAddressSource`
  reason: `ClientAddressSource` has no value called `forward_for`; the four are `connection`, `forwarded_for`, `forwarded`, `cloudflare`. Did you mean `forwarded_for`?
  next: Use `ClientAddressSource.forwarded_for` instead (agent)
  rule: configuration.client_address_source_unknown
  help: docs/language/10-configuration-and-time.html#configuration-client-address-source-unknown

check failed: 1 error, 1 file
```
