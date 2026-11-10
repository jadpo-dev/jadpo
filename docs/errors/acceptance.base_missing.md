# `acceptance.base_missing`

This clone doesn't hold the commit on the default branch that the change is compared with.

## Why Jadpo refuses it

`jadpo check` works out what needs a person's acceptance by comparing the
change with the default branch, at the commit this branch left it. A clone
made without that commit has nothing to compare with, such as CI's
depth-1 checkout. Passing anyway would let through a change nobody
accepted, so the check fails instead.

Nothing in the project is wrong, so Jadpo doesn't ask you to change it. The
clone needs its history.

## What it looks like

A project's branch, `change`, cloned at a depth of 1, so the clone holds
neither the default branch nor the commit the branch left it at:

```jadpo check project=e-base-missing file=app.jadpo hidden
application {}
```

```sh run project=e-base-missing expect=acceptance.base_missing
export GIT_AUTHOR_NAME=Ada GIT_AUTHOR_EMAIL=ada@example.com GIT_COMMITTER_NAME=Ada GIT_COMMITTER_EMAIL=ada@example.com
export GIT_AUTHOR_DATE=2026-10-20T14:00:00Z GIT_COMMITTER_DATE=2026-10-20T14:00:00Z
git init -q -b main
git add app.jadpo
git commit -q -m "chore: start the project"
git switch -q -c change
git commit -q --allow-empty -m "feat: change something"
git clone -q --depth 1 --branch change "file://$PWD" ../clone
cd ../clone
jadpo check
```

```text output
error: The base to compare with, the default branch, isn't in this clone
  reason: `jadpo check` compares the change with the default branch at its merge base, and this clone was made without that commit, so what needs a person's acceptance can't be worked out.
  next: Fetch the history with `git fetch --unshallow`; in CI, check out with `fetch-depth: 0` (agent)
  rule: acceptance.base_missing
  help: https://jadpo.dev/docs/errors/acceptance.base_missing

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you are in a clone on your own machine**, fetch the rest of its
history and the default branch:

```sh
git fetch --unshallow
git fetch origin main:refs/remotes/origin/main
jadpo check
```

The base commit is now in the clone, so `jadpo check` compares the change
with it and judges the acceptances.

**If this is CI**, check out the whole history. With GitHub Actions:

```yaml
- uses: actions/checkout@v4
  with:
    fetch-depth: 0
```

Every run of the check then has the base commit to compare with.

## Related

- [Acceptances](../language/18-records-and-manifest.md#acceptances)
- [Comparing two builds](../language/18-records-and-manifest.md#comparing-two-builds)
- [`jadpo check`](../language/14-tooling.md#jadpo-check)
