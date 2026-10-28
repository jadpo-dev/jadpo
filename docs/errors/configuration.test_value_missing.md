# `configuration.test_value_missing`

A test reads each setting's `test:` value, or its default; this one has neither:

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
  help: docs/language/10-configuration-and-time.html#configuration-test-value-missing

check failed: 1 error, 1 file
```
