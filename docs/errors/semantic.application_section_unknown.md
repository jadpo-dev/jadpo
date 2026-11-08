# `semantic.application_section_unknown`

An `application { }` block has a section Jadpo doesn't know.

## Why Jadpo refuses it

`application { }` holds only the sections the language decides. A section
Jadpo doesn't know would otherwise be dropped without a word, and a
misspelt setting would look as if it had been applied.

## What it looks like

A misspelt section is refused, with the edit to the section it meant:

main.jadpo:

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
  reason: `application { }` holds only the sections the language decides: `jadpo`, `display_name`, `revocation_delay`, `connections`, `packages`, `deployables`, `apis`, `client_address`, `translations`, `rate_limits`, `flags`, `email`, `events`, `schedules`, `steps`, `receiving`, `ai`, `field_encryption_key` and `privacy`. A section it doesn't know would otherwise be dropped without a word. Did you mean `revocation_delay`?
  next: Use `revocation_delay` instead of `revocation_dela` (agent)
  rule: semantic.application_section_unknown
  help: https://jadpo.dev/docs/errors/semantic.application_section_unknown

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A section that is close to none of them comes with no edit:

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
  reason: `application { }` holds only the sections the language decides: `jadpo`, `display_name`, `revocation_delay`, `connections`, `packages`, `deployables`, `apis`, `client_address`, `translations`, `rate_limits`, `flags`, `email`, `events`, `schedules`, `steps`, `receiving`, `ai`, `field_encryption_key` and `privacy`. A section it doesn't know would otherwise be dropped without a word.
  next: Remove `colour_scheme` from `application { }`, or move it where it belongs (agent)
  rule: semantic.application_section_unknown
  help: https://jadpo.dev/docs/errors/semantic.application_section_unknown

check failed: 1 error, 1 file
```

## How to fix it

**If the name is misspelt**, write the section it meant:

```jadpo check project=e-application-section-fix-1
application {
    revocation_delay: 5m
}
```

Jadpo now applies the setting.

**If the section doesn't belong in `application { }`**, remove it, or move
it where it belongs:

```jadpo check project=e-application-section-fix-2
application {}
```

The application now has only what the language decides.

## Related

- [What `application { }` holds](../language/01-project-layout-and-modules.md#what-application---holds)
