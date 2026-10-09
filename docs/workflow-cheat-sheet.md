# Roadmap and delivery cheat sheet

Use these prompts in the project's chat. Replace bracketed text with your idea,
task ID or epic. The four personal skills work across projects.

## Choose a skill

| Command | What it does | Use it when |
|---|---|---|
| `$roadmap-add` | Captures an idea with an ID, scope, dependencies and planning needs; checks for duplicates | You think of something to do later |
| `$roadmap-tidy` | Organises epics/tasks, separates completed history and checks estimates/dependencies | The roadmap is cluttered or hard to follow |
| `$adaptive-delivery` | Checks the active model, routes planning/implementation/review tasks and verifies results | You want to get roadmap work done |
| `$verify-loop` | Establishes a measurable target and trustworthy verifier, then iterates against evidence | Correctness, reliability, performance or fidelity needs sustained investigation |

## Capture and organise

**Add an idea**

```text
Use $roadmap-add to add this idea: [describe it].
```

After discussing an idea, “Add that to the roadmap” is enough. Adding it does
not start implementation.

**Tidy the roadmap**

```text
Use $roadmap-tidy to tidy this project's roadmap.
```

## Plan and implement

**Plan one batch**

```text
Use $adaptive-delivery to plan the next batch. Include model recommendations. Do not implement.
```

**Plan unattended**

```text
Create a planning Goal using $adaptive-delivery for [epic or scope]. Keep preparing and checking saved task plans until complete or you need my input. Do not implement.
```

**Implement unattended**

```text
Create an implementation Goal using $adaptive-delivery for [epic or scope]. Implement, verify and record each task, then continue until complete or you need my input. Use $verify-loop where appropriate.
```

**Implement one task**

```text
Use $adaptive-delivery to implement [task ID].
```

**Review one task**

```text
Use $adaptive-delivery to review [task ID] and report findings.
```

At batch entry, adaptive delivery checks actual model/effort against the lowest
sufficient setting for the hardest expected work in that bounded batch. It
retains that pin through ordinary planning, implementation, verification and
self-review transitions. It reassesses at a batch boundary, material scope
change or evidence that the pin is insufficient. A mismatch in either direction
at entry requires a switch or scoped override; unknown entry settings require
confirmation. A later incomplete metadata reading alone does not invalidate an
established pin. Planning/completion states stay separate from model deferral;
recommendations do not automatically change your model.

After switching, say **“Continue”** to retain the scope and resume deferred work.
**“Use this model anyway for [scope]”** is an explicit routing override. “Continue”
without a switch does not override a remaining mismatch. Model mismatches always
pause even when ordinary task questions permit independent work.

Loops ask about blockers once and continue independent work where possible.
Add **“stop at the first question”** if you prefer. Planning and implementation
are separate: a planning loop never starts implementation by itself.

**Continue authorised parallel delivery**

```text
Continue the golden Todo plan in parallel. Use separate worktrees, one owner for shared compiler/runtime changes, and independent harness work where dependencies allow. Verify and integrate through PRs.
```

Normal chat prompts are enough; you do not need to manage branches or invoke a
skill explicitly. Parallel agents, other-chat messages and publishing require
their own authorisation. Before assigning work, check whether another delivery
chat already owns the same scope. Give an independent lane its own worktree and
bounded ownership; a second chat is not a second writer on the same checkout.
An ordinary loop runs within the current turn. Request a Goal explicitly when
you want its outcome to persist across turns; a saved plan alone does not resume
work.

A planning Goal produces saved documents in ordinary task execution. Native
Plan mode does not itself trigger automatic Goal continuation; the host must
support the requested continuation. See the [official Goals guide](https://developers.openai.com/cookbook/examples/codex/using_goals_in_codex).

## Run a measured improvement campaign

```text
Use $verify-loop to improve [behaviour or component].
```

The skill uses the conversation to propose a target, confirms the target and
actual model/effort for a new campaign, then measures and iterates. Ordinary
implementation keeps its normal tests; it does not need a campaign every time.
Model recommendations do not automatically change your selected settings.

## Control a Goal

These are app commands, not skills. A Goal keeps a defined outcome active across
turns; a normal skill prompt does not by itself schedule future turns.

| Command | Action |
|---|---|
| `/goal` | View the current Goal |
| `/goal [outcome and completion criteria]` | Set a Goal |
| `/goal pause` | Pause it |
| `/goal resume` | Resume it |
| `/goal clear` | Remove it |

## Check progress

```text
Show me the latest project progress report.
```

Jadpo's report refreshes daily at **9am London time**. It includes delivered
work, task counts, planning status, blockers, epic estimates and daily changes.

[Latest report](progress/latest.md) · [Roadmap](implementation-roadmap.md) ·
[Project workflow](roadmap-workflow.md)

For unscoped Jadpo work, the roadmap's current first priority is **RM-213**, the
documentation audit, while that task remains open. Explicit task/epic requests
keep their own scope.
