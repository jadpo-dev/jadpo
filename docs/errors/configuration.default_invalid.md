# `configuration.default_invalid`

A PostgreSQL address is not a `Url`:

```jadpo check project=e-default-invalid expect=configuration.default_invalid
type PrimaryDatabase = Url {}

config {
    primary_database: PrimaryDatabase {
        default: "postgres://localhost:5432/todo"
    }
}
```

```text output
error: `"postgres://localhost:5432/todo"` isn't a valid `PrimaryDatabase`
 --> main.jadpo:5:18
  |
5 |         default: "postgres://localhost:5432/todo"
  |                  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not a valid value
  reason: The value isn't an `http://` or `https://` URL with a host, such as `https://example.com`; a default is checked while compiling, with the rules of the setting's type.
  next: Write a valid `PrimaryDatabase` value (agent)
  rule: configuration.default_invalid
  help: docs/errors/configuration.default_invalid.md

check failed: 1 error, 1 file
```
