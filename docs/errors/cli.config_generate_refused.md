# `cli.config_generate_refused`

A new key would be written where a person could read it, by `jadpo config generate`, or `--generate` would replace a key already set.

## Why Jadpo refuses it

Nobody needs to see a key, and losing or leaking one can't be undone. So
`jadpo config generate` writes one only into a pipe, never to a terminal or
inside a JSON document, where it would stay in scrollback or a coding agent's
context. And `jadpo config set <field> --generate` won't replace a key already
set: everything the old key sealed would be unreadable, for good.

## What it looks like

A project that seals with `field_encryption_key`, whose key is already in
`.env.local`:

```jadpo check project=e-config-generate file=app.jadpo hidden
application {
    jadpo: "0.2"
}
```

```jadpo check project=e-config-generate file=config.jadpo hidden
config {
    field_encryption_key: EncryptionKey
}
```

```sh run project=e-config-generate expect=cli.config_generate_refused
jadpo config set field_encryption_key --generate
jadpo config set field_encryption_key --generate
```

```text output
error: `FIELD_ENCRYPTION_KEY` already has a value, so `--generate` leaves it alone
  reason: Replacing a key leaves every value sealed with it unreadable, for good, and no backup brings them back without the old key.
  next: To rotate it, move its value to `FIELD_ENCRYPTION_KEY_PREVIOUS` in `.env.local`, remove `FIELD_ENCRYPTION_KEY`, and run this again; to start again with no data, delete the line yourself (human)
  rule: cli.config_generate_refused
  help: https://jadpo.dev/docs/errors/cli.config_generate_refused
```

`(human)` at the end of the `next:` line tells your coding agent to stop and
ask you which fix you want, because the choice is yours to make: it decides
whether what the key sealed is kept.

At a terminal, `jadpo config generate` is refused the same way:

```sh run project=e-config-generate tty expect=cli.config_generate_refused
jadpo config generate field_encryption_key
```

```text output
error: `jadpo config generate` writes a key, and this is a terminal
  reason: A key on a screen reaches scrollback, screen shares and a coding agent's context, and losing or leaking this one can't be undone, so the command writes it only into a pipe.
  next: Pipe it into your secret manager, `jadpo config generate field_encryption_key | wrangler secret put <VARIABLE>`, or run `jadpo config set field_encryption_key --generate` to keep it in `.env.local` (human)
  rule: cli.config_generate_refused
  help: https://jadpo.dev/docs/errors/cli.config_generate_refused
```

## How to fix it

**If you are rotating the key**, move its value to
`FIELD_ENCRYPTION_KEY_PREVIOUS` in `.env.local`, remove `FIELD_ENCRYPTION_KEY`,
and run this again:

```sh
jadpo config set field_encryption_key --generate
```

The app then reads both until nothing needs the old one
([Rotating the key](../language/15-personal-data.md#rotating-the-key)).

**If you want a key for somewhere else**, pipe it there:

```sh
jadpo config generate field_encryption_key | wrangler secret put FIELD_ENCRYPTION_KEY
```

The key goes straight into your secret manager, and nothing on this machine
holds it.

## Related

- [Setting a value: `jadpo config set`](../language/10-configuration-and-time.md#setting-a-value-jadpo-config-set)
- [What can't be undone](../language/15-personal-data.md#what-cant-be-undone)
