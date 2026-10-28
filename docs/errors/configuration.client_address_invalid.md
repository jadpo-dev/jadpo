# `configuration.client_address_invalid`

`client_address:` is a block of `source:` and `hops:` and nothing else:

```jadpo check project=e-client-address-invalid expect=configuration.client_address_invalid file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.forwarded_for
        hop: 1
    }
}
```

```text output
error: `hop` isn't an option of `client_address:`
 --> app.jadpo:4:9
  |
4 |         hop: 1
  |         ^^^ unknown option
  reason: `client_address:` takes `source:` and `hops:`, and nothing else; which proxies to trust is a count and not a list. Did you mean `hops`?
  next: Use `hops` instead of `hop` (agent)
  rule: configuration.client_address_invalid
  help: docs/errors/configuration.client_address_invalid.md

check failed: 1 error, 1 file
```
