# `type.locale_duplicate`

A locale is listed twice in `locales { }`, in `supported:` or in `in_progress:`.

## Why Jadpo refuses it

A locale is listed or it isn't, so a second mention says nothing more. It
is usually a slip for another locale, such as `Locale.en_gb` written
where `Locale.en_us` was meant, which Jadpo can't guess.

## What it looks like

app.jadpo:

```jadpo check project=e-locale-duplicate file=app.jadpo expect=type.locale_duplicate
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.en_gb]
}
```

```text output
error: `Locale.en_gb` is listed twice
 --> app.jadpo:5:31
  |
5 |     supported: [Locale.en_gb, Locale.en_gb]
  |                               ^^^^^^^^^^^^ listed again
  |                 ------------ first listed here
  reason: A locale is listed or it is not; listing it twice says nothing more.
  next: Remove the second `Locale.en_gb` (agent)
  rule: type.locale_duplicate
  help: https://jadpo.dev/docs/errors/type.locale_duplicate

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether the second mention was meant to be another
locale.

**If it was the same locale**, remove the second one:

```jadpo fragment
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

```jadpo check project=e-locale-duplicate-fix-1 file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

The app supports British English alone.

**If you meant another locale**, write that one:

```jadpo fragment
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.en_us]
}
```

```jadpo check project=e-locale-duplicate-fix-2 file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.en_us]
}
```

The app supports American English as well, so its messages need American
wording.

## Related

- [Locales](../language/10-configuration-and-time.md#locales)
