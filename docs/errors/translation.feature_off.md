# `translation.feature_off`

Jadpo writes the messages of a feature only for an application that has it, so wording one of a feature that is off changes nothing and would hide a mistake. Here the application signs no one in with magic links.

```jadpo check project=e-tr-feature-off file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

```jadpo check project=e-tr-feature-off file=translations/en_gb.jadpo expect=translation.feature_off
translations {
    jadpo: {
        magic_link_email: "Your link to sign in"
    }
}
```

```text output
error: `magic_link_email` is a message of a feature this application has off
 --> translations/en_gb.jadpo:3:9
  |
3 |         magic_link_email: "Your link to sign in"
  |         ^^^^^^^^^^^^^^^^ a feature this application has off
  reason: `magic_link_email` is part of Jadpo's magic links text, which Jadpo writes when the application signs users in with magic links. This application doesn't, so wording it changes nothing and would hide a mistake.
  next: Remove `magic_link_email` from `translations/en_gb.jadpo`, or switch the feature on (agent)
  rule: translation.feature_off
  help: https://jadpo.dev/docs/errors/translation.feature_off

check failed: 1 error, 2 files
```
