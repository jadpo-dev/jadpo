# `format.write_failed`

A file `jadpo fmt` formatted couldn't be written, usually because it is read-only.

## Why Jadpo refuses it

`jadpo fmt` lays out each `.jadpo` file the one way Jadpo writes it, and
writes back the files that change. When the operating system refuses
one, it says which file and why, leaves that file as it was, and still
formats the others. It exits with a failure, so a script or an agent
doesn't take the project for formatted.

Jadpo won't change the file's permissions for you: a read-only file may
be read-only on purpose, and who may write it is a decision about your
machine, not about the app.

## What it looks like

A project's `config.jadpo` is indented with two spaces, and the file is
read-only:

```jadpo check project=e-format-write file=app.jadpo hidden
application {
    jadpo: "0.2"
}
```

```sh run project=e-format-write expect=format.write_failed
cat > config.jadpo <<'EOF'
config {
  /// How many todos a page shows.
  page_size: Int {
    default: 20
  }
}
EOF
chmod a-w config.jadpo
jadpo fmt
```

```text output
error: `config.jadpo` couldn't be written
 --> config.jadpo
  reason: Writing the formatted file failed: Permission denied (os error 13).
  next: Check the file can be written, then run `jadpo fmt` again (human)
  rule: format.write_failed
  help: https://jadpo.dev/docs/errors/format.write_failed

fmt failed: 1 couldn't be written, 1 unchanged
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides who may write the project's files.

## How to fix it

**If the file should be writable**, let yourself write to it, then
format again:

```sh
chmod u+w config.jadpo
jadpo fmt
```

The file is written in the canonical layout, with four spaces for each
level, and `jadpo fmt` exits 0.

## Related

- [Formatting](../language/14-tooling.md#formatting)
- [`jadpo fmt`](../language/14-tooling.md#jadpo-fmt)
