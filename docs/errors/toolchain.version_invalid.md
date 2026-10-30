# `toolchain.version_invalid`

`jadpo:` is a release line: a major and a minor number as text. A release
that only fixes something stays on its line, so the patch number is left
out:

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
