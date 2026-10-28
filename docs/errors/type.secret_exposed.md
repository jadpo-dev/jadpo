# `type.secret_exposed`

A secret value ("Secret values") is never sent, logged,
stored or shown: an `Output`, a failure's fields, an event, an email, a
message and an entity refuse one, however deep, and so does any
operation, operator or construction but the one made to take it. It
arrives in an `Input`. A client that needs to know whether a key is set is
sent that instead.

```jadpo check project=e-secret-exposed expect=type.secret_exposed
type PartnerName = Text {
    max_length: 100
}

/// What the partners page shows of a partner.
type PartnerProfile = Output {
    name: PartnerName
    partner_key: ApiKey
}
```

```text output
error: `PartnerProfile.partner_key` holds an `ApiKey`
 --> main.jadpo:8:18
  |
8 |     partner_key: ApiKey
  |                  ^^^^^^ sent to a client
  reason: An `ApiKey` is a secret value: it is never sent, logged, stored or shown. A response is written out, logged by proxies and kept by browsers, and a client never needs a secret back.
  next: Remove `partner_key`, and send what the client needs instead, such as whether one is set (agent)
  rule: type.secret_exposed
  help: docs/errors/type.secret_exposed.md

check failed: 1 error, 1 file
```
