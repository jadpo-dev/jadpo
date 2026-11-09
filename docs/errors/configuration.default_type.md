# `configuration.default_type`

A setting's `default:` or `test:` value, or a test's `config:` value, is the wrong kind of value for its type.

## Why Jadpo refuses it

A value written in source is checked while compiling, so a mistake here
doesn't wait for a deployment to find it. The value has to be a literal of
the setting's own type: code that reads the setting relies on what the type
says it holds.

## What it looks like

```jadpo check project=e-default-type expect=configuration.default_type
config {
    page_size: Int {
        default: "many"
    }
}
```

```text output
error: `"many"` can't be the default of `page_size`
 --> main.jadpo:3:18
  |
3 |         default: "many"
  |                  ^^^^^^ the wrong kind of value
  reason: `page_size` is `Int`, and this is a different kind of value; the default must be a literal of the setting's own type.
  next: Write a `Int` value after `default:` (agent)
  rule: configuration.default_type
  help: https://jadpo.dev/docs/errors/configuration.default_type

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A variant of another enum is the wrong kind of value too:

```jadpo check project=e-default-type-variant expect=configuration.default_type
type Verbosity = Enum {
    quiet
    normal
}

type Tone = Enum {
    plain
    warm
}

config {
    verbosity: Verbosity {
        default: Tone.plain
    }
}
```

```text output
error: `Tone.plain` can't be the default of `verbosity`
 --> main.jadpo:13:18
   |
13 |         default: Tone.plain
   |                  ^^^^^^^^^^ a different type
  reason: `verbosity` is `Verbosity`, and this is a variant of `Tone`; the default must be a variant of the setting's own type.
  next: Write a `Verbosity` variant, such as `Verbosity.quiet` (agent)
  rule: configuration.default_type
  help: https://jadpo.dev/docs/errors/configuration.default_type

check failed: 1 error, 1 file
```

## How to fix it

**If the value is wrong**, write one of the setting's type:

```jadpo check project=e-default-type-value
config {
    page_size: Int {
        default: 25
    }
}
```

A deployment that doesn't set `PAGE_SIZE` now starts with 25.

**If the type is wrong**, change the setting to the type of its value:

```jadpo check project=e-default-type-type
config {
    page_size: Text {
        default: "many"
    }
}
```

`config.page_size` now reads as text wherever code uses it.

## Related

- [Defaults and test values](../language/10-configuration-and-time.md#defaults-and-test-values)
- [What a setting can hold](../language/10-configuration-and-time.md#what-a-setting-can-hold)
