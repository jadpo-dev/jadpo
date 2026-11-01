# `format.unsafe`

A file was left as it was, because what the formatter made of it didn't read back as the same code, which is a mistake in Jadpo's formatter.

## Why Jadpo refuses it

`jadpo fmt` lays out each `.jadpo` file the one way Jadpo writes it, and
reads its own result back before writing anything: the formatted text
has to parse to the same tokens, nodes and comments as the file, and
formatting it again has to change nothing. When either fails, writing it
could change what the code means, or leave a file that never settles, so
that file is left exactly as it was. The other files are still
formatted.

The file isn't wrong: `jadpo check` still compiles it. Jadpo won't write
a layout it can't show is the same code, and it won't guess a smaller
change instead. Whether the file waits for a fixed formatter, and
whether a report goes to Jadpo's maintainers, is yours to decide.

## What it looks like

A project whose `config.jadpo` is indented with two spaces:

```jadpo check project=e-format-unsafe file=app.jadpo hidden
application {
    jadpo: "0.2"
}
```

```jadpo check project=e-format-unsafe file=config.jadpo hidden
config {
    /// How many todos a page shows.
    page_size: Int {
        default: 20
    }
}
```

```sh
cat > config.jadpo <<'EOF'
config {
  /// How many todos a page shows.
  page_size: Int {
    default: 20
  }
}
EOF
jadpo fmt
```

The formatter checks every file it formats, and no known file makes it
refuse, so no run of a released `jadpo` raises this error. This output
is copied rather than checked: it is `jadpo fmt` on a Mac, run by a
`jadpo` built from this checkout with the read-back check made to fail
for `config.jadpo`:

```text
error: This file couldn't be formatted safely
 --> config.jadpo:1:1
  |
1 | config {
  | ^ left as it is
  reason: The formatted text didn't read back as the same code, or didn't stay the same when formatted again, so the formatter wrote nothing. This is a mistake in the formatter, not in the file.
  next: Leave the file as it is, and report it to Jadpo's maintainers with the file (human)
  rule: format.unsafe
  help: https://jadpo.dev/docs/errors/format.unsafe

fmt failed: 1 refused by the formatter, 1 unchanged
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether the file is reported to Jadpo's maintainers, and so
leaves your project.

## How to fix it

**If the file can be shared**, keep it as it is and report it, with the
version of `jadpo` that refused it:

```sh
jadpo --version
```

Attach the file, or the smallest part of it that is still refused, to
the [issue form](https://github.com/jadpo-dev/jadpo/issues/new?template=bug.yml).
The other files are already formatted, and `jadpo check` still compiles
this one.

**If it can't be shared**, leave it as it is. The project still
compiles:

```sh
jadpo check
```

`jadpo check` doesn't need the file formatted, so it passes; `jadpo fmt
--check`, as CI runs it, keeps refusing that file until a release of
Jadpo fixes the formatter.

## Related

- [Formatting](../language/14-tooling.md#formatting)
- [`jadpo fmt`](../language/14-tooling.md#jadpo-fmt)
