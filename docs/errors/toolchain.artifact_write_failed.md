# `toolchain.artifact_write_failed`

A file Jadpo writes for you, such as a record, couldn't be written, usually because it or its folder is read-only.

## Why Jadpo refuses it

Commands such as `jadpo records update`, `jadpo api update`,
`jadpo migrate new` and `jadpo build` write files you commit or deploy.
When the operating system refuses one, the command stops there and says
which file and why, rather than carry on as if the project were up to
date.

Jadpo won't change permissions for you: who may write in the project is
a decision about your machine, not about the app.

## What it looks like

The project's folder is read-only, and `access.jadpo_record`, the record
of who can do what, has to be written again:

```jadpo check project=e-artifact-write file=app.jadpo hidden
application {}
```

```jadpo check project=e-artifact-write file=policy.jadpo hidden
use shop

policy {
    actions: {
        shop.status: [Access.public]
    }
}
```

```jadpo check project=e-artifact-write file=domains/shop/types/opening.jadpo hidden
/// Whether the shop is open.
type Opening = Output {
    open: Bool
}
```

```jadpo check project=e-artifact-write file=domains/shop/actions/status.jadpo hidden
use shop/types { Opening }

/// Says whether the shop is open.
action status() -> Opening {
    return Opening {
        open: true
    }
}
```

```sh run project=e-artifact-write expect=toolchain.artifact_write_failed
rm access.jadpo_record
chmod a-w .
jadpo records update
```

```text output
error: `access.jadpo_record` couldn't be written
  reason: Writing it failed: Permission denied (os error 13).
  next: Check that the folder can be written, then run `jadpo records update` again (human)
  rule: toolchain.artifact_write_failed
  help: https://jadpo.dev/docs/errors/toolchain.artifact_write_failed
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides who may write in the project.

## How to fix it

**If the folder or the file should be writable**, let yourself write to
it, then run the command again:

```sh
chmod u+w . access.jadpo_record
jadpo records update
```

The record is written, ready to commit with the code.

**If a tool keeps the files read-only until you open them for editing**,
open the record for editing with that tool, then run the command again:

```sh
jadpo records update
```

The record is written, and the tool shows it as changed, ready to
commit with the code.

## Related

- [`jadpo records`](../language/18-records-and-manifest.md#jadpo-records)
- [Record files](../language/18-records-and-manifest.md#record-files)
- [The `build/` folder](../language/14-tooling.md#the-build-folder)
