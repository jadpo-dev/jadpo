# Publish a package
<!-- topic: guide-publish-a-package -->

A package you publish on the registry at jadpo.dev can be installed by
anyone, by name: `jadpo package add @acme/tidying`. The registry never takes
an upload. It fetches each release from a tag in your public git repository,
builds and tests it itself, and lists it for you to publish with a passkey.
This guide starts a small library package, `Tidying`, that takes the spaces
off what people type, checks it, tags it and publishes it as
`@acme/tidying`.

You start from an empty folder, with `jadpo` and `git` installed and a public
repository on GitHub or another git host to push to. `@acme` and
`github.com/acme/tidying` stand for your own publisher name and repository.

## Start the package

`jadpo package new` makes a package's folder, named as the registry writes the
name. `--library` makes a package of types and functions, and `--publisher`
names who publishes it:

```sh run
jadpo package new Tidying --library --publisher @acme
```

```text output
Created tidying
    package    Tidying.v1, library
    publisher  @acme
    licence    proprietary, publishing needs an open one
    jadpo      X.Y
    files      15 written

next: cd tidying && jadpo package check  runs the publishing bar on it
```

It writes a working package with a small example of each kind of file, which
passes `jadpo package check` as it is. The rest of this guide replaces the
example with the package's own code. Delete what you won't keep:

```sh
cd tidying
rm -r recipes sample libraries/starter.jadpo libraries/starter.test.jadpo
```

`recipes/` holds code an app can copy in, and `sample/` is the small app the
recipes compile in. This package has none.

## Say what it is

`package.jadpo` is the package's index: one sentence on what it does, who
publishes it, its licence and the lowest Jadpo release line it needs. The
registry carries only open licences, so choose one: `Licence.mit` here.
`package`, `Licence` and the keys are built into Jadpo; the name `Tidying` is
yours, and its version label, `v1`, is its first major version.

`package.jadpo` (replace the whole file):

```jadpo check project=guide-publish file=shared/providers/tidying/v1/package.jadpo
/// Tidying: text as people type it, with the spaces around it taken off.
package Tidying.v1 {
    publisher: "@acme"
    licence: Licence.mit
    jadpo: "X.Y"
}
```

## Write the code

A package is laid out as an app's `shared/` folder is, and `public` marks what
an app may use. These two types and the failure are the ones `jadpo package
new` wrote, kept as they are.

`types/entered_text.jadpo`:

```jadpo check project=guide-publish file=shared/providers/tidying/v1/types/entered_text.jadpo
/// Text as someone typed it.
public type EnteredText = Text {
    max_length: 200
}
```

`types/tidy_text.jadpo`:

```jadpo check project=guide-publish file=shared/providers/tidying/v1/types/tidy_text.jadpo
/// Text with no spaces around it, and something in it.
public type TidyText = Text {
    min_length: 1
    max_length: 200
}
```

`failures/nothing_entered.jadpo`:

```jadpo check project=guide-publish file=shared/providers/tidying/v1/failures/nothing_entered.jadpo
/// Nothing was left once the text was tidied.
public type NothingEntered = Failure {
    kind: FailureKind.invalid_value
}
```

The function is new. Every public declaration needs a `///` description, and
every function of a public library needs an example in its description,
which runs as a test when the package is checked. The example shows each
failure the function can raise. `text.trim` is built in.

`libraries/tidy.jadpo`:

```jadpo check project=guide-publish file=shared/providers/tidying/v1/libraries/tidy.jadpo
/// Tidying what people type.
public library tidy {
    /// Takes the spaces off both ends of what someone typed.
    ///
    /// ```jadpo
    /// var tidied: TidyText = attempt tidy.entered("  Hello ")
    /// assert tidied == "Hello"
    /// assert tidy.entered("   ") fails NothingEntered
    /// ```
    function entered(typed: EnteredText) fails NothingEntered -> TidyText {
        return attempt TidyText(text.trim(typed)) {
            invalid: NothingEntered
        }
    }
}
```

Tests sit beside the file, as in an app.

`libraries/tidy.test.jadpo`:

```jadpo check project=guide-publish file=shared/providers/tidying/v1/libraries/tidy.test.jadpo
test "tidying keeps the spaces inside the text" {
    var tidied: TidyText = attempt tidy.entered(" a b ")
    assert tidied == "a b"
}
```

```jadpo check project=guide-publish file=app.jadpo hidden
application {}
```

## Say why it is released

`CHANGELOG.md` has a section for each release. Jadpo writes the list of what
changed, and you write why, beneath it.

`CHANGELOG.md` (replace the whole file):

```text
# Changelog

