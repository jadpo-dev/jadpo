# `configuration.test_value_missing`

A test reaches code that reads a setting, and the setting has no value for tests to use.

## Why Jadpo refuses it

A test never reads your machine's environment, so it gives the same
answer on every machine. Each setting gets its value in a test from
`config.jadpo` instead: its `test:` value, or else its `default:`. A
setting with neither has nothing for the test to read, so the test could
not run.

Only a secret is different. A test run makes up a throwaway value for
each secret, because secrets never appear in test code.

## What it looks like

main.jadpo:

```jadpo check project=e-test-value-missing expect=configuration.test_value_missing
type Greeting = Text {
    max_length: 80
}

config {
    greeting: Greeting
}

function hello() -> Greeting {
    return config.greeting
}

test "the greeting is read" {
    assert hello() == "hello"
}
```

```text output
error: `config.greeting` has no value in tests
 --> main.jadpo:10:12
   |
10 |     return config.greeting
   |            ^^^^^^^^^^^^^^^ no `test:` or `default:`
  reason: The test "the greeting is read" reaches this read. A test reads each setting's `test:` value, or its `default:`, and `greeting` has neither; only a secret is made up for a run, and `greeting` isn't one.
  next: Give `greeting` a `test:` value, or a `default:`, in `config.jadpo` (agent)
  rule: configuration.test_value_missing
  help: https://jadpo.dev/docs/errors/configuration.test_value_missing

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If every test may read the same value**, give the setting a `test:`
value:

```jadpo fragment
    greeting: Greeting {
        test: "hello"
    }
```

```jadpo check project=e-test-value-missing-test hidden
type Greeting = Text {
    max_length: 80
}

config {
    greeting: Greeting {
        test: "hello"
    }
}

function hello() -> Greeting {
    return config.greeting
}

test "the greeting is read" {
    assert hello() == "hello"
}
```

Every test now reads `"hello"` for `config.greeting`. Production still
needs the value set, because a `test:` value is read only by tests.

**If the app should fall back to a value when none is set**, give the
setting a `default:`:

```jadpo fragment
    greeting: Greeting {
        default: "hello"
    }
```

```jadpo check project=e-test-value-missing-default hidden
type Greeting = Text {
    max_length: 80
}

config {
    greeting: Greeting {
        default: "hello"
    }
}

function hello() -> Greeting {
    return config.greeting
}

test "the greeting is read" {
    assert hello() == "hello"
}
```

Tests read `"hello"`, and so does every deployment that doesn't set the
setting.

A single test can instead give the value in its own `config:` block
([Configuration in tests](../language/13-testing.md#configuration-in-tests)).
Every other test that reaches `config.greeting` then needs one too, so a
`test:` value is usually the simpler fix.

## Related

- [Defaults and test values](../language/10-configuration-and-time.md#defaults-and-test-values)
- [Configuration in tests](../language/13-testing.md#configuration-in-tests)
- [Secrets](../language/10-configuration-and-time.md#secrets)
