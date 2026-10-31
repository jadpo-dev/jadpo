# `configuration.local_changed`

Something else saved `.env.local` while `jadpo config set` was saving a value in it.

## Why Jadpo refuses it

`jadpo config set` saves a value by reading `.env.local`, writing a new
file beside it, and putting that in the old one's place. Just before the
swap, it reads `.env.local` again. If the file changed in between,
because an editor, a script or a second `jadpo config set` saved it, the
swap would throw that change away, so the command stops, and
`.env.local` keeps what the other program saved.

Jadpo won't merge the two saves for you: the other change may be a
secret, and only you know which value you meant.

## What it looks like

Only two programs saving at the same moment raise this error, so this
output is copied from a real run rather than checked by the docs. A
script was rewriting `.env.local` over and over while a value was typed
at the prompt:

```sh
jadpo config set mail_api_key
```

```text
? Enter MAIL_API_KEY hidden
error: `.env.local` changed while it was being updated
  reason: Something saved the file after it was read, and replacing it now would lose that change.
  next: Run the command again (human)
  rule: configuration.local_changed
  help: https://jadpo.dev/docs/errors/configuration.local_changed
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which of the two saves `.env.local` keeps.

## How to fix it

**If the other save was a one-off**, such as an editor saving the file,
run the command again and type the value once more:

```sh
jadpo config set mail_api_key
```

The value is saved beside whatever the other program wrote, since the
command reads the file afresh.

**If a program keeps rewriting `.env.local`**, stop it first, or give
the value in the environment instead, which nothing writes:

```sh
export MAIL_API_KEY
read -rs MAIL_API_KEY
jadpo dev
```

The environment wins over `.env.local`, and nothing is written.

## Related

- [Setting a value: `jadpo config set`](../language/10-configuration-and-time.md#setting-a-value-jadpo-config-set)
- [Values on your machine and in production](../language/10-configuration-and-time.md#values-on-your-machine-and-in-production)
