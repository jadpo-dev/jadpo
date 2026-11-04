# `protocol.mismatch`

A registry's OpenAPI document differs from the registry protocol.

## Why Jadpo refuses it

The `jadpo` tool is built against [the registry
protocol](../language/11-integrations.md#the-registry-protocol)'s table, so a
registry whose requests, answers or failures differ from it can't serve
`jadpo package add` or `jadpo package publish`. `jadpo package protocol
<document>` compares the document with the table, and lists each difference
under the request it is in.

## What it looks like

```jadpo check project=e-protocol-mismatch file=app.jadpo hidden
application {
    jadpo: "X.Y"
}
```

```sh run project=e-protocol-mismatch expect=protocol.mismatch
printf '{"openapi":"3.1.0"}' > main.json
jadpo package protocol main.json
```

```text output
error: `main.json` differs from the registry protocol
  reason: The `jadpo` tool is built against the registry protocol's table, so a registry that differs from it can't serve `jadpo package add` or `jadpo package publish`.
  next: Change the routes each line below names, then run this again (agent)
  rule: protocol.mismatch
  help: https://jadpo.dev/docs/errors/protocol.mismatch

document
  has no paths

Differs from the registry protocol: 1 difference, main.json
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If the registry application should follow the protocol**, change the routes
each line names, then write the document again and check it:

```sh
jadpo check
jadpo package protocol build/openapi/main.json
```

The check passes once the document matches the table, and the run exits 0.

## Related

- [The registry protocol](../language/11-integrations.md#the-registry-protocol)
- [`jadpo package protocol`](../language/14-tooling.md#jadpo-package-protocol)
