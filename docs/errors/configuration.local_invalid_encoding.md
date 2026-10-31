# `configuration.local_invalid_encoding`

The `.env.local` file holds bytes that aren't UTF-8 text.

## Why Jadpo refuses it

Jadpo reads `.env.local` as UTF-8 text, so the value the app gets is
the value that was typed. A file saved in another encoding, such as
Latin-1 or UTF-16 by an older editor, would decode to different
characters, and a secret read that way is a wrong secret that fails
later, somewhere far from its cause. Every command that reads the file
stops instead.

Jadpo won't convert the file for you: doing so means guessing which
encoding it was saved in, and the file holds values only you know.

## What it looks like

A project's `config.jadpo` declares a secret `mail_api_key`, and an
editor saved `.env.local` as Latin-1, so its `é` is one byte that isn't
UTF-8:

```jadpo check project=e-local-encoding file=app.jadpo hidden
application {
    jadpo: "0.1"
}
```

```jadpo check project=e-local-encoding file=config.jadpo hidden
config {
    /// The key the mail provider gave this app.
    mail_api_key: Text {
        secret: true
    }
}
```

```sh run project=e-local-encoding expect=configuration.local_invalid_encoding
printf 'MAIL_API_KEY=caf\351_51ab\n' > .env.local
jadpo config check
```

```text output
error: `.env.local` isn't UTF-8 text
  reason: Local values are read as UTF-8 text, one `NAME=value` per line, so what is read is what was typed.
  next: Open `.env.local` in an editor and save it as UTF-8 (human)
  rule: configuration.local_invalid_encoding
  help: https://jadpo.dev/docs/errors/configuration.local_invalid_encoding
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides what `.env.local` holds, which may be secrets only you should
see.

## How to fix it

**If you know the encoding it was saved in**, convert it to UTF-8, here
from Latin-1:

```sh
iconv -f LATIN1 -t UTF-8 .env.local > .env.local.utf8
mv .env.local.utf8 .env.local
```

The values read as they were typed, and `jadpo dev` starts with them.
An editor's "save with encoding" does the same.

**If you'd rather type the values again**, delete the file and store
each value at the prompt:

```sh
rm .env.local
jadpo config set mail_api_key
```

`jadpo config set` writes a new `.env.local` as UTF-8, with the value
you typed.

## Related

- [Values on your machine and in production](../language/10-configuration-and-time.md#values-on-your-machine-and-in-production)
- [Setting a value: `jadpo config set`](../language/10-configuration-and-time.md#setting-a-value-jadpo-config-set)
- [Checking values: `jadpo config check`](../language/10-configuration-and-time.md#checking-values-jadpo-config-check)
