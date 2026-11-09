# `toolchain.not_built_yet`

Code uses something the language declares and this compiler can't run yet.

## Why Jadpo refuses it

The language decides some things ahead of what this compiler can run, such
as the generated `oauth.approve`. Compiling a call to one would give you a
program that can't run, or one that quietly does the wrong thing. So it's
an error here, where you can still choose to do without it.

## What it looks like

```jadpo check project=e-not-built-yet expect=toolchain.not_built_yet
type ConsentId = Uuid {}

action approve_consent(consent: ConsentId) -> Unit {
    oauth.approve(consent)
}
```

```text output
error: `oauth.approve` isn't built yet
 --> main.jadpo:4:5
  |
4 |     oauth.approve(consent)
  |     ^^^^^^^^^^^^^ declared, not built
  reason: The language declares `oauth.approve`, but this compiler has nothing to run it with yet, so a program that calls it couldn't run.
  next: Do without `oauth.approve` for now (agent)
  rule: toolchain.not_built_yet
  help: https://jadpo.dev/docs/errors/toolchain.not_built_yet

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Something this compiler can't do yet can also be refused where its effect
would be wrong. A `File` in a provider operation's `input:` or a webhook's
`reply:` is refused, since the provider would be told where the file is
kept and be given none of it
([What a `File` holds](../language/16-built-in-services.md#what-a-file-holds)).
A `FileContents` in a route's `output:` or a failure's `client:` is refused
too, since the answer would hold the whole file as Base64.

A slot for a `File` field over 100 MB is refused where it is asked for,
since such a field gets a slot in parts
([Large uploads go in parts](../language/16-built-in-services.md#large-uploads-go-in-parts)).

An email's attachments are its `File` fields, one file each, so a
`List<File>`, a `Set<File>` or a `Map` of them and a `FileContents` on an
email or a partial are refused, as a `File` on a partial is, since each
would compile and never be sent
([Attachments](../language/16-built-in-services.md#attachments)).

## How to fix it

**If the app can do without it for now**, take the call out:

```jadpo check project=e-not-built-yet-fix
type ConsentId = Uuid {}
```

The project compiles and runs without that step. Add it back when a
release of Jadpo can run it.

## Related

- [What is built](../language/01-project-layout-and-modules.md#what-is-built)