## 1.0.0

- first release

Tidies text as people type it, for forms.
```

`GUIDE.md` is how to use the package well, which `jadpo package show tidying
--guide` prints in an app. Write a few lines there, and delete its "Before you
release it" section.

## Check it

`jadpo package check` runs the publishing bar, the checks the registry holds
every release to: it compiles, every public declaration is described, the
examples and tests pass, the version is the one its changes allow, the
changelog says why, and it names a publisher and an open licence. It prints
the package's facts, as the registry will show them, and writes
`surface.jadpo_record`, the list of everything the package offers:

```sh
jadpo package check
```

```text
Facts
  kind: library, pure
  publisher: @acme
  storable: EnteredText, TidyText
  failures: tidy.entered: NothingEntered

Publishing bar
  compiles      passed                        no errors
  described     passed                        every public declaration
  examples      passed                        1 example, every failure shown
  tests         passed                        1 test
  version       passed                        1.0.0, the first release
  changelog     passed                        1.0.0 says why
  publisher     passed                        @acme
  licence       passed                        mit, from Jadpo X.Y

Wrote surface.jadpo_record

package check passed: Tidying.v1 1.0.0, 8 passed
```

A row that fails says what to change. The version comes from the package's
surface: the first release is `1.0.0`, and each later one is worked out from
what changed since the last tag, so a change that would break an app needs a
new major version.

## Tag the release

Commit everything, `surface.jadpo_record` included, and push it. A release is
a tag, `v` and the version, pushed to the repository:

```sh
git init
git add .
git commit -m "Tidying 1.0.0"
git branch -M main
git remote add origin https://github.com/acme/tidying
git push -u origin main
git tag v1.0.0
git push origin v1.0.0
```

The repository has to be public and reached over HTTPS, since the registry
fetches the release from it.

## Choose your publisher name

The first time, sign in on [jadpo.dev](https://jadpo.dev/sign-in) with your
email address. Accounts are open to a short list of people for now; anyone can
search and install without one.

1. **Choose your publisher name**, the `@name` your packages are published
   under: `@acme`. It is the `publisher:` in `package.jadpo`.
2. **Name your GitHub user or organisation**, or the account on another git
   host, where your repositories are: `github.com/acme`. jadpo.dev publishes
   under your name only from repositories there, so nobody else's repository
   can.
3. **Add a passkey** on your account page. Publishing a version needs one; an
   email address alone never does.

## Publish it

In the package's folder, at the tag:

```sh
jadpo package publish
```

It checks that the folder is the tag's commit, unchanged, with the same tag
on `origin` ([`publish.not_at_tag`](../errors/publish.not_at_tag.md)), runs
the bar here, then asks the registry to fetch, build and test that tag, and
waits for its answer. The registry checks the release with the release of
Jadpo it runs and the one your package declares. When it passes, the command
ends `release pending`: the release is checked, and waits for you.

On your [account page](https://jadpo.dev/account), the version is listed
under "Ready to publish". Open it to see what the registry found: what the
package can do, what changed, its changelog and its source. Publish it with
your passkey. Anyone can then install it:

```sh
jadpo package add @acme/tidying
```

`jadpo package publish --dry-run` asks the registry to check the tag without
listing anything to publish, and `jadpo package preview` serves the
package's page on your machine before you tag it.

## Release a new version

Change the code, run `jadpo package check`, which works out the new version
from what changed since the last tag and adds its section to `CHANGELOG.md`
for you to say why, then commit, tag and publish as before. A published
version is never changed: if one turns out wrong, yank it on its page, which
stops new installs of it while apps that have it keep working, and release a
fix.

## Read more

[Developing and publishing a package](../language/11-integrations.md#developing-and-publishing-a-package),
[What publishing requires](../language/11-integrations.md#what-publishing-requires),
[The package format](../language/11-integrations.md#the-package-format),
[The public registry](../language/11-integrations.md#the-public-registry),
[`jadpo package check`](../language/14-tooling.md#jadpo-package-check) and
[`jadpo package publish`](../language/14-tooling.md#jadpo-package-publish). To
install a package, see [Use a package from the registry](22-use-a-package-from-the-registry.md).

## Not built yet

An organisation with several members who confirm releases isn't built yet at
jadpo.dev, so a publisher name has one person. Nor are automatic release of
routine versions and urgent releases: every version waits for you to publish
it with your passkey.
