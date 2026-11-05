# `toolchain.not_built_yet`

The language declares some operations that aren't built yet, such as the
generated `oauth.approve` (area 8). Calling one is an error rather than a
program that can't run:

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

A decided feature that isn't built can also be refused where its effect would
be wrong. A `File` in a route's `output:` or a failure's `client:` is refused,
because downloads aren't built and the answer would hold the store and key the
file is kept under ([What a `File` holds](../language/16-built-in-services.md#what-a-file-holds)).
A `File` in a provider operation's `input:` or a webhook's `reply:` is refused
for the same reason, since sending a file to a provider isn't built. A
`FileContents` in a route's `output:` or a failure's `client:` is refused too,
since the answer would hold the whole file as Base64.

An email's attachments are its `File` fields, one file each, so a `List<File>`,
a `Set<File>` or a `Map` of them and a `FileContents` on an email or a partial
are refused, as a `File` on a partial is, since each would compile and never be
sent ([Attachments](../language/16-built-in-services.md#attachments)).
