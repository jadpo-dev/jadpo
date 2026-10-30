# `deployable.api_unknown`

`serves:` names APIs the app has: those `apis:` declares, or `main`, and
`webhooks` once a connection receives webhooks:

```jadpo check project=e-deployable-api-unknown expect=deployable.api_unknown file=app.jadpo
application {
    deployables: {
        kiosk: {
            serves: [mian]
            target: Target.native
        }
    }
}
```

```text output
error: `mian` isn't one of the app's APIs
 --> app.jadpo:4:22
  |
4 |             serves: [mian]
  |                      ^^^^ no such API
  reason: `serves:` names APIs the app has: `main`. Did you mean `main`?
  next: Use `main` instead of `mian` (agent)
  rule: deployable.api_unknown
  help: https://jadpo.dev/docs/errors/deployable.api_unknown

check failed: 1 error, 1 file
```
