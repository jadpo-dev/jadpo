# `translation.group_unknown`

A group in the root language file is not one Jadpo reads.

## Why Jadpo refuses it

The root language file words text your application didn't write, grouped by
whose text it is: `jadpo` for Jadpo's own messages, or a library package's
folder name for that package's. A group that names neither has nothing to
word, so whatever it holds would be ignored without anyone noticing.

## What it looks like

The group is meant to be `jadpo`, with a letter missing.

```jadpo check project=e-tr-group-unknown file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

`translations/en_gb.jadpo`:

```jadpo check project=e-tr-group-unknown file=translations/en_gb.jadpo expect=translation.group_unknown
translations {
    jadp: {
        text_direction: {
            left_to_right: "ltr"
            right_to_left: "rtl"
        }
    }
}
```

```text output
error: `jadp` isn't a group Jadpo reads
 --> translations/en_gb.jadpo:2:5
  |
2 |     jadp: {
  |     ^^^^ no such group
  reason: The root file words the text the application did not write, grouped by whose it is: `jadpo` for Jadpo's own messages, or the folder name of a library package the application uses for that package's. Did you mean `jadpo`?
  next: Rename the group to `jadpo` (agent)
  rule: translation.group_unknown
  help: https://jadpo.dev/docs/errors/translation.group_unknown

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A package held in two versions at once is worded under each version's
own key, so a group that uses the plain folder name then, or a version key
while only one version is held, is refused too. The error says which key
to use.

## How to fix it

**If the group's name is misspelt**, write the name it means. In
`translations/en_gb.jadpo`:

```jadpo check project=e-tr-group-unknown-fix file=translations/en_gb.jadpo
translations {
    jadpo: {
        text_direction: {
            left_to_right: "ltr"
            right_to_left: "rtl"
        }
    }
}
```

```jadpo check project=e-tr-group-unknown-fix file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

The members under it now word Jadpo's own messages.

**If it is for a package you don't use**, remove the group, or install the
package it belongs to first. With the group removed, the file holds
nothing for that package:

```jadpo check project=e-tr-group-unknown-remove file=translations/en_gb.jadpo
translations {}
```

```jadpo check project=e-tr-group-unknown-remove file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

Nothing in the file words text the application doesn't have.

## Related

- [The root wording file](../language/10-configuration-and-time.md#the-root-wording-file)
- [Packages' text in an app](../language/10-configuration-and-time.md#packages-text-in-an-app)
- [Two versions of a package](../language/10-configuration-and-time.md#two-versions-of-a-package)
