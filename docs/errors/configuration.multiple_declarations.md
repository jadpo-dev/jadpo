# `configuration.multiple_declarations`

A project declares `config { }` more than once.

## Why Jadpo refuses it

Every setting the application needs is a field of the one `config { }`, in
`config.jadpo`, so one place lists everything a deployment must provide. A
second declaration would mean reading two places to find out, and Jadpo
won't merge them for you, since the order and comments are yours.

## What it looks like

```jadpo check project=e-multiple expect=configuration.multiple_declarations
config {
    mail_sender: Email {
        binding: "MAIL_SENDER"
    }
}

config {
    browser_origin: Url {
        binding: "BROWSER_ORIGIN"
    }
}
```

```text output
error: `config { }` is declared twice
 --> main.jadpo:7:1
  |
1 | config {
  | ------ first declared here
7 | config {
  | ^^^^^^ declared again
  reason: Every setting the application needs is a field of the one `config { }`, in `config.jadpo`, so one place lists everything a deployment must provide.
  next: Move the settings this `config` holds into the first, and remove it (agent)
  rule: configuration.multiple_declarations
  help: https://jadpo.dev/docs/errors/configuration.multiple_declarations

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If both are in use**, move the settings of the second into the first, and
remove it:

```jadpo check project=e-multiple-fix
config {
    browser_origin: Url {
        binding: "BROWSER_ORIGIN"
    }

    mail_sender: Email {
        binding: "MAIL_SENDER"
    }
}
```

Every setting is now in the one list, and code reads each as
`config.<name>` as before.

## Related

- [Settings: `config.jadpo`](../language/10-configuration-and-time.md#settings-configjadpo)
