# `semantic.application_section_unknown`

`application { }` holds only the sections the language decides. A
misspelt one is refused, with the edit to the section it meant:

```jadpo check project=e-application-section-near expect=semantic.application_section_unknown
application {
    revocation_dela: 5m
}
```

```text output
error: `revocation_dela` isn't something `application { }` holds
 --> main.jadpo:2:5
  |
2 |     revocation_dela: 5m
  |     ^^^^^^^^^^^^^^^ not a section
  reason: `application { }` holds only the sections the language decides: `jadpo`, `display_name`, `revocation_delay`, `connections`, `deployables`, `apis`, `client_address`, `translations`, `rate_limits`, `flags`, `email`, `events`, `schedules`, `steps`, `receiving` and `ai`. A section it doesn't know would otherwise be dropped without a word. Did you mean `revocation_delay`?
  next: Use `revocation_delay` instead of `revocation_dela` (agent)
  rule: semantic.application_section_unknown
  help: https://jadpo.dev/docs/errors/semantic.application_section_unknown

check failed: 1 error, 1 file
```

A section that is near none of them is removed, or moved where it belongs:

```jadpo check project=e-application-section-unknown expect=semantic.application_section_unknown
application {
    colour_scheme: "dark"
}
```

```text output
error: `colour_scheme` isn't something `application { }` holds
 --> main.jadpo:2:5
  |
2 |     colour_scheme: "dark"
  |     ^^^^^^^^^^^^^ not a section
  reason: `application { }` holds only the sections the language decides: `jadpo`, `display_name`, `revocation_delay`, `connections`, `deployables`, `apis`, `client_address`, `translations`, `rate_limits`, `flags`, `email`, `events`, `schedules`, `steps`, `receiving` and `ai`. A section it doesn't know would otherwise be dropped without a word.
  next: Remove `colour_scheme` from `application { }`, or move it where it belongs (agent)
  rule: semantic.application_section_unknown
  help: https://jadpo.dev/docs/errors/semantic.application_section_unknown

check failed: 1 error, 1 file
```
