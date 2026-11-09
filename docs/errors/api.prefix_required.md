# `api.prefix_required`

A declared API has no `prefix:`, or its prefix isn't a path.

## Why Jadpo refuses it

Every route in a declared API starts with the API's prefix, so a URL says
which API it belongs to. With no prefix, Jadpo has no path to put the API's
routes and OpenAPI document under, and it won't choose one for you.

## What it looks like

app.jadpo:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A prefix is written as a path, not as text:

```jadpo check project=e-api-prefix-text file=app.jadpo expect=api.prefix_required
application {
    apis: {
        main: {
            prefix: "/v1"
        }
    }
}
```

```text output
error: API `main`'s prefix isn't a path
 --> app.jadpo:4:13
  |
4 |             prefix: "/v1"
  |             ^^^^^^ not a path
  reason: An API's prefix is the path its routes start with: `prefix: /admin`.
  next: Write the prefix as a path starting with `/` (agent)
  rule: api.prefix_required
  help: https://jadpo.dev/docs/errors/api.prefix_required

check failed: 1 error, 1 file
```

## How to fix it

**If the API has no prefix yet**, add one. A prefix written as text loses
its quotes:

```jadpo check project=e-api-prefix-fixed file=app.jadpo
application {
    apis: {
        main: {
            prefix: /v1
        }
        admin: {
            prefix: /admin
        }
    }
}
```

`admin`'s routes now start with `/admin`.

## Related

- [APIs and versions](../language/07-http.md#apis-and-versions)
- [Route groups](../language/07-http.md#route-groups)
