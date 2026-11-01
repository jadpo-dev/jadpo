# `package.own_source_invalid`

Application's `packages:` lists your company's git locations wrongly.

## Why Jadpo refuses it

A package from a repository under one of `own_sources:` installs without
a person's acceptance, so the list says exactly whose code the app trusts.
An entry is a host and a path ending in `/`, and is matched by its host
and its whole path parts. Anything else could read as covering more than
it does, or cover nothing while a reviewer believes it does, so Jadpo
doesn't guess what was meant.

## What it looks like

An entry with no path would let in every repository on the host:

```jadpo check project=e-own-source-host expect=package.own_source_invalid
application {
    packages: {
        own_sources: ["github.com/"]
    }
}
```

```text output
error: `github.com/` isn't a host and a path ending in `/`
 --> main.jadpo:3:23
  |
3 |         own_sources: ["github.com/"]
  |                       ^^^^^^^^^^^^^ not a git location
  reason: An entry is a host and a path ending in `/`, such as `github.com/acme/`: `github.com/` alone would let in every repository on the host, a stranger's among them.
  next: Name the company's folder on the host, such as `github.com/acme/` (agent)
  rule: package.own_source_invalid
  help: https://jadpo.dev/docs/errors/package.own_source_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An entry that doesn't end in `/` is refused, with the edit to the one
that does:

```jadpo check project=e-own-source-slash expect=package.own_source_invalid
application {
    packages: {
        own_sources: ["github.com/acme"]
    }
}
```

```text output
error: `github.com/acme` isn't a host and a path ending in `/`
 --> main.jadpo:3:23
  |
3 |         own_sources: ["github.com/acme"]
  |                       ^^^^^^^^^^^^^^^^^ not a git location
  reason: An entry is a host and a path ending in `/`, such as `github.com/acme/`. An entry ends in `/` so that what it covers reads plainly: `github.com/acme/` covers `github.com/acme/postcodes`, and never `github.com/acme-evil/postcodes`.
  next: Write `github.com/acme/` (agent)
  rule: package.own_source_invalid
  help: https://jadpo.dev/docs/errors/package.own_source_invalid

check failed: 1 error, 1 file
```

So is an address with a scheme, which says how a repository is fetched,
not whose it is:

```jadpo check project=e-own-source-scheme expect=package.own_source_invalid
application {
    packages: {
        own_sources: ["https://github.com/acme/"]
    }
}
```

```text output
error: `https://github.com/acme/` isn't a host and a path ending in `/`
 --> main.jadpo:3:23
  |
3 |         own_sources: ["https://github.com/acme/"]
  |                       ^^^^^^^^^^^^^^^^^^^^^^^^^^ not a git location
  reason: An entry is a host and a path ending in `/`, such as `github.com/acme/`, with no scheme: `https://` is how a repository is fetched, not whose it is.
  next: Write `github.com/acme/` (agent)
  rule: package.own_source_invalid
  help: https://jadpo.dev/docs/errors/package.own_source_invalid

check failed: 1 error, 1 file
```

A misspelt setting is refused with the edit to `own_sources`, since a list
that was never read would let nothing in without a word:

```jadpo check project=e-own-source-setting expect=package.own_source_invalid
application {
    packages: {
        own_source: ["github.com/acme/"]
    }
}
```

```text output
error: `own_source` isn't something `packages:` holds
 --> main.jadpo:3:9
  |
3 |         own_source: ["github.com/acme/"]
  |         ^^^^^^^^^^ not a setting
  reason: `packages:` holds only `own_sources:`, your company's git locations.
  next: Use `own_sources` instead of `own_source` (agent)
  rule: package.own_source_invalid
  help: https://jadpo.dev/docs/errors/package.own_source_invalid

check failed: 1 error, 1 file
```

## How to fix it

**If the entry should cover your company's repositories**, write its host
and the folder that holds them, ending in `/`:

```jadpo check project=e-own-source-fix-1
application {
    packages: {
        own_sources: ["github.com/acme/"]
    }
}
```

A package locked from `github.com/acme/postcodes` now needs no
`PackageChange.git_source`, and `github.com/acme-evil/postcodes` still does.
The change that adds the entry needs a person's acceptance itself:
`"github.com/acme/": PolicyChange.own_source_added`.

## Related

- [Installing safely](../language/11-integrations.md#installing-safely)
- [Acceptances](../language/18-records-and-manifest.md#acceptances)
