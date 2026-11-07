# Use a package from the registry
<!-- topic: guide-use-a-package-from-the-registry -->

A package is Jadpo code someone else wrote for you to install: a library of
types and functions, or a provider package that describes another company's
API. Public packages are published on the registry at jadpo.dev, by name.
This guide finds a package that sends email, installs it, reads what it
offers, and then keeps it up to date, checks it for advisories and removes
it.

You start from any Jadpo project, such as the one `jadpo new` writes, and
run each command in its folder. Every command here but `jadpo check` and
`jadpo build` asks the registry, so it needs the network. The output on this
page is copied from real runs.

## Find a package

Describe what you need in a few words. `jadpo package search` looks through
the standard library and every published package, and lists what Jadpo has
built in first, since you need no package for that:

```sh
jadpo package search send an email
```

```text
Packages
  @jadpo/resend SendEmailRequest  Matched: send an email

1 found: 1 in a package

next: jadpo package add @jadpo/resend  installs the first package found
```

`@jadpo/resend` is a registry name: the publisher, `@jadpo`, then the
package's own part, `resend`. Each result also names the declaration that
matched. [jadpo.dev/packages](https://jadpo.dev/packages) searches the same
packages in a browser, and each package's page shows what it can do and its
releases.

## Install it

`jadpo package add` installs that exact name, or nothing: it never picks a
name that only looks like it. A name with no publisher, such as `resend`,
means Jadpo's own, `@jadpo/resend`.

```sh
jadpo package add @jadpo/resend
```

```text
Added Resend.v1 1.0.0
    folder     packages/resend/v1
    source     registry @jadpo/resend
    publisher  @jadpo, official
    age        first released 2026-11-06, today
    log        entry 1 of 2
    surface    matches its code

Facts
  kind: provider
  publisher: @jadpo
  hosts: the address its connection gives
  changes: emails.send
  erasure: not declared
  storable: none
  failures: emails.send: InvalidAttachment, InvalidParameter, MissingRequiredField, MissingRequiredParameter, ValidationError
  contract: 0 of 1 operation confirmed

package added: Resend.v1 1.0.0, 1 operation, 3 types, 5 failures

next: jadpo package show resend  shows what it offers
```

Read the top before you go on, since it says whose code your app now runs:

- **publisher**: who released it. `official` is Jadpo's own, and `verified`
  is a publisher the registry has checked.
- **age**: when it was first released. A name released today deserves a
  closer look than one apps have used for months.
- **log**: the registry keeps a public log of every release, and `add` checks
  that this release is in it, so the registry can't give your app different
  code from everyone else's.
- **surface**: Jadpo compiled the package and compared what it offers with
  what its author committed, and they match.
- **Facts**: what the package does, worked out from its code rather than its
  author's word: the hosts it calls, the calls that change something there,
  the personal data it sends and the failures each call can give.

The package is now in `packages/resend/v1/`, and `jadpo.lock` holds its name,
version and digest:

```text
package Resend.v1 version 1.0.0 registry @jadpo/resend digest sha256:28e1384d2c0a1029e5381bf6a1271844458e817f3c98e11d4ed67d9940222613
```

Commit both. `jadpo check` and `jadpo build` read only these files and never
go online, so your app builds the same everywhere, and nothing in a package
runs when it is installed.

## See what it offers

`jadpo package show` prints the package's description, facts and everything
it makes public:

```sh
jadpo package show resend
```

```text
Resend: the app's email sent through Resend's API, as the `email` capability.
  package: Resend.v1
  kind: provider
  version: 1.0.0
  source: registry @jadpo/resend
  publisher: @jadpo
  licence: apache_2_0
  jadpo: 1.0
  folder: packages/resend/v1

Facts
  kind: provider
  publisher: @jadpo
  hosts: the address its connection gives
  changes: emails.send
  erasure: not declared
  storable: none
  failures: emails.send: InvalidAttachment, InvalidParameter, MissingRequiredField, MissingRequiredParameter, ValidationError
  contract: 0 of 1 operation confirmed

Provider
  protocol                  json_http
  credential                bearer

Operations
  emails.send               POST /emails  SendEmailRequest -> SendEmailResponse

Types
  EmailAttachment           Object { content: Bytes, content_type: Text?, fil...
  SendEmailRequest          Object { attachments: List<EmailAttachment>, from...
  SendEmailResponse         Object { id: Text }

Failures
  InvalidAttachment         invalid_value
  InvalidParameter          invalid_value
  MissingRequiredField      invalid_value
  MissingRequiredParameter  invalid_value
  ValidationError           invalid_value

Resend.v1: 1 operation, 3 types, 5 failures

next: jadpo package show resend emails.send  shows one in full, with its file
```

`jadpo package show resend emails.send` prints one declaration in full, and
`--guide` prints the package's `GUIDE.md` when it has one. In code a package
keeps its own name and version, `Resend.v1`, whatever its registry name.

## Use it in your app

What you write next depends on the kind of package:

- **A provider package**, such as Resend's, is reached through a connection
  in `app.jadpo` that names it, with its credential from a setting
  ([Connecting in `app.jadpo`](../language/11-integrations.md#connecting-in-appjadpo)).
  Resend's fills the `email` capability, so `email:` names that connection
  and your emails send through it
  ([Jadpo's own packages](../language/11-integrations.md#jadpos-own-packages)
  shows its connection). The first time a connection reaches a new host,
  `jadpo check` asks for a person's acceptance of it
  ([Approve a risky change](09-get-a-sign-off.md)).
- **A library package** is used like one of your own libraries: `use
  packages/<name>`, then its names through the package's name, such as
  `Postcodes.uk.tidy(...)`
  ([Where packages live](../language/11-integrations.md#where-packages-live)).

## Keep it up to date

`jadpo package update` moves a package to the newest release of the same
major version that is at least three days old, so a bad release has time to
be noticed and yanked before your app takes it:

```sh
jadpo package update resend
```

```text
Resend.v1 is up to date: 1.0.0, registry @jadpo/resend
```

When it takes a release, it lists what changed in what the package offers,
marks what your app uses, and shows the new facts. A change that could break
your app, such as a call it uses that now fails in a new way, needs a
person's acceptance before `jadpo check` passes
([Package upgrades](../language/18-records-and-manifest.md#package-upgrades)).
`--to <version>` takes a release at once, after showing its facts. A new major
version is a new package beside the old one, which you add with `jadpo package
add`.

## Check it for advisories

An advisory says a release has a problem, and names the declarations and
versions it affects. `jadpo package audit` asks the registry about every
package your app installed from it, and says whether your app reaches each
affected declaration:

```sh
jadpo package audit
```

```text
no advisories: 1 package
```

It is the one check that goes online, so run it in CI. An advisory your app
reaches fails it ([`package.advisory`](../errors/package.advisory.md)), and one
it doesn't reach is only listed.

## Remove it

`jadpo package remove` takes out a package your app no longer uses. It is
refused while your code still has `use packages/<name>`
([`install.in_use`](../errors/install.in_use.md)):

```sh
jadpo package remove resend
```

```text
Removed packages/resend/v1 and its line in jadpo.lock

package removed: Resend.v1 1.0.0
```

## When something goes wrong

- **The name isn't found.** The registry has no package of that name, or no
  such release ([`install.not_in_registry`](../errors/install.not_in_registry.md)).
  Check the spelling, or search for it.
- **A release was yanked.** Its publisher withdrew it, and new installs refuse
  it ([`install.yanked`](../errors/install.yanked.md)). Apps that have it keep
  working; update to a newer release.
- **The registry can't be reached.** Check your connection and run the command
  again ([`registry.unreachable`](../errors/registry.unreachable.md)).

## Read more

[Where packages come from](../language/11-integrations.md#where-packages-come-from),
[Installing safely](../language/11-integrations.md#installing-safely),
[Installing and updating from the registry](../language/11-integrations.md#installing-and-updating-from-the-registry),
[What a release shows](../language/11-integrations.md#what-a-release-shows) and
[Commands for packages](../language/14-tooling.md#commands-for-packages). To
publish a package of your own, see [Publish a package](23-publish-a-package.md).

## Not built yet

The registry doesn't publish advisories yet, so `jadpo package audit` finds
none for now. Search matches words, not meaning, so describe what you need in
the words a package's descriptions would use.
