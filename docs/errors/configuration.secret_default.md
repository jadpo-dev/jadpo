# `configuration.secret_default`

```jadpo check project=e-secret-default expect=configuration.secret_default
type PartnerKey = Text {
    min_length: 8
}

config {
    api_key: PartnerKey {
        secret: true
        default: "development-key"
    }
}
```

```text output
error: `api_key` is secret, and has a `default:` in source
 --> main.jadpo:8:9
  |
8 |         default: "development-key"
  |         ^^^^^^^ a secret written in source
  reason: A secret's value comes only from the deployment's environment, so a forgotten production secret stops the app instead of falling back to a value written in source; `jadpo test` generates its own for a test run, so it needs no `default:` either.
  next: Remove `default:` from `api_key` (agent)
  rule: configuration.secret_default
  help: docs/errors/configuration.secret_default.md

check failed: 1 error, 1 file
```
