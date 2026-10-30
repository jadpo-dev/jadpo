# `configuration.default_literal_required`

```jadpo check project=e-default-literal expect=configuration.default_literal_required
config {
    page_size: Int {
        default: largest_page
    }
}
```

```text output
error: `default:` on `page_size` isn't a literal
 --> main.jadpo:3:18
  |
3 |         default: largest_page
  |                  ^^^^^^^^^^^^ not a literal
  reason: A `default:` is a literal of the setting's type, checked while compiling (`default: 25`, `default: 5s`); it can't be computed, because nothing runs before the application starts.
  next: Write a literal value after `default:`, or remove it (agent)
  rule: configuration.default_literal_required
  help: https://jadpo.dev/docs/errors/configuration.default_literal_required

check failed: 1 error, 1 file
```

Here `largest_page` names nothing. A constant of the setting's type is
refused the same way: a value is a constant or a setting, never both
([A constant or a setting](../language/10-configuration-and-time.md#a-constant-or-a-setting)). For a constant,
the `next:` offers `jadpo config move`, which makes the constant this
setting: its value becomes the `default:` and every read of it reads
`config.page_size`:

```jadpo check project=e-default-constant expect=configuration.default_literal_required
type RowLimit = Int {
    min: 0
}

constant largest_page: RowLimit = 100

config {
    page_size: RowLimit {
        default: largest_page
    }
}

function page() -> RowLimit {
    return RowLimit(config.page_size + largest_page)
}
```

```text output
error: `default:` on `page_size` isn't a literal
 --> main.jadpo:9:18
  |
9 |         default: largest_page
  |                  ^^^^^^^^^^^^ not a literal
  reason: A `default:` is a literal of the setting's type, checked while compiling (`default: 25`, `default: 5s`); it can't be computed, because nothing runs before the application starts.
  next: Run `jadpo config move largest_page`, which makes the constant this setting, or write a literal value after `default:` (agent)
  rule: configuration.default_literal_required
  help: https://jadpo.dev/docs/errors/configuration.default_literal_required

check failed: 1 error, 1 file
```
