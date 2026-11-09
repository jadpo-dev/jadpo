# `configuration.client_address_hops_invalid`

A `client_address:` that counts proxies has no `hops:`, or one that isn't a count from 1.

## Why Jadpo refuses it

`forwarded_for` and `forwarded` find the client by counting the proxies of
your own deployment from the right of a header, because anything to the
left of them could be written by the caller. A count of 0, a missing count
or a count that could turn out to be 0 would pick an address the caller
chose. So the count is never guessed, and a setting that supplies it has to
declare `min:` of 1 or more, so the app can't start with less.

## What it looks like

```jadpo check project=e-client-address-hops-invalid expect=configuration.client_address_hops_invalid file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.forwarded_for
        hops: 0
    }
}
```

```text output
error: `0` can't be a number of proxies
 --> app.jadpo:4:15
  |
4 |         hops: 0
  |               ^ below 1
  reason: `hops:` is how many proxies of the deployment's own sit in front of the app: a whole number from 1, or an `Int` setting whose type declares `min:` of at least 1.
  next: Write a whole number from 1, such as `hops: 1`, or read an `Int` setting that declares `min: 1` (agent)
  rule: configuration.client_address_hops_invalid
  help: https://jadpo.dev/docs/errors/configuration.client_address_hops_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A source that counts proxies needs the count:

```jadpo check project=e-client-address-hops-missing expect=configuration.client_address_hops_invalid file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.forwarded_for
    }
}
```

```text output
error: `forwarded_for` needs `hops:`
 --> app.jadpo:3:17
  |
3 |         source: ClientAddressSource.forwarded_for
  |                 ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ no `hops:`
  reason: `forwarded_for` counts the proxies of the deployment's own from the right of the header, and the count is never guessed: `hops: 1` for one load balancer in front of the app.
  next: Add `hops: <number of proxies>`, such as `hops: 1` (agent)
  rule: configuration.client_address_hops_invalid
  help: https://jadpo.dev/docs/errors/configuration.client_address_hops_invalid

check failed: 1 error, 1 file
```

A setting has to rule out a count below 1 in its type:

```jadpo check project=e-client-address-hops-setting file=config.jadpo hidden
config {
    proxy_hops: Int {
        default: 1
    }
}
```

```jadpo check project=e-client-address-hops-setting expect=configuration.client_address_hops_invalid file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.forwarded_for
        hops: config.proxy_hops
    }
}
```

```text output
error: `hops:` reads `proxy_hops`, whose type doesn't declare `min: 1`
 --> app.jadpo:4:15
  |
4 |         hops: config.proxy_hops
  |               ^^^^^^^^^^^^^^^^^ `proxy_hops` may be below 1
  reason: A count below 1 would trust no proxy at all, so the setting's type has to rule it out when the app starts: `type <Name> = Int { min: 1 }`.
  next: Give `proxy_hops` a type that declares `min:` of at least 1 (agent)
  rule: configuration.client_address_hops_invalid
  help: https://jadpo.dev/docs/errors/configuration.client_address_hops_invalid

check failed: 1 error, 2 files
```

## How to fix it

**If the number of proxies is the same everywhere**, write it:

```jadpo check project=e-client-address-hops-fixed file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.forwarded_for
        hops: 1
    }
}
```

With one load balancer in front of the app, the client is the last address
in `X-Forwarded-For`.

**If it differs between deployments**, read it from a setting whose type
declares `min:` of 1 or more:

```jadpo check project=e-client-address-hops-read file=domains/edge/types/proxy_hops.jadpo
type ProxyHops = Int {
    min: 1
    max: 8
}
```

```jadpo check project=e-client-address-hops-read file=config.jadpo
use edge/types { ProxyHops }

config {
    proxy_hops: ProxyHops {
        default: 1
    }
}
```

```jadpo check project=e-client-address-hops-read file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.forwarded_for
        hops: config.proxy_hops
    }
}
```

Each deployment now sets its own count, and the app can't start with one
below 1.

## Related

- [`hops:`](../language/10-configuration-and-time.md#hops)
- [Client addresses](../language/10-configuration-and-time.md#client-addresses)
