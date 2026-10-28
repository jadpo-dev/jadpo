# `configuration.client_address_duplicate`

`client_address:` is declared once, in `app.jadpo`; a second one:

```jadpo check project=e-client-address-duplicate expect=configuration.client_address_duplicate file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.connection
    }
    client_address: {
        source: ClientAddressSource.cloudflare
    }
}
```

```text output
error: `client_address:` is declared twice
 --> app.jadpo:5:5
  |
2 |     client_address: {
  |     -------------- first declared here
5 |     client_address: {
  |     ^^^^^^^^^^^^^^ declared again
  reason: The client's address is read one way for the whole application, so every request is counted by the same rule.
  next: Keep one `client_address:` and remove the other (agent)
  rule: configuration.client_address_duplicate
  help: docs/language/10-configuration-and-time.html#configuration-client-address-duplicate

check failed: 1 error, 1 file
```
