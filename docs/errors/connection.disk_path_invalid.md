# `connection.disk_path_invalid`

A disk store's `path:` isn't an absolute folder outside the project.

## Why Jadpo refuses it

A store of files on the server's own disk, `driver: Driver.disk`, keeps
its files in the folder `path:` names, and those files are your users'
data. Inside the project, among its source or in `build/`, git would
commit them, `jadpo build` and `jadpo dev` could write over them, and
every copy of the source would carry them. A relative path is refused
too, since the folder it names depends on where the server starts, which
is often the project's own folder. Only you know which folder on the
server is kept for the app's data, so Jadpo doesn't choose one.

## What it looks like

app.jadpo:

```jadpo check project=e-disk-path-relative file=app.jadpo expect=connection.disk_path_invalid
application {
    connections: {
        uploads: {
            driver: Driver.disk
            path: "uploads"
        }
    }
}
```

```text output
error: `uploads`'s `path:` must be an absolute folder
 --> app.jadpo:5:19
  |
5 |             path: "uploads"
  |                   ^^^^^^^^^ a relative path
  reason: A disk store keeps its files at `path:` on the server that runs the app. A relative path depends on where the server starts, often the project's own folder, where git and `jadpo build` would sweep users' files up with the source.
  next: Write the folder in full, outside the project, such as `path: "/var/lib/<app>/uploads"` (human)
  rule: connection.disk_path_invalid
  help: https://jadpo.dev/docs/errors/connection.disk_path_invalid

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides where on the server your users' files are kept.

A path with `.` or `..` in it is refused the same way, since it hides
which folder it is:

```jadpo check project=e-disk-path-dotted file=app.jadpo expect=connection.disk_path_invalid
application {
    connections: {
        uploads: {
            driver: Driver.disk
            path: "/var/lib/acme/../uploads"
        }
    }
}
```

```text output
error: `uploads`'s `path:` must name its folder without `.` or `..`
 --> app.jadpo:5:19
  |
5 |             path: "/var/lib/acme/../uploads"
  |                   ^^^^^^^^^^^^^^^^^^^^^^^^^^ holds `.` or `..`
  reason: `path:` names a disk store's folder as it is, so which folder it is, and whether it is inside the project, can be told from the text alone.
  next: Write the folder in full, outside the project, such as `path: "/var/lib/<app>/uploads"` (human)
  rule: connection.disk_path_invalid
  help: https://jadpo.dev/docs/errors/connection.disk_path_invalid

check failed: 1 error, 1 file
```

So is a folder inside the project's own, such as `build/files` under it,
or a link that leads there: the headline is `` `uploads`'s `path:` is
inside the project ``, and the label names the folder it is in, such as
`` inside `build/` ``.

## How to fix it

**If the server keeps a folder for the app's data**, name it in full:

```jadpo fragment
uploads: {
    driver: Driver.disk
    path: "/var/lib/acme/uploads"
}
```

```jadpo check project=e-disk-path-fix file=app.jadpo hidden
application {
    connections: {
        uploads: {
            driver: Driver.disk
            path: "/var/lib/acme/uploads"
        }
    }
}
```

The deployable makes the folder as it starts when it is missing, readable
by the app's user only, and doesn't use one other users can write to.

## Related

- [The disk driver](../language/16-built-in-services.md#the-disk-driver)
- [Files](../language/16-built-in-services.md#files)
- [A native deployable](../language/14-tooling.md#a-native-deployable)
- [`deployable.target_unsupported`](../errors/deployable.target_unsupported.md)
