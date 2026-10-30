# `api.prefix_required`

Every declared API says the path its routes start with:

```jadpo check project=e-api-prefix file=app.jadpo expect=api.prefix_required
application {
    apis: {
        main: {
            prefix: /v1
        }
        admin: {}
    }
}
```

```text output
error: API `admin` has no prefix
 --> app.jadpo:6:9
  |
6 |         admin: {}
  |         ^^^^^ no `prefix:`
  reason: Every declared API says the path its routes start with, so a URL says which API it belongs to: `main: { prefix: /v1 }`.
  next: Add `prefix: /admin` to `admin` (agent)
  rule: api.prefix_required
  help: https://jadpo.dev/docs/errors/api.prefix_required

check failed: 1 error, 1 file
```
