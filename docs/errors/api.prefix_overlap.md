# `api.prefix_overlap`

Two APIs share a prefix.

## Why Jadpo refuses it

A URL says which API it belongs to through its prefix, and each API has its
own OpenAPI document and its own record. Two APIs at one prefix would mix
their routes and documents, and Jadpo can't tell which API a request was
meant for.

## What it looks like

app.jadpo:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If both APIs are needed**, give one a prefix of its own:

```jadpo check project=e-api-overlap-fixed file=app.jadpo
application {
    apis: {
        main: {
            prefix: /v1
        }
        partners: {
            prefix: /partners
        }
    }
}
```

`partners` now answers under `/partners`, and each API has its own document
and record.

## Related

- [APIs and versions](../language/07-http.md#apis-and-versions)
- [A new major version](../language/07-http.md#a-new-major-version)
