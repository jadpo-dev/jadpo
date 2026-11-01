# `toolchain.source_read_failed`

A `.jadpo` file in the project couldn't be read, because it isn't UTF-8 text or you aren't allowed to read it.

## Why Jadpo refuses it

Jadpo checks the whole project at once, since a declaration in one file
can be used in any other. A file it can't read could hold any of them,
so checking without it would report errors that aren't there, or miss
ones that are. It names the file and why reading failed, and checks
nothing else until it can.

Source is read as UTF-8 text. Jadpo won't convert a file saved in
another encoding, since doing so means guessing which one it was.

## What it looks like

An editor saved `app.jadpo` as Latin-1, so the `é` in its comment is
one byte that isn't UTF-8:

```jadpo check project=e-source-read file=app.jadpo hidden
// The café's ordering app.
application {}
```

```sh run project=e-source-read expect=toolchain.source_read_failed
iconv -f UTF-8 -t LATIN1 app.jadpo > app.latin1
mv app.latin1 app.jadpo
jadpo check
```

```text output
error: `app.jadpo` couldn't be read
 --> app.jadpo
  reason: Reading it failed: stream did not contain valid UTF-8.
  next: Check it can be read, then run `jadpo check` again (human)
  rule: toolchain.source_read_failed
  help: https://jadpo.dev/docs/errors/toolchain.source_read_failed

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides how a file you saved is read.

A file you aren't allowed to read is refused the same way, with
`Permission denied` as the reason.

## How to fix it

**If you know the encoding it was saved in**, convert it to UTF-8, here
from Latin-1:

```sh
iconv -f LATIN1 -t UTF-8 app.jadpo > app.utf8
mv app.utf8 app.jadpo
jadpo check
```

The file reads as it was typed, and the whole project is checked again.
An editor's "save with encoding" does the same.

**If you aren't allowed to read it**, make it readable:

```sh
chmod u+r app.jadpo
jadpo check
```

`jadpo check` reads the file and checks the project with it.

## Related

- [`jadpo check`](../language/14-tooling.md#jadpo-check)
- [Source files](../language/01-project-layout-and-modules.md#source-files)
