# `toolchain.version_newer`

`app.jadpo` names the Jadpo release line the project is written for. The
installed `jadpo` runs that line's release for you
([Which jadpo runs](../language/14-tooling.md#which-jadpo-runs)); a `jadpo`
on an earlier line that can't, because it was told to run everything
itself (`JADPO_TOOLCHAIN=local`) or couldn't fetch the release, refuses the
project with this error alone, rather than misread what a later version
added:

```jadpo check project=e-version-newer file=app.jadpo expect=toolchain.version_newer
application {
    jadpo: "99.0"
}
```

```text output
error: This project needs Jadpo 99.0
 --> app.jadpo:2:12
  |
2 |     jadpo: "99.0"
  |            ^^^^^^ later than this jadpo
  reason: `jadpo:` names the release line the project is written for, 99.0, which is later than this jadpo's (`jadpo --version` shows it), so it stops here rather than misread what a later version added.
  next: Run it with Jadpo 99.0, which the installed `jadpo` fetches unless `JADPO_TOOLCHAIN` is `local` (human)
  rule: toolchain.version_newer
  help: docs/errors/toolchain.version_newer.md

check failed: 1 error, 1 file
```

When the installed `jadpo` couldn't fetch the release, its reason starts
with why, and its next step fits: run again once github.com can be
reached; name a published line; set `XDG_CACHE_HOME` when there is no
cache folder; or, for a download that failed its SHA-256 or attestation
check, run again to fetch it afresh, and report it if it fails the same
way.
