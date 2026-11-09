# `deployable.api_unknown`

A deployable's `serves:` names an API the app doesn't have.

## Why Jadpo refuses it

`serves:` decides which routes a deployable answers, so a name that isn't
an API would answer nothing, and the routes you meant would go unserved.
Jadpo can only match a name against the APIs the app has: those `apis:`
declares, `main`, and `webhooks` once a connection receives webhooks.

## What it looks like

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the name is misspelt**, write it as the app does:

```jadpo check project=e-deployable-api-spelt file=app.jadpo
application {
    deployables: {
        kiosk: {
            serves: [main]
            target: Target.native
        }
    }
}
```

`kiosk` now answers the routes of `main`, and its `openapi.json`.

**If you meant an API the app doesn't declare yet**, declare it in `apis:`:

```jadpo check project=e-deployable-api-new file=app.jadpo
application {
    apis: {
        main: {
            prefix: /
        }
        backstage: {
            prefix: /backstage
        }
    }
    deployables: {
        kiosk: {
            serves: [main, backstage]
            target: Target.native
        }
    }
}
```

The routes of `backstage` are now answered under `/backstage`, by `kiosk`.

## Related

- [Declared deployables](../language/14-tooling.md#declared-deployables)
