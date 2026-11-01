# `cli.package_unreadable`

A file given to `jadpo package import` isn't there, can't be read, or isn't JSON.

## Why Jadpo refuses it

`jadpo package import openapi <file>` turns a provider's OpenAPI
document into a package of the app's own, in `shared/providers/`. It
reads the whole document before it writes anything, so a file it can't
find or can't read as JSON writes nothing: half a package would check
against an API the provider doesn't have.

It reads JSON only for now. A YAML document is refused rather than
guessed at.

## What it looks like

A project importing Tidepool's API from the YAML its documentation
links:

```jadpo check project=e-package-unreadable file=app.jadpo hidden
application {}
```

```sh run project=e-package-unreadable expect=cli.package_unreadable
cat > tidepool.yaml <<'DOC'
openapi: 3.1.0
info:
  title: Tidepool
  version: "1"
paths: {}
DOC
jadpo package import openapi tidepool.yaml
```

```text output
error: `tidepool.yaml` isn't JSON
  reason: The importer reads an OpenAPI document as JSON; YAML isn't read yet.
  next: Convert the document to JSON, then import that (agent)
  rule: cli.package_unreadable
  help: https://jadpo.dev/docs/errors/cli.package_unreadable
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

A path with no file at it is refused too:

```sh run project=e-package-unreadable expect=cli.package_unreadable
jadpo package import openapi tidepool.json
```

```text output
error: `tidepool.json` can't be read
  reason: There's no file at this path, from the folder the command runs in.
  next: Give the path of the OpenAPI document, from this folder (agent)
  rule: cli.package_unreadable
  help: https://jadpo.dev/docs/errors/cli.package_unreadable
```

## How to fix it

**If the provider publishes JSON too**, download that document and
import it:

```sh
jadpo package import openapi tidepool.json
```

The package is written in `shared/providers/tidepool/v1/`, and the
screen lists its operations and types.

**If it publishes only YAML**, convert the document to JSON with a YAML
tool, then import the JSON:

```sh
yq -o=json tidepool.yaml > tidepool.json
jadpo package import openapi tidepool.json
```

The import reads the JSON as it would the provider's own.

## Related

- [`jadpo package import`](../language/14-tooling.md#jadpo-package-import)
- [Commands for packages](../language/14-tooling.md#commands-for-packages)
