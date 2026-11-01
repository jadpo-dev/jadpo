# `cli.email_arguments`

A `jadpo email` command names an action it doesn't have, or a mail or link the dev inbox doesn't hold.

## Why Jadpo refuses it

`jadpo email` reads the inbox `jadpo dev` catches mail in: it lists the mail, shows one, lists its links or opens one. Each acts on a mail by its id, and never on another one for a name it doesn't know.

## What it looks like

Nothing has run `jadpo dev` yet, so the inbox is empty:

```jadpo check project=e-email-arguments file=app.jadpo hidden
application {
    jadpo: "0.2"
}
```

```sh run project=e-email-arguments expect=cli.email_arguments
jadpo email show mail_3
```

```text output
error: There is no mail `mail_3`
  reason: The inbox keeps the last 200 mails `jadpo dev` caught, and holds none now.
  next: Run `jadpo email` to see their ids (agent)
  rule: cli.email_arguments
  help: https://jadpo.dev/docs/errors/cli.email_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

An action it doesn't have is refused with the ones it does:

```sh run project=e-email-arguments expect=cli.email_arguments
jadpo email read mail_3
```

```text output
error: `read` isn't something `jadpo email` does
  reason: `jadpo email` reads the dev inbox and checks the sending domain: `list`, `show`, `links`, `open`, `clear` or `check-domain`.
  next: Run `jadpo email` to list the mail (agent)
  rule: cli.email_arguments
  help: https://jadpo.dev/docs/errors/cli.email_arguments
```

## How to fix it

**If you want to read a mail**, list them first, then show one by its id:

```sh
jadpo email
jadpo email show mail_3
```

The inbox keeps the last 200 mails `jadpo dev` caught.

## Related

- [`jadpo email`](../language/14-tooling.md#jadpo-email)
- [`jadpo dev`](../language/14-tooling.md#jadpo-dev)
