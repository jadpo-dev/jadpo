# `configuration.default_literal_required`

A setting's `default:` or `test:` value isn't a literal.

## Why Jadpo refuses it

A `default:` is checked while compiling and read when the app starts, so it
has to be a literal of the setting's type: nothing runs before the
application starts that could work one out. A constant isn't allowed
either, because a value is a constant or a setting, never both. Code that
read the constant would quietly ignore what a deployment set.

## What it looks like

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Here `largest_page` names nothing. A constant of the setting's type is
refused the same way, and the `next:` offers to make the constant the
setting:

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

## How to fix it

**If the value is fixed**, write it after `default:`:

```jadpo check project=e-default-literal-value
config {
    page_size: Int {
        default: 25
    }
}
```

A deployment that doesn't set `PAGE_SIZE` now starts with 25.

**If a constant should differ per deployment**, make it the setting, with
the command the error names:

```sh
jadpo config move largest_page
```

Its value becomes the `default:`, and every read of it reads
`config.page_size`.

## Related

- [Defaults and test values](../language/10-configuration-and-time.md#defaults-and-test-values)
- [A constant or a setting](../language/10-configuration-and-time.md#a-constant-or-a-setting)
- [Making a constant a setting](../language/10-configuration-and-time.md#making-a-constant-a-setting)
- [`jadpo config move`](../language/14-tooling.md#jadpo-config-move)
