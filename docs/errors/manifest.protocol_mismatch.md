# `manifest.protocol_mismatch`

A monitoring product's endpoint answers the manifest protocol's requests differently from the protocol.

## Why Jadpo refuses it

Every monitoring product is given Jadpo's build manifests through [the
manifest protocol](../language/16-built-in-services.md#the-manifest-protocol),
so that each decodes the same telemetry the same way. `jadpo manifest
protocol <endpoint>` sends the protocol's requests to a product's
endpoint with a sample build, and lists each answer that differs under
the request it was given. It exits 1, which is what stops the product's
CI.

## What it looks like

Nothing listens at the endpoint, so the first request gets no answer:

```jadpo check project=e-manifest-protocol-mismatch file=app.jadpo hidden
application {}
```

```sh run project=e-manifest-protocol-mismatch expect=manifest.protocol_mismatch
jadpo manifest protocol http://127.0.0.1:1
```

```text output
error: `http://127.0.0.1:1` differs from the manifest protocol
  reason: Every monitoring product decodes Jadpo's telemetry against the manifests it is given through the protocol, so a product that answers differently can't be given them, or decodes them differently from the others.
  next: Change what each request below answers, then run this again (agent)
  rule: manifest.protocol_mismatch
  help: https://jadpo.dev/docs/errors/manifest.protocol_mismatch

PUT jadpo/v1/builds/b_000000000000
  No connection to `127.0.0.1:1`: nothing is listening there.

Differs from the manifest protocol: 1 difference, 1 check, http://127.0.0.1:1
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If the product should follow the protocol**, change what each request
named answers, then check it again:

```sh
jadpo manifest protocol https://collector.example
```

The check passes once every answer is the protocol's, and the run exits 0.

## Related

- [The manifest protocol](../language/16-built-in-services.md#the-manifest-protocol)
- [`jadpo manifest`](../language/14-tooling.md#jadpo-manifest)
