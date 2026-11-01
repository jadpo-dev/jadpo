# `email.browser_failed`

A `jadpo email open` command couldn't start the browser to open a mail's link.

## Why Jadpo refuses it

`jadpo email open <mail> <link>` opens a link of a mail the dev inbox
caught in your browser: the program `BROWSER` names, or else the
system's opener, `open` on a Mac and `xdg-open` elsewhere. When that
program isn't there, or can't be run, nothing opens, so the command says
which program it tried and gives the link to open by hand.

Jadpo won't choose a browser for you: which program opens links on your
machine is yours to set.

## What it looks like

`jadpo dev` caught a welcome mail, and `BROWSER` names a program that
isn't installed. The output was copied from a run rather than checked,
since a mail reaches the inbox only through `jadpo dev`; the mail was
written into `build/dev/inbox` as `jadpo dev` writes it:

```jadpo check project=e-browser-failed file=app.jadpo hidden
application {
    jadpo: "0.1"
}
```

```sh
BROWSER=firefx jadpo email open mail_1 1
```

```text
error: The browser couldn't be started
  reason: `BROWSER` names `firefx` as the browser, and there's no program of that name.
  next: Set `BROWSER` to your browser's command, or open `http://localhost:3000/welcome` yourself (human)
  rule: email.browser_failed
  help: https://jadpo.dev/docs/errors/email.browser_failed
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which program opens links on your machine.

With `BROWSER` unset, the same refusal names the system's opener.

## How to fix it

The question is which browser should open the link.

**If you have one in mind**, set `BROWSER` to its command, then open
the link again:

```sh
export BROWSER=firefox
jadpo email open mail_1 1
```

The link opens in that browser, and every later `jadpo email open` in
the shell uses it too.

**If any will do**, leave `BROWSER` unset, so the system's opener opens
it:

```sh
unset BROWSER
jadpo email open mail_1 1
```

The link opens in the system's default browser.

## Related

- [`jadpo email`](../language/14-tooling.md#jadpo-email)
- [Email](../language/16-built-in-services.md#email)
