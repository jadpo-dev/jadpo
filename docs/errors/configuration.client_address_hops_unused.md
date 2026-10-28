# `configuration.client_address_hops_unused`

`cloudflare` and `connection` count no proxies, so `hops:` means nothing with them:

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
  help: docs/language/10-configuration-and-time.html#configuration-client-address-hops-unused

check failed: 1 error, 1 file
```
