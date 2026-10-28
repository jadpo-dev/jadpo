# `api.serve_openapi_invalid`

An API serves its OpenAPI document unless its entry turns it off with
`false`:

```jadpo check project=e-api-serve-openapi file=app.jadpo expect=api.serve_openapi_invalid
application {
    apis: {
        admin: {
            prefix: /admin
            serve_openapi: "no"
        }
    }
}
```

```text output
error: `"no"` isn't `true` or `false`
 --> app.jadpo:5:28
  |
5 |             serve_openapi: "no"
  |                            ^^^^ not a `Bool`
  reason: `serve_openapi:` says whether the API serves its OpenAPI document at `<prefix>/openapi.json`: `false` turns it off, and `true`, the default, can be left out.
  next: Write `serve_openapi: false`, or leave `serve_openapi:` out (agent)
  rule: api.serve_openapi_invalid
  help: docs/language/07-http.html#api-serve-openapi-invalid

check failed: 1 error, 1 file
```
