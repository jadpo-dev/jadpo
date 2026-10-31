# `connection.disk_path_invalid`

A store of files on the server's own disk, `driver: Driver.disk`, keeps
its files in the folder `path:` names. They are your users' data, so the
folder is written in full, and plainly, and is never inside the project:
among its source or in `build/`, git would commit them, `jadpo build` and
`jadpo dev` could write over them, and every copy of the source would
carry them (area 16, "The disk driver"). A relative path is refused,
since it would depend on where the server starts:

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

So is one with `.` or `..` in it, which would hide which folder it is:

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

A folder inside the project's own, such as `build/files` under it, or a
link that leads there, is refused as `` `uploads`'s `path:` is inside
the project ``, its label naming the folder it is in (`` inside `build/` ``).
Choose a folder the server keeps for the app's data, such as
`/var/lib/<app>/uploads`; the deployable makes it, the app's user's
alone, as it starts.
