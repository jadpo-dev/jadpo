# `toolchain.version_invalid`

An application's `jadpo:` isn't a major and minor version as text.

## Why Jadpo refuses it

`jadpo:` names the Jadpo release line the project is written for, so the
right compiler runs it. It is a line and not a release: while Jadpo is at
0.x a minor release may break apps, so the line is what must match, and a
release that only fixes something stays on its line. A patch number would
say something the line doesn't mean.

## What it looks like

```jadpo check project=e-version-invalid file=app.jadpo expect=toolchain.version_invalid
application {
    jadpo: "0.1.0"
}
```

```text output
error: `"0.1.0"` isn't a Jadpo version
 --> app.jadpo:2:12
  |
2 |     jadpo: "0.1.0"
  |            ^^^^^^^ not a major and minor number as text
  reason: `jadpo:` names the Jadpo release line the project is written for: its major and minor numbers as text, such as `"0.1"`. A release that only fixes something stays on its line, so the patch number is left out.
  next: Write the line as its major and minor numbers in quotes, such as `jadpo: "0.1"` (agent)
  rule: toolchain.version_invalid
  help: https://jadpo.dev/docs/errors/toolchain.version_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A version that isn't in quotes is refused the same way, because a line is
text.

## How to fix it

**If it names a whole release, or has no quotes**, write the line as its
major and minor numbers, in quotes:

```jadpo check project=e-version-invalid-line file=app.jadpo
application {
    jadpo: "X.Y"
}
```

Jadpo now runs this project with the newest release of that line it keeps
or can fetch.

**If the project shouldn't name a line**, leave `jadpo:` out:

```jadpo check project=e-version-invalid-none file=app.jadpo
application {}
```

With the line left out, nothing is checked, and whichever `jadpo` is
installed compiles the project.

## Related

- [The root](../language/01-project-layout-and-modules.md#the-root)
- [Which jadpo runs](../language/14-tooling.md#which-jadpo-runs)
