# `translation.group_unknown`

The root file groups its wording by whose text it is: `jadpo` for Jadpo's own messages, or a library package's folder name (`postcodes`) for that package's. A group that names neither words nothing.

```jadpo check project=e-tr-group-unknown file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

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
