# `configuration.local_write_failed`

Saving a value in `.env.local` failed, usually because the project's folder can't be written.

## Why Jadpo refuses it

`jadpo config set` saves a value by writing a new file only you can
read beside `.env.local`, then putting it in the old one's place, so a
half-written file never holds your secrets. When that can't be done,
it stops and says why, and `.env.local` is as it was.

Jadpo won't change the folder's permissions for you: who may write in
it is a decision about your machine, not about the app.

## What it looks like

A project's `config.jadpo` declares a secret `mail_api_key`, and the
project's folder is read-only. Storing a value at the prompt:

```jadpo check project=e-local-write file=app.jadpo hidden
application {
    jadpo: "0.2"
}
```

```jadpo check project=e-local-write file=config.jadpo hidden
config {
    /// The key the mail provider gave this app.
    mail_api_key: Text {
        secret: true
    }
}
```

```sh run project=e-local-write expect=configuration.local_write_failed tty typed=mk_test_51ab
chmod a-w .
jadpo config set mail_api_key
```

```text output
? Enter MAIL_API_KEY hidden
error: `.env.local` couldn't be saved
  reason: Creating it failed: Permission denied (os error 13).
  next: Check that the project's folder is writable, then run the command again (human)
  rule: configuration.local_write_failed
  help: https://jadpo.dev/docs/errors/configuration.local_write_failed
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides who may write in the project's folder.

## How to fix it

**If the folder should be writable**, let yourself write to it, then
store the value again:

```sh
chmod u+w .
jadpo config set mail_api_key
```

The value is saved in `.env.local`, which git ignores.

**If the folder should stay read-only**, give the value in the
environment instead, as a server does:

```sh
export MAIL_API_KEY
read -rs MAIL_API_KEY
jadpo dev
```

The environment wins over `.env.local`, and nothing is written.

## Related

- [Setting a value: `jadpo config set`](../language/10-configuration-and-time.md#setting-a-value-jadpo-config-set)
- [Values on your machine and in production](../language/10-configuration-and-time.md#values-on-your-machine-and-in-production)
