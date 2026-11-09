# `configuration.client_address_duplicate`

The app declares `client_address:` twice.

## Why Jadpo refuses it

Every request is counted by one rule: rate limits and the request timeline
use the address `client_address:` chooses. Two declarations could choose
two different addresses for the same request, and Jadpo can't tell which
one describes your deployment.

## What it looks like

```jadpo check project=e-client-address-duplicate expect=configuration.client_address_duplicate file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.connection
    }
    client_address: {
        source: ClientAddressSource.cloudflare
    }
}
```

```text output
error: `client_address:` is declared twice
 --> app.jadpo:5:5
  |
2 |     client_address: {
  |     -------------- first declared here
5 |     client_address: {
  |     ^^^^^^^^^^^^^^ declared again
  reason: The client's address is read one way for the whole application, so every request is counted by the same rule.
  next: Keep one `client_address:` and remove the other (agent)
  rule: configuration.client_address_duplicate
  help: https://jadpo.dev/docs/errors/configuration.client_address_duplicate

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If one of them describes how the app is deployed**, keep that one and
remove the other:

```jadpo check project=e-client-address-duplicate-fix file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.cloudflare
    }
}
```

Every request now reads the client's address from `CF-Connecting-IP`.

## Related

- [Client addresses](../language/10-configuration-and-time.md#client-addresses)
- [Rate limits](../language/16-built-in-services.md#rate-limits)
