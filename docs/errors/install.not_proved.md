# `install.not_proved`

The registry's answers don't prove the release is the one in its public log.

## Why Jadpo refuses it

Every release's digest is in a public, append-only log, so the registry
can't give one app different code from another. `jadpo package add` and
`update` take a release only when all of this holds:

- the log's head extends the last head this project saw, kept in
  `.jadpo/registry_head`;
- the log proves the release is in that head, under its own name and
  version;
- the archive's files have the digest the log holds;
- the package in those files is what the registry says it is.

If any of these fails, the registry, or something between you and it, is
answering differently from what it has told everyone else. Jadpo installs
nothing, and leaves `jadpo.lock` and `packages/` as they were.

## What it looks like

An archive arrives with other files than the release the log holds:

```sh
jadpo package add @acme/postcodes
```

```text
error: The archive of `@acme/postcodes` 2.1.0 isn't the release the log holds
  reason: The log holds `sha256:e3b0…`, and its files give `sha256:9f86…`.
  next: Install nothing from it until you know why: report it to Jadpo, with this output (human)
  rule: install.not_proved
  help: https://jadpo.dev/docs/errors/install.not_proved
```

Other messages say which check failed: `The registry's public log doesn't
prove`, with a head that doesn't extend the one this project saw or a
proof that doesn't hold; or `isn't what the registry says it is`, when the
files name another package or version.

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you, because nothing it can change makes this right.

## How to fix it

**Don't work around it.** Don't delete `.jadpo/registry_head` to make the
message go away: it is the record of what this project has already been
told. Try again from another network, in case something on yours is
changing the answers, and report it to Jadpo with the output if it stays:
see [SECURITY.md](../../SECURITY.md).

**If you need the package now**, keep the version you have installed. Apps
never go online to run a package they already hold.

## Related

- [The registry protocol](../language/11-integrations.md#the-registry-protocol)
- [Installing and updating from the registry](../language/11-integrations.md#installing-and-updating-from-the-registry)
