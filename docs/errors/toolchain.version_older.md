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
  reason: `jadpo:` names the release line the project is written for, 0.0, which is earlier than this jadpo's (`jadpo --version` shows it). A 0.x minor release may break an app, so this jadpo doesn't compile the project itself.
  next: Run it with Jadpo 0.0, which the installed `jadpo` fetches unless `JADPO_TOOLCHAIN` is `local` (human)
  rule: toolchain.version_older
  help: docs/errors/toolchain.version_older.md

check failed: 1 error, 1 file
```

When the installed `jadpo` couldn't fetch the release, its reason starts
with why, and its next step fits: run again once github.com can be
reached; name a published line; set `XDG_CACHE_HOME` when there is no
cache folder; or, for a download that failed its SHA-256 or attestation
check, run again to fetch it afresh, and report it if it fails the same
way. A project moving to a later
line changes `jadpo:` once its code is brought up to that line.
