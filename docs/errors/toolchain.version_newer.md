# `toolchain.version_newer`

The project is written for a later Jadpo release line than the `jadpo` checking it, and that `jadpo` can't run the later release instead.

## Why Jadpo refuses it

`jadpo:` in `app.jadpo` names the release line the project is written
for. Normally the installed `jadpo` runs that line's release for you,
fetching it once if it doesn't keep it. You see this error when it
can't: you set `JADPO_TOOLCHAIN=local`, so it runs every command
itself, or it couldn't fetch the release.

An earlier `jadpo` doesn't know what a later line added, so it stops
rather than misread the project. Jadpo won't move the project to
another line for you, because the line decides which version of the
language the code follows.

## What it looks like

app.jadpo:

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
  help: https://jadpo.dev/docs/errors/toolchain.version_newer

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which Jadpo release line the project is written for.

When the installed `jadpo` tried to fetch the release and couldn't, the
reason starts with why, and the next step fits the cause:

- GitHub couldn't be reached: run the command again once this machine
  can reach github.com.
- GitHub's API refused for its hourly limit: run the command again in
  an hour.
- No release of the line is published: name a published line in
  `jadpo:`.
- There is no cache folder to keep the release in: set
  `XDG_CACHE_HOME` to one.
- The download failed its SHA-256 or attestation check: run the command
  again to fetch it afresh, and if it fails the same way, install the
  newest `jadpo`, whose trusted root may be newer.

When `jadpo.lock` pins an exact release, the error names that release,
and the reason says the lock pins it. If no such release is published,
the next step is to run `jadpo toolchain unpin` or pin a published one.

## How to fix it

**If you set `JADPO_TOOLCHAIN=local`**, run the command without it:

```sh
unset JADPO_TOOLCHAIN
jadpo check
```

The installed `jadpo` then runs the release the project names, fetching
it once if it doesn't keep it, and the command's own output follows.

**If the project should be on this `jadpo`'s line**, name that line, the
major and minor numbers `jadpo --version` shows. In `app.jadpo`:

```jadpo fragment
    jadpo: "0.2"
```

```jadpo check project=e-version-newer-fix-1 file=app.jadpo hidden
application {
    jadpo: "0.2"
}
```

This `jadpo` now checks and runs the project itself.

## Related

- [Which jadpo runs](../language/14-tooling.md#which-jadpo-runs)
- [Fetching a release](../language/14-tooling.md#fetching-a-release)
- [When the release can't be run](../language/14-tooling.md#when-the-release-cant-be-run)
- [Run your own build on every project](../language/14-tooling.md#run-your-own-build-on-every-project)
- [The root](../language/01-project-layout-and-modules.md#the-root)
