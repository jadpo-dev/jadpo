# `api.setting_unknown`

```jadpo check project=e-api-setting file=app.jadpo expect=api.setting_unknown
application {
    apis: {
        main: {
            prefix: /v1
            version: 2
        }
    }
}
```

```text output
error: `version` isn't an API setting
 --> app.jadpo:5:13
  |
5 |             version: 2
  |             ^^^^^^^ unknown setting
  reason: An API in `apis:` holds its `prefix:`, `idempotency:`, `authentication:` and `serve_openapi:`, and later its `rate_limit:`.
  next: Remove `version` (agent)
  rule: api.setting_unknown
  help: https://jadpo.dev/docs/errors/api.setting_unknown

check failed: 1 error, 1 file
```
