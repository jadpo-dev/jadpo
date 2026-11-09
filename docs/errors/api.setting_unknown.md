# `api.setting_unknown`

An API in `apis:` has a setting an API doesn't take.

## Why Jadpo refuses it

An API has a fixed set of settings. One Jadpo doesn't know would do
nothing, so a mistyped `idempotency:` would leave the API unprotected while
looking as if it were set.

## What it looks like

app.jadpo:

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
  reason: An API in `apis:` holds its `prefix:`, `idempotency:`, `authentication:`, `serve_openapi:` and `rate_limit:`.
  next: Remove `version` (agent)
  rule: api.setting_unknown
  help: https://jadpo.dev/docs/errors/api.setting_unknown

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you meant a version of the API**, a version is another API with its
own prefix:

```jadpo check project=e-api-setting-version file=app.jadpo
application {
    apis: {
        main: {
            prefix: /v1
        }
        v2: {
            prefix: /v2
        }
    }
}
```

`v2` has its own routes, document and record, beside `main`.

**If the setting is a mistake**, remove it:

```jadpo check project=e-api-setting-removed file=app.jadpo
application {
    apis: {
        main: {
            prefix: /v1
        }
    }
}
```

The API keeps its prefix and the default for every other setting.

## Related

- [APIs and versions](../language/07-http.md#apis-and-versions)
- [A new major version](../language/07-http.md#a-new-major-version)
