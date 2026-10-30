# `api.prefix_overlap`

Each API has a prefix of its own:

```jadpo check project=e-api-overlap file=app.jadpo expect=api.prefix_overlap
application {
    apis: {
        main: {
            prefix: /v1
        }
        partners: {
            prefix: /v1
        }
    }
}
```

```text output
error: APIs `main` and `partners` share the prefix `/v1`
 --> app.jadpo:6:9
  |
6 |         partners: {
  |         ^^^^^^^^ same prefix
  reason: Each API's routes start with its own prefix, so a URL says which API, OpenAPI document and record it belongs to.
  next: Give `partners` a prefix of its own (agent)
  rule: api.prefix_overlap
  help: https://jadpo.dev/docs/errors/api.prefix_overlap

check failed: 1 error, 1 file
```
