# `configuration.secret_flow`

```jadpo check project=e-secret-flow expect=configuration.secret_flow
type PartnerKey = Text {
    min_length: 8
}

config {
    api_key: PartnerKey {
        secret: true
    }
}

function expose_secret() -> PartnerKey {
    return config.api_key
}
```

```text output
error: `config.api_key` is a secret
 --> main.jadpo:12:12
   |
12 |     return config.api_key
   |            ^^^^^^^^^^^^^^ a secret, read here
  reason: A secret never leaves the server, so it is accepted only where the compiler itself owns the place: a connection's `url:`, an integration's `credential:`, an authentication `signing_key:` and a `crypto` call's `key:`. Returning it, storing it, logging it, putting it in a failure, building another value from it or comparing it with something could leak it.
  next: Give `config.api_key` to the connection, integration or authentication that needs it in `app.jadpo` or `authentication.jadpo`, or as a `crypto` call's `key:`, or leave `secret: true` off `api_key` if its value is public (agent)
  rule: configuration.secret_flow
  help: https://jadpo.dev/docs/errors/configuration.secret_flow

check failed: 1 error, 1 file
```
