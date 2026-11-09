# `configuration.secret_default`

A secret setting has a value in source, in its `default:`, its `test:` or a test's `config:`.

## Why Jadpo refuses it

A secret's value comes only from the deployment's environment, so a
forgotten production secret stops the app instead of falling back to a
value written in source. A test run makes its own throwaway value for each
secret, so a test doesn't need one in source either.

## What it looks like

```jadpo check project=e-secret-default expect=configuration.secret_default
type PartnerKey = Text {
    min_length: 8
}

config {
    api_key: PartnerKey {
        secret: true
        default: "development-key"
    }
}
```

```text output
error: `api_key` is secret, and has a `default:` in source
 --> main.jadpo:8:9
  |
8 |         default: "development-key"
  |         ^^^^^^^ a secret written in source
  reason: A secret's value comes only from the deployment's environment, so a forgotten production secret stops the app instead of falling back to a value written in source; `jadpo test` generates its own for a test run, so it needs no `default:` either.
  next: Remove `default:` from `api_key` (agent)
  rule: configuration.secret_default
  help: https://jadpo.dev/docs/errors/configuration.secret_default

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A test's `config:` is refused the same way:

```jadpo check project=e-secret-default-test expect=configuration.secret_default
type PartnerKey = Text {
    min_length: 8
}

config {
    api_key: PartnerKey {
        secret: true
    }
}

test "the key is given" {
    config: {
        api_key: "development-key"
    }
    assert 1 == 1
}
```

```text output
error: `api_key` is secret, and a test gives it a value
 --> main.jadpo:13:9
   |
13 |         api_key: "development-key"
   |         ^^^^^^^ a secret written in source
  reason: Secrets never appear in test source: every test run makes its own throwaway value, which is enough because a secret can only reach the places the compiler owns.
  next: Remove `api_key:` from `config:` (agent)
  rule: configuration.secret_default
  help: https://jadpo.dev/docs/errors/configuration.secret_default

check failed: 1 error, 1 file
```

## How to fix it

**If the value is a secret**, remove the value from source:

```jadpo check project=e-secret-default-removed
type PartnerKey = Text {
    min_length: 8
}

config {
    api_key: PartnerKey {
        secret: true
    }
}
```

The app won't start until the deployment sets `API_KEY`. On your own
machine, `jadpo config set api_key` keeps the value in `.env.local`.

**If it isn't a secret**, leave out `secret: true` and keep the value:

```jadpo check project=e-secret-default-open
type PartnerKey = Text {
    min_length: 8
}

config {
    api_key: PartnerKey {
        default: "development-key"
    }
}
```

The value can now be read anywhere and appears in source, so only do this
for something that is safe to publish.

## Related

- [Secrets](../language/10-configuration-and-time.md#secrets)
- [Defaults and test values](../language/10-configuration-and-time.md#defaults-and-test-values)
- [Setting a value: `jadpo config set`](../language/10-configuration-and-time.md#setting-a-value-jadpo-config-set)
