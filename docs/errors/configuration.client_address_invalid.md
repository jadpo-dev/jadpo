# `configuration.client_address_invalid`

The app's `client_address:` isn't a block of a `source:` and, for some sources, `hops:`.

## Why Jadpo refuses it

`client_address:` says where the client's real address is and how many of
your own proxies to believe, and nothing else. Which proxies to trust is a
count rather than a list of addresses, so it can't go out of date when a
provider's addresses change. Any other shape leaves Jadpo not knowing which
address the rate limits should count.

## What it looks like

```jadpo check project=e-client-address-invalid expect=configuration.client_address_invalid file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.forwarded_for
        hop: 1
    }
}
```

```text output
error: `hop` isn't an option of `client_address:`
 --> app.jadpo:4:9
  |
4 |         hop: 1
  |         ^^^ unknown option
  reason: `client_address:` takes `source:` and `hops:`, and nothing else; which proxies to trust is a count and not a list. Did you mean `hops`?
  next: Use `hops` instead of `hop` (agent)
  rule: configuration.client_address_invalid
  help: https://jadpo.dev/docs/errors/configuration.client_address_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Every `client_address:` names its `source:`:

```jadpo check project=e-client-address-no-source expect=configuration.client_address_invalid file=app.jadpo
application {
    client_address: {
        hops: 1
    }
}
```

```text output
error: `client_address:` has no `source:`
 --> app.jadpo:2:5
  |
2 |     client_address: {
  |     ^^^^^^^^^^^^^^ no `source:`
  reason: The client's address is read from one of four sources: the connection, `X-Forwarded-For`, `Forwarded` or Cloudflare's `CF-Connecting-IP`.
  next: Add `source: ClientAddressSource.<source>` (agent)
  rule: configuration.client_address_invalid
  help: https://jadpo.dev/docs/errors/configuration.client_address_invalid

check failed: 1 error, 1 file
```

And it is a block, even with only a `source:` to give:

```jadpo check project=e-client-address-not-block expect=configuration.client_address_invalid file=app.jadpo
application {
    client_address: ClientAddressSource.connection
}
```

```text output
error: `client_address:` isn't a block
 --> app.jadpo:2:5
  |
2 |     client_address: ClientAddressSource.connection
  |     ^^^^^^^^^^^^^^ a block goes here
  reason: The client's address is read from a `source:` and, for the sources that count proxies in a header, the number of proxies of the deployment's own: `client_address: { source: ClientAddressSource.forwarded_for hops: config.<setting> }`.
  next: Write `client_address: { source: ClientAddressSource.<source> }`, with `hops:` for `forwarded_for` and `forwarded` (agent)
  rule: configuration.client_address_invalid
  help: https://jadpo.dev/docs/errors/configuration.client_address_invalid

check failed: 1 error, 1 file
```

## How to fix it

**If an option is misspelt**, write `source:` and `hops:` as they are
spelt:

```jadpo check project=e-client-address-invalid-option file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.forwarded_for
        hops: 1
    }
}
```

The app now counts one proxy from the right of `X-Forwarded-For`.

**If it has no `source:` or isn't a block**, write the block with its
source:

```jadpo check project=e-client-address-invalid-block file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.connection
    }
}
```

The app now reads the connection's own address, and no header.

## Related

- [Client addresses](../language/10-configuration-and-time.md#client-addresses)
- [`hops:`](../language/10-configuration-and-time.md#hops)
