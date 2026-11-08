# `publish.type_unreviewed`

A provider package's public type isn't in its `reviewed:` list.

## Why Jadpo refuses it

A provider package describes someone else's API, and only that API's
documentation says what each field really is: whether a number is pence or
seconds, whether text is an email address or a message a person wrote.
`package.standard_candidate` catches the fields whose names give them away,
but names can't tell it everything. So before a provider package is
published, a coding agent goes through each public type against the
provider's documentation, and lists each one it has gone through in
`reviewed:` in `provider.jadpo`. Any public type can reach an app that
installs the package, so the publishing bar holds every one of them to it.

Jadpo can't do the review for you, because it hasn't read the provider's
documentation.

## What it looks like

A courier's package, written by hand, lists `Delivery` as reviewed but not
`DeliveryQuery`:

```text
$ jadpo package check
error: Public type `DeliveryQuery` isn't reviewed
 --> types/delivery_query.jadpo:2:13
  |
2 | public type DeliveryQuery = Object {
  |             ^^^^^^^^^^^^^ not in `reviewed:`
  reason: A provider package is published once an agent has gone through each of its public types against the provider's documentation: what each field is, which standard type fits it, and which hold personal data, which names alone can't tell. `reviewed:` records each type gone through.
  next: Go through `DeliveryQuery` against the provider's documentation, then add it to `reviewed:` in `provider.jadpo` (agent)
  rule: publish.type_unreviewed
  help: https://jadpo.dev/docs/errors/publish.type_unreviewed

Publishing bar
  compiles      passed                        no errors
  described     passed                        every public declaration
  reviewed      failed                        1 not in `reviewed:`
  examples      passed                        no functions to show
  tests         passed                        no tests
  version       passed                        1.0.0, the first release
  changelog     passed                        1.0.0 says why
  publisher     passed                        @acme
  licence       passed                        mit, from Jadpo 1.0

package check failed: 1 failed, 8 passed
```

`jadpo package verify --json`, which the registry runs, gives the row's
whole detail, so the page of a refused release names each type and the step:
"`DeliveryQuery` isn't reviewed: go through it against the provider's
documentation, then add it to `reviewed:` in `provider.jadpo`".

`(agent)` at the end of the `next:` line means your coding agent can make the
fix itself.

## How to fix it

**If the package was written by hand**, have your agent read the type's fields
against the provider's documentation, use the standard type wherever one fits,
mark free text `personal:`, then add the type to `reviewed:` in
`provider { }`:

```text
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
    }
    credential: CredentialScheme.bearer
    reviewed: [Delivery, DeliveryQuery]
}
```

The `reviewed` row passes once every public type is listed.

**If the package was imported**, the list is `imported:`'s own `reviewed:`,
beside the description it was imported from:

```text
provider {
    ...
    imported: {
        file: "acme-couriers.json"
        digest: "sha256:4f1c"
        reviewed: [Delivery, DeliveryQuery]
    }
}
```

Importing the description again keeps the list, and the `reviewed` row passes
once every public type is in it.

## Related

- [The scoped review and unconfirmed operations](../language/11-integrations.md#the-scoped-review-and-unconfirmed-operations)
- [What publishing requires](../language/11-integrations.md#what-publishing-requires)
- [`package.standard_candidate`](package.standard_candidate.md)
