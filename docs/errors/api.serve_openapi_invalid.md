# `api.serve_openapi_invalid`

An API's `serve_openapi:` setting holds something other than `true` or `false`.

## Why Jadpo refuses it

`serve_openapi:` says whether the API serves its OpenAPI document, the
description of its routes that other apps read to call it, at
`<prefix>/openapi.json`. The setting has only two answers. Any other value
is a mistake, and Jadpo won't guess whether `"no"` or `0` meant off.

## What it looks like

app.jadpo:

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
  help: https://jadpo.dev/docs/errors/api.serve_openapi_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the API shouldn't serve its document**, write `false`. In `app.jadpo`:

```jadpo fragment
            serve_openapi: false
```

```jadpo check project=e-api-serve-openapi-off file=app.jadpo hidden
application {
    apis: {
        admin: {
            prefix: /admin
            serve_openapi: false
        }
    }
}
```

The API stops serving `/admin/openapi.json`, and the route that served it
leaves the API's record and `jadpo routes`.

**If it should**, delete the `serve_openapi:` line, since serving the
document is the default. `app.jadpo` becomes:

```jadpo check project=e-api-serve-openapi-on file=app.jadpo
application {
    apis: {
        admin: {
            prefix: /admin
        }
    }
}
```

The API serves its document at `/admin/openapi.json`.

## Related

- [APIs and versions](../language/07-http.md#apis-and-versions)
- [Serving the document](../language/07-http.md#serving-the-document)
