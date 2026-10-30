# `toolchain.version_older`

`app.jadpo` names the Jadpo release line the project is written for, and
while Jadpo is at 0.x a minor release may break an app, so a later `jadpo`
doesn't compile a project on an earlier line. The installed `jadpo` runs
that line's release for you
([Which jadpo runs](../language/14-tooling.md#which-jadpo-runs)); one that
can't, because it was told to run everything itself
(`JADPO_TOOLCHAIN=local`) or couldn't fetch the release, refuses the
project with this error alone:

```jadpo check project=e-version-older file=app.jadpo expect=toolchain.version_older
application {
    jadpo: "0.0"
}
```

```text output
error: This project needs Jadpo 0.0
 --> app.jadpo:2:12
  |
2 |     jadpo: "0.0"
  |            ^^^^^ earlier than this jadpo
  reason: `jadpo:` names the Jadpo release line the project is written for, and this jadpo is on a later one (`jadpo --version` shows it). While Jadpo is at 0.x a minor release may break an app, so it doesn't compile the project.
  next: Install Jadpo 0.0 from https://github.com/jadpo-dev/jadpo/releases, then run the command again (human)
  rule: toolchain.version_older
  help: docs/errors/toolchain.version_older.md

check failed: 1 error, 1 file
```

When the launcher couldn't fetch the release, the reason ends with why:
offline, no release of the line published yet, or a download that didn't
match its published SHA-256 or attestation. A project moving to a later
line changes `jadpo:` once its code is brought up to that line.
