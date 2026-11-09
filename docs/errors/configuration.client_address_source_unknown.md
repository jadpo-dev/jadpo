# `configuration.client_address_source_unknown`

A `client_address:` names a `source:` that isn't one of the four `ClientAddressSource` values.

## Why Jadpo refuses it

The client's address decides who a rate limit counts, so where it comes
from is a fixed list: the connection, `X-Forwarded-For`, `Forwarded` or
Cloudflare's `CF-Connecting-IP`. A source Jadpo doesn't know is one it
can't read, and falling back to another would count the wrong address.

## What it looks like

```jadpo check project=e-client-address-source-unknown expect=configuration.client_address_source_unknown file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.forward_for
        hops: 1
    }
}
```

```text output
error: `ClientAddressSource.forward_for` isn't a source of the client's address
 --> app.jadpo:3:17
  |
3 |         source: ClientAddressSource.forward_for
  |                 ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not a `ClientAddressSource`
  reason: `ClientAddressSource` has no value called `forward_for`; the four are `connection`, `forwarded_for`, `forwarded`, `cloudflare`. Did you mean `forwarded_for`?
  next: Use `ClientAddressSource.forwarded_for` instead (agent)
  rule: configuration.client_address_source_unknown
  help: https://jadpo.dev/docs/errors/configuration.client_address_source_unknown

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A source written without `ClientAddressSource.` in front is refused too:

```jadpo check project=e-client-address-source-bare expect=configuration.client_address_source_unknown file=app.jadpo
application {
    client_address: {
        source: forwarded_for
        hops: 1
    }
}
```

```text output
error: `forwarded_for` isn't a source of the client's address
 --> app.jadpo:3:17
  |
3 |         source: forwarded_for
  |                 ^^^^^^^^^^^^^ not a `ClientAddressSource`
  reason: `source:` is a `ClientAddressSource`, written `ClientAddressSource.<source>`: `connection`, `forwarded_for`, `forwarded`, `cloudflare`.
  next: Write `source: ClientAddressSource.forwarded_for`, for instance (agent)
  rule: configuration.client_address_source_unknown
  help: https://jadpo.dev/docs/errors/configuration.client_address_source_unknown

check failed: 1 error, 1 file
```

## How to fix it

**If the app is behind proxies of your own**, name the header they write:

```jadpo check project=e-client-address-source-forwarded file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.forwarded_for
        hops: 1
    }
}
```

The client is now the address one proxy from the right of
`X-Forwarded-For`.

**If nothing sits in front of the app**, use the connection's address:

```jadpo check project=e-client-address-source-connection file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.connection
    }
}
```

No header is read, so a caller can't choose its address. It is what a
native app does with no `client_address:` at all.

## Related

- [Client addresses](../language/10-configuration-and-time.md#client-addresses)
