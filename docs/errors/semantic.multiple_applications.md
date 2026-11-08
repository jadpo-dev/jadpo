# `semantic.multiple_applications`

A project declares more than one `application { }`.

## Why Jadpo refuses it

A project is one application, and every setting it has lives in one block,
so there is one place to read them. Jadpo can't tell which of two blocks
you mean, so it won't merge them.

## What it looks like

main.jadpo:

```jadpo check project=e-multiple-applications expect=semantic.multiple_applications
application {}

application {
    revocation_delay: 5m
}
```

```text output
error: `application { }` is declared twice
 --> main.jadpo:3:1
  |
1 | application {}
  | ----------- first declared here
3 | application {
  | ^^^^^^^^^^^ declared again
  reason: A project is one application, declared once in `app.jadpo`; its settings all go in that one block.
  next: Move what this `application` holds into the first, and remove it (agent)
  rule: semantic.multiple_applications
  help: https://jadpo.dev/docs/errors/semantic.multiple_applications

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If both blocks hold settings you want**, move them into one:

```jadpo check project=e-multiple-applications-fix
application {
    revocation_delay: 5m
}
```

The project now has one `application { }`, with every setting in it.

## Related

- [What `application { }` holds](../language/01-project-layout-and-modules.md#what-application---holds)
