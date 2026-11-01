# `configuration.local_read_failed`

The `.env.local` file is there, but it couldn't be read: it is a folder, or you aren't allowed to read it.

## Why Jadpo refuses it

`.env.local` holds this machine's values for the settings in
`config.jadpo`. When it is there but can't be read, carrying on without
it would start the app with other values, or report settings as missing
that the file sets, so every command that reads it stops instead and
says why reading failed.

Jadpo won't fix it for you: making the file readable or removing it
changes your files, and only you know whether what is there should be
kept.

## What it looks like

A project's `config.jadpo` declares a secret `mail_api_key`, and
`.env.local` is a folder:

```jadpo check project=e-local-read file=app.jadpo hidden
application {
    jadpo: "0.2"
}
```

```jadpo check project=e-local-read file=config.jadpo hidden
config {
    /// The key the mail provider gave this app.
    mail_api_key: Text {
        secret: true
    }
}
```

```sh run project=e-local-read expect=configuration.local_read_failed
mkdir .env.local
jadpo config check
```

```text output
error: `.env.local` couldn't be read
  reason: Reading it failed: Is a directory (os error 21).
  next: Check that `.env.local` is a file you can read (human)
  rule: configuration.local_read_failed
  help: https://jadpo.dev/docs/errors/configuration.local_read_failed
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides what happens to a file that may hold your secrets.

A file you aren't allowed to read, such as one written with `sudo`, is
refused the same way, with `Permission denied` as the reason.

## How to fix it

**If it is a folder made by mistake**, remove it, then store the values
at the prompt:

```sh
rmdir .env.local
jadpo config set mail_api_key
```

The settings read from a new, ordinary `.env.local` that only you can
read.

**If it is a file you aren't allowed to read**, make it yours again:

```sh
sudo chown "$USER" .env.local
```

The commands read the values it holds, and `jadpo dev` starts with
them.

## Related

- [Values on your machine and in production](../language/10-configuration-and-time.md#values-on-your-machine-and-in-production)
- [Setting a value: `jadpo config set`](../language/10-configuration-and-time.md#setting-a-value-jadpo-config-set)
- [Checking values: `jadpo config check`](../language/10-configuration-and-time.md#checking-values-jadpo-config-check)
