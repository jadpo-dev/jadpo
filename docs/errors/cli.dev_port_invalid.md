# `cli.dev_port_invalid`

A `jadpo dev --port` flag holds something other than a whole number from 0 to 65535.

## Why Jadpo refuses it

`jadpo dev` serves the app at `http://localhost:3000`, and `--port`
moves it to another port. A port is a whole number from 0 to 65535, and
0 asks the system for any free one. Anything else names no port the app
could listen on, so nothing starts rather than the app serving
somewhere you didn't ask for.

## What it looks like

The port is one past the largest:

```jadpo check project=e-dev-port file=app.jadpo hidden
application {
    jadpo: "0.2"
}
```

```sh run project=e-dev-port expect=cli.dev_port_invalid
jadpo dev --port 65536
```

```text output
error: `65536` isn't a port
  reason: A port is a whole number from 0 to 65535; 0 lets the system choose a free one.
  next: Use a port such as `--port 3000` (agent)
  rule: cli.dev_port_invalid
  help: https://jadpo.dev/docs/errors/cli.dev_port_invalid
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

A port written with its host, as an address is, is refused with the
port alone to give instead:

```sh run project=e-dev-port expect=cli.dev_port_invalid
jadpo dev --port localhost:8080
```

```text output
error: `localhost:8080` isn't a port
  reason: A port is a whole number from 0 to 65535; 0 lets the system choose a free one.
  next: Give the port alone: `--port 8080` (agent)
  rule: cli.dev_port_invalid
  help: https://jadpo.dev/docs/errors/cli.dev_port_invalid
```

## How to fix it

**If you want a port of your own**, give its number alone, as the
`next:` line does:

```sh
jadpo dev --port 8080
```

The app serves at `http://localhost:8080`.

**If any free port will do**, give 0:

```sh
jadpo dev --port 0
```

The system picks a port, and the screen `jadpo dev` prints names it.

## Related

- [`jadpo dev`](../language/14-tooling.md#jadpo-dev)
- [The `jadpo` command](../language/14-tooling.md#the-jadpo-command)
