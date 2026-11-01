# `cli.package_not_openapi`

A file given to `jadpo package import` is JSON but not an OpenAPI 3.0 or 3.1 document.

## Why Jadpo refuses it

`jadpo package import openapi <file>` reads the operations and types of
an OpenAPI 3.0 or 3.1 document, and nothing else. Swagger 2.0, the
version before OpenAPI 3, lays its operations and types out
differently, so read as OpenAPI it would give a package of the wrong
calls. Nothing is written.

## What it looks like

The provider's document is Swagger 2.0:

```jadpo check project=e-package-not-openapi file=app.jadpo hidden
application {
    jadpo: "0.2"
}
```

```sh run project=e-package-not-openapi expect=cli.package_not_openapi
cat > tidepool.json <<'DOC'
{
  "swagger": "2.0",
  "info": { "title": "Tidepool", "version": "1" },
  "paths": {}
}
DOC
jadpo package import openapi tidepool.json
```

```text output
error: `tidepool.json` isn't an OpenAPI document the importer reads
  reason: The importer reads OpenAPI 3.0 and 3.1, and this is a Swagger 2.0 document, the version before OpenAPI 3.
  next: Give the provider's OpenAPI 3 document (agent)
  rule: cli.package_not_openapi
  help: https://jadpo.dev/docs/errors/cli.package_not_openapi
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

A JSON file that says no version at all, or another version than 3, is
refused the same way, with what it says.

## How to fix it

**If the provider publishes an OpenAPI 3 document too**, import that
one:

```sh
jadpo package import openapi tidepool-openapi3.json
```

The package is written in `shared/providers/tidepool/v1/`.

**If it publishes only Swagger 2.0**, convert the document to OpenAPI 3
with a converter such as `swagger2openapi`, then import the result:

```sh
npx swagger2openapi tidepool.json -o tidepool-openapi3.json
jadpo package import openapi tidepool-openapi3.json
```

The converted document's operations become the package's, and its
provenance names the converted file.

## Related

- [`jadpo package import`](../language/14-tooling.md#jadpo-package-import)
- [Commands for packages](../language/14-tooling.md#commands-for-packages)
