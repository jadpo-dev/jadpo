# `files.unavailable`

A store of files couldn't be reached, read or written by a `jadpo files` command, or its settings have no value here.

## Why Jadpo refuses it

`jadpo files` reaches a store as the deployed app does: a disk store at
its `path:` on the server's own disk, and an S3 store with the settings the
environment gives, then `.env.local`. When the folder isn't on this
machine, a setting has no value, or the store can't be reached or has no
room, there is nothing to read or write, so the command stops before it
does either.

Jadpo won't make a disk store's folder to read it, or guess at another
store. The folder may be production's, on another server, and whether
this is the right machine only you know.

## What it looks like

A bakery keeps its price sheets on the server's disk, and its photos in an
S3 bucket:

```jadpo check project=e-files-unavailable file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
        photos: {
            driver: Driver.s3
            host: config.photos_host
            region: "auto"
            bucket: "bakery-photos"
            credential: {
                key_id: config.photos_key_id
                secret: config.photos_secret
            }
        }
    }
}
```

```jadpo check project=e-files-unavailable file=config.jadpo hidden
config {
    /// The S3 service's address.
    photos_host: Url
    /// The S3 credential's key id.
    photos_key_id: Text
    /// The S3 credential's secret access key.
    photos_secret: Text {
        secret: true
    }
}
```

On a laptop, which keeps no `/var/lib/bakery/sheets`:

```sh run project=e-files-unavailable expect=files.unavailable
jadpo files ls sheets
```

```text output
error: The folder `/var/lib/bakery/sheets` isn't on this machine
  reason: `sheets` keeps its files on the server's own disk, at `/var/lib/bakery/sheets`, and there is no such folder here.
  next: Run the command on the server that keeps the files, or add `--dev` for `jadpo dev`'s copy (human)
  rule: files.unavailable
  help: https://jadpo.dev/docs/errors/files.unavailable
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which machine, and so which copy of the files, the command
reaches.

With no value for the S3 store's settings here:

```sh run project=e-files-unavailable expect=files.unavailable
jadpo files ls photos
```

```text output
error: `photos`'s setting `photos_host` has no value here
  reason: An S3 store is reached with the settings the deployed app reads, from the environment and then `.env.local`, and `PHOTOS_HOST` gives none.
  next: Set `PHOTOS_HOST` as the deployment does, or add `--dev` for `jadpo dev`'s copy (human)
  rule: files.unavailable
  help: https://jadpo.dev/docs/errors/files.unavailable
```

## How to fix it

**If you meant the files `jadpo dev` keeps**, add `--dev`:

```sh
jadpo files ls sheets --dev
```

The command reads `.jadpo/files/sheets/` instead.

**If you meant production's**, run the command on the server that keeps
a disk store's files, or with the S3 store's settings set as the
deployment sets them:

```sh
PHOTOS_HOST=https://acme.r2.cloudflarestorage.com PHOTOS_KEY_ID=… PHOTOS_SECRET=… jadpo files ls photos
```

The command reaches the bucket with them and lists what it keeps, never
printing the secret.

**If the store can't be reached or has no room**, check it is up and has
space, then run the command again:

```sh
jadpo files copy /var/lib/bakery/sheets photos
```

`copy` leaves the files it already copied, so it carries on from where it
stopped.

## Related

- [`jadpo files`](../language/14-tooling.md#jadpo-files)
- [The disk driver](../language/16-built-in-services.md#the-disk-driver)
- [The S3 driver](../language/16-built-in-services.md#the-s3-driver)
