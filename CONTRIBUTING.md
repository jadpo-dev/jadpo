# Contributing to Jadpo

Jadpo is experimental. The [roadmap](docs/implementation-roadmap.md) records
remaining work; the [history](docs/implementation-history.md) records completed
capabilities and their evidence. The full golden application remains unfinished.
Licensing has not yet been selected.

## Setup and checks

Clone [richardwillars/jadpo](https://github.com/richardwillars/jadpo) and build
the compiler from the repository root:

```sh
cargo build --locked --manifest-path jadpo/Cargo.toml -p jadpo-cli
cargo run --locked --manifest-path jadpo/Cargo.toml -p jadpo-cli -- check examples/jadpo-seed
```

The supported gate is `python3 tools/verify.py`. It requires Rust 1.78,
Bun 1.2.20, Node.js, Python, Ruby, Bash and PostgreSQL tools; see the
[validation guide](tests/validation/README.md) for the complete setup and suite
boundaries. Focused checks are useful during implementation. Run the required
full gate on the integrated revision before landing compiler/runtime changes.
Use `--require-golden` when claiming full golden completion; ordinary supported
checks explicitly retain the open golden gates.

## Branches and worktrees

`main` is the shared integration baseline. New work uses a `codex/` branch and
a pull request. GitHub requires an up-to-date passing `supported-language`
check and resolved review conversations before merging, including for
administrators. Force pushes and deletion of `main` are disabled. There is no
mandatory human approval count; required independent project reviews remain
separate acceptance evidence.

Give each editing agent its own worktree and a bounded task,
with explicit ownership of the files it may change. For example:

```sh
git fetch origin
git worktree add -b codex/rm-109-harness ../jadpo-rm-109-harness origin/main
```

A worktree starts from committed source. Commit or explicitly transfer needed
prerequisite changes before assigning dependent work; an ordinary Git worktree
does not include another checkout's uncommitted edits. Recreate ignored setup
files and dependencies in the new worktree as needed. Keep secrets and local
caches out of Git. Checked-in service-adapter TLS keys are deliberately public
test fixtures for `.invalid` hosts.

One integration owner coordinates changes to shared compiler/runtime files,
the verifier manifest and roadmap records. Parallel test authors can prepare
cases from accepted contracts before dependent implementation finishes; those
tasks stay open until their required runtime evidence exists. A reviewer examines
a fixed commit or captured source revision and records the scope of that review.
Check the [project workflow](docs/roadmap-workflow.md) and applicable `AGENTS.md`
before roadmap delivery; required independent reviews still apply.

## Commits and pull requests

Commit coherent checkpoints with descriptive messages and task IDs when relevant.
Keep implementation, its tests and the necessary owning documentation together.
Preserve original acceptance conditions and distinguish partial work from task
completion. Avoid carrying unrelated changes into a task branch.

Open a pull request to `main` describing the resulting behaviour, validation and
remaining limitations. Link required review evidence and identify its reviewed
revision. The integration owner resolves conflicts and reruns affected checks
after integration; passing tests on separate branches do not establish that the
combined revision works. GitHub Actions runs the supported gate on pull requests
and pushes to `main` and retains its reports. Feature-branch pushes are covered
by their pull request, avoiding a duplicate full run for the same update.

Use normal merges or fast-forward updates; preserve existing history. Rewriting
or force-pushing shared branches requires an explicit reason and owner approval.
Remove a worktree only after its changes are committed and integrated or safely
preserved. Do not reset or clean another agent's checkout.
