# `protocol.unreadable`

The document given to `jadpo package protocol` couldn't be read as OpenAPI's JSON.

## Why Jadpo refuses it

`jadpo package protocol <document>` checks a file, an `https://` address, or
an `http://` address on this machine. A file that isn't there, an answer other
than 200, or text that isn't JSON says nothing about whether the registry
conforms, so it is an error of its own and not a list of differences.

## What it looks like

```jadpo check project=e-protocol-unreadable file=app.jadpo hidden
application {
    jadpo: "X.Y"
}
```

```sh run project=e-protocol-unreadable expect=protocol.unreadable
jadpo package protocol build/openapi/main.json
```

```text output
error: `build/openapi/main.json` can't be read as an OpenAPI document
  reason: No such file or directory (os error 2).
  next: Give the file `jadpo check` writes, `build/openapi/<api>.json`, or the address its API serves it at (agent)
  rule: protocol.unreadable
  help: https://jadpo.dev/docs/errors/protocol.unreadable
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If it is your own application's document**, give the file `jadpo check`
writes:

```sh
jadpo check
jadpo package protocol build/openapi/main.json
```

The document is read from the checkout, with no server running.

**If an address answers 401**, the API takes a sign-in, so anyone reading its
document needs one too. `authentication: []` in the API's entry accepts no way
to sign in, which opens the document:

```jadpo fragment
registry: {
    prefix: /registry/v1
    authentication: []
}
```

```jadpo check project=e-protocol-unreadable-open file=app.jadpo hidden
application {
    apis: {
        registry: {
            prefix: /registry/v1
            authentication: []
        }
    }
}
```

The address then answers anyone, and so must every route in that API
([`api.route_needs_sign_in`](api.route_needs_sign_in.md)).

## Related

- [`jadpo package protocol`](../language/14-tooling.md#jadpo-package-protocol)
- [Serving the document](../language/07-http.md#serving-the-document)
