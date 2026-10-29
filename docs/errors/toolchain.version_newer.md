# `toolchain.version_newer`

`app.jadpo` gives the earliest Jadpo version the project needs. An older
Jadpo refuses the project with this error alone, rather than misread what
a later version added:

```jadpo check project=e-version-newer file=app.jadpo expect=toolchain.version_newer
application {
    jadpo: "99.0"
}
```

```text output
error: This project needs Jadpo 99.0 or later
 --> app.jadpo:2:12
  |
2 |     jadpo: "99.0"
  |            ^^^^^^ newer than this Jadpo
  reason: `jadpo:` is the earliest Jadpo version the project needs. This Jadpo is older, so it stops here rather than misread what a later version added.
  next: Upgrade Jadpo to 99.0 or later (`jadpo version` shows this one), then run the command again (human)
  rule: toolchain.version_newer
  help: docs/errors/toolchain.version_newer.md

check failed: 1 error, 1 file
```
