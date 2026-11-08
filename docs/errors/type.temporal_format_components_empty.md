# `type.temporal_format_components_empty`

A declared `TimeFormat` lists no parts to show.

## Why Jadpo refuses it

A declared format lists the parts of a date or a time it shows, each with
a `ComponentStyle`, and the locale arranges them. With none listed there
is nothing to show.

## What it looks like

main.jadpo:

```jadpo check project=e-components-empty expect=type.temporal_format_components_empty
type Blank = TimeFormat {}
```

```text output
error: `Blank` shows nothing
 --> main.jadpo:1:6
  |
1 | type Blank = TimeFormat {}
  |      ^^^^^ no parts
  reason: A format lists the parts of a date or a time it shows, each with a `ComponentStyle`, and the locale arranges them; with none listed there is nothing to show.
  next: List the parts to show, such as `day: ComponentStyle.numeric`, or use a built-in format such as `TimeFormat.date_long` (agent)
  rule: type.temporal_format_components_empty
  help: https://jadpo.dev/docs/errors/type.temporal_format_components_empty

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether a built-in format already shows what you want.

**If you want particular parts**, list them:

```jadpo fragment
type Blank = TimeFormat {
    day: ComponentStyle.numeric
    month: ComponentStyle.long
}
```

```jadpo check project=e-components-empty-fix-1 hidden
type Blank = TimeFormat {
    day: ComponentStyle.numeric
    month: ComponentStyle.long
}
```

The format now shows the day and the month's name, in the order the locale
puts them.

**If a built-in format is what you want**, remove the declaration and name
the built-in one where you format the value:

```jadpo check project=e-components-empty-fix-2
type InvoiceDate = CalendarDate {}

function invoice_dated(day: InvoiceDate, reader_locale: Locale) -> PresentationText {
    return text.format(day, locale: reader_locale, format: TimeFormat.date_long)
}
```

The date shows in the locale's long form, as "27 September 2026" in
`en_gb`.

## Related

- [Declared formats](../language/10-configuration-and-time.md#declared-formats)
- [Built-in formats](../language/10-configuration-and-time.md#built-in-formats)
