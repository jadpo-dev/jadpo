# `configuration.client_address_hops_unused`

A `client_address:` gives `hops:` to a source that counts no proxies.

## Why Jadpo refuses it

`hops:` says how far to count along a list of addresses in a header.
`cloudflare` reads one address that Cloudflare writes itself, and
`connection` reads the connection's own address and no header, so neither
has a list to count along. A count there would suggest it changes which
address is believed, and it wouldn't.

## What it looks like

```jadpo check project=e-client-address-hops-unused expect=configuration.client_address_hops_unused file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.cloudflare
        hops: 1
    }
}
```

```text output
error: `hops:` means nothing with `cloudflare`
 --> app.jadpo:4:15
  |
4 |         hops: 1
  |               ^ no proxies are counted
  reason: Cloudflare sets `CF-Connecting-IP` on every request from its edge and overwrites a caller's, so there is no list to count from the right.
  next: Remove `hops:` (agent)
  rule: configuration.client_address_hops_unused
  help: https://jadpo.dev/docs/errors/configuration.client_address_hops_unused

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`connection` is refused the same way, since it reads no header:

```jadpo check project=e-client-address-hops-unused-connection expect=configuration.client_address_hops_unused file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.connection
        hops: 1
    }
}
```

```text output
error: `hops:` means nothing with `connection`
 --> app.jadpo:4:15
  |
4 |         hops: 1
  |               ^ no proxies are counted
  reason: `connection` reads the connection's own address and no header, so there is nothing to count.
  next: Remove `hops:` (agent)
  rule: configuration.client_address_hops_unused
  help: https://jadpo.dev/docs/errors/configuration.client_address_hops_unused

check failed: 1 error, 1 file
```

## How to fix it

**If the source is the right one**, remove `hops:`:

```jadpo check project=e-client-address-hops-unused-remove file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.cloudflare
    }
}
```

The address now comes from `CF-Connecting-IP`, with nothing to count.

**If your own proxies add the address to a header**, use the source that
reads that header, which does count them:

```jadpo check project=e-client-address-hops-unused-forwarded file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.forwarded_for
        hops: 1
    }
}
```

The client is now the address one proxy from the right of
`X-Forwarded-For`.

## Related

- [`hops:`](../language/10-configuration-and-time.md#hops)
- [Client addresses](../language/10-configuration-and-time.md#client-addresses)
