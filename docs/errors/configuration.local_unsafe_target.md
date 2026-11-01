# `configuration.local_unsafe_target`

The `.env.local` file that `jadpo config set` would save a value in is a link, not an ordinary file.

## Why Jadpo refuses it

`jadpo config set` saves secrets, and it writes them only where it can
keep them safe: an ordinary `.env.local` in the project, which it adds
to `.gitignore` and which only you can read. Through a link, the secret
would land wherever the link points, in a file git may not ignore, a
folder that is synced, or another project's settings. So it refuses,
before writing anything.

Jadpo won't remove the link for you, since other projects may read the
file it points to.

## What it looks like

A project's `config.jadpo` declares a secret `mail_api_key`, and
`.env.local` is a link to a file of values kept in the home folder.
Storing a value at the prompt:

```jadpo check project=e-local-link file=app.jadpo hidden
application {}
```

```jadpo check project=e-local-link file=config.jadpo hidden
config {
    /// The key the mail provider gave this app.
    mail_api_key: Text {
        secret: true
    }
}
```

```sh run project=e-local-link expect=configuration.local_unsafe_target tty typed=mk_test_51ab
ln -s ~/shared.env .env.local
jadpo config set mail_api_key
```

```text output
? Enter MAIL_API_KEY hidden
error: `.env.local` isn't an ordinary file
  reason: It is a link or a folder, and a secret written through a link could end up somewhere that isn't git-ignored, so Jadpo writes only to an ordinary file.
  next: Remove `.env.local`, and run the command again (human)
  rule: configuration.local_unsafe_target
  help: https://jadpo.dev/docs/errors/configuration.local_unsafe_target
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides where your secrets are kept.

## How to fix it

**If this project should keep its own values**, remove the link and
store the value again:

```sh
rm .env.local
jadpo config set mail_api_key
```

The value is in an ordinary `.env.local` in the project, which git
ignores. The file the link pointed to is unchanged.

**If the values should keep coming from the shared file**, edit that
file yourself instead of running `jadpo config set`:

```sh
"$EDITOR" ~/shared.env
```

Every command still reads the values through the link; only
`jadpo config set` refuses to write through it.

## Related

- [Setting a value: `jadpo config set`](../language/10-configuration-and-time.md#setting-a-value-jadpo-config-set)
- [Values on your machine and in production](../language/10-configuration-and-time.md#values-on-your-machine-and-in-production)
- [Secrets](../language/10-configuration-and-time.md#secrets)
