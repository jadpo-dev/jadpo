# Task timing and estimate calibration

**Started:** 2026-10-01. Measure agent work in session hours, human effort
separately, and external waiting separately. The original roadmap day estimates
were unsupported and have been withdrawn; do not convert them to hours by
multiplying by eight or applying a universal speed-up factor.

## What the existing records actually tell us

Chat turn metadata records `startedAt`, `completedAt` and `durationMs`. Git
records commit times, not task start times. The progress ledger mostly records
dates. Verifier reports record command durations, not implementation effort.

[Historical samples](historical-samples.json) preserve a purposive sample of
12 completed implementation turns from **Implement P10.6 and DX2** and
**Continue through the roadmap**, with thread/turn IDs, UTC start/end times,
recorded durations and scope limitations. No private transcript is copied.

| Observed work | Sessions | Range | Median |
|---|---:|---:|---:|
| Focused compiler/diagnostic fixes | 4 | 6.6–9.9 min | 7.2 min |
| Broader implementation slices | 6 | 36.7–69.1 min | 55.0 min |
| Mixed migration/formatter/verification batches | 2 | 104.2–115.8 min | 110.0 min |

Examples: initial golden source migration took 43.2 minutes; generated identity
plus Todo creation took 69.1 minutes; transaction/concurrency evidence took
66.3 minutes. Those are completed **slices**, not complete epics. The source
migration did not run the golden acceptance suite. The mixed batches include
unsuccessful attempts and unresolved contracts; their full duration is retained.

Turn duration includes reasoning, tool execution, tests and any within-turn
waiting. It excludes gaps between turns, and does not identify active versus
blocked time or allocate a mixed turn among tasks. Parallel agents, model,
reasoning settings, cache state and human effort are not consistently recorded.
These are observed session durations, not measured task labour or statistical
confidence intervals. They cannot establish a deadline for the remaining app.

The [verification samples](verification-samples.json) capture 29 successful full
reports available at calibration: summed measured commands range from 0.26 to
0.95 minutes, median 0.72 minutes. That excludes setup/reporting outside measured
steps and says nothing about time spent authoring or repairing tests. It is not
a whole-verifier wall-clock or clean-machine benchmark.

## Replacement estimates

Use these explicit planning bands for agent work, including focused validation
and documentation. They are deliberately wider than the small observed sample.

| Code | Work shape | Initial forecast | Basis |
|---|---|---|---|
| S | Focused fix or bounded decision preparation | 0.25–0.75h | Four 7–10 minute fixes, widened for investigation; decision preparation is an analogy |
| M | Bounded source/test/tooling slice | 0.5–2h | Six 37–69 minute cross-cutting sessions, widened for variation |
| L | Several connected slices or integration/debugging | 1–4h | Extrapolation from M and the two roughly two-hour mixed batches |
| N | New subsystem or substantial reference application | 2–8h provisional | Weak extrapolation; no complete comparable subsystem timing. Split and re-estimate after the first working slice |
| U | External reviews, user studies or final comparison campaigns | Unmeasured | No comparable completed evidence; measure a pilot and arrange participants first |

These are **agent session effort**, not engineer-days. Human decision time,
external scheduling and unattended soak duration are additional and unknown.
Decision-dependent implementation estimates assume the contract is settled;
preparation may proceed first. Dependencies between epics still apply. In
particular, E01's own estimated work excludes E02/E03/E04/E05/E06 prerequisites.
Do not sum incomplete estimates into a release date or call N/U work calibrated.

The [withdrawn estimates](withdrawn-estimates.json) retain the original task
forecasts for provenance. Do not use them as a planning baseline.

## 2026-10-01 forecast review

Reviewed all **90 open task forecasts** against the historical samples and the
local recorder snapshot. The [review evidence](estimate-review-2026-10-01.json)
preserves every prior/current forecast, its disposition and reason, source hashes,
and the observed runs. This is a point-in-time review; other delivery work can
continue adding records. No task scope, dependency, activation or planning state
changed as a result of this estimate review.

Only two evidence-backed task completions have local timer records:

| Completed task | Runs, including earlier attempts | Recorded active time | Original forecast |
|---|---:|---:|---|
| RM-101: golden obligation mapping | 2 | 16.11 min | 0.5–2h |
| RM-213: documentation audit and maintenance | 2 | 5.16 min | Unknown |

RM-101 finished below its forecast's 30-minute lower bound; RM-213 has no original
estimate to compare. Their model/effort settings are unknown. RM-213's separate
later roadmap reconciliation is META work, excluded here; five minutes is not a
forecast for all documentation work. Neither task proves complete runtime
integration speed. RM-501/RM-502 have completion markers but no local timer runs
in this snapshot, so they cannot supply measured task/estimate ratios.

The snapshot also contains 17 runs on nine still-open tasks: 16 partial runs and
one open timer, with the unclosed tail excluded. For example, RM-102 has 12.51
recorded minutes across six partial attempts; RM-601 has 4.92 across three.
These are effort observed so far, not completion times. Settings include
gpt-5.6-luna/xhigh, gpt-6-sol/high, gpt-6-astra/high and unknown values; task
selection and small samples prevent a model-speed comparison. Retain all attempts
and distinguish these phase timings from historical whole-turn durations.

**Decision:** retain S/M/L/N bands and all existing numeric task forecasts.
The five-comparable-completions threshold has not been reached. The historical
7–10-minute fixes, 37–69-minute slices and roughly two-hour mixed batches remain
the available implementation analogues; the new records support fast bounded
preparation but do not justify a blanket reduction in integration/subsystem work.

Added **14 provisional forecasts** where the task descriptions now bound an
outcome: RM-214–RM-219, RM-409, RM-508, RM-1005 and RM-1101–RM-1105. Each task's
analogy, uncertainty and relevant re-estimation point are in the review evidence.
M/L estimates for decision tasks include evidence and fixtures; implementation
explicitly reserved for later tasks is additional. N estimates remain weak
extrapolations, especially the exhaustive coverage inventory and new console/MCP
subsystems. Assess the first inventory or working slice before committing to the
rest; numeric estimates do not mean the tasks are planned or activated.

Twelve rows still lack a defensible whole-task numeric forecast: six external
review/study/trial tasks (U); five tasks whose selected extension, workload,
advisory pilot or commercial research scope remains unknown (RM-220, RM-408,
RM-410, RM-1106, RM-1107); and RM-212, whose S allowance is **per decision** with
no selected decision count. Their next estimation inputs are retained in the
review evidence. Conditional work remains outside non-conditional epic totals.
The [daily report](../progress/latest.md) computes current own-task totals from
the roadmap, without duplicating shared prerequisites or treating effort as a
calendar schedule.

## Record future work

Run from the repository root. This local tool uses Python's standard library
and a file lock on macOS/Linux. Each run has its own append-only JSONL file in
`docs/task-timing/runs/`, so independent agents need not edit one shared ledger.
Keep these records in version control with the completed work.

```sh
python3 tools/task-time.py start RM-102 --actor YOUR_CHAT_ID --category cross-cutting --estimate 1 4
# Copy the returned run ID into subsequent commands.
python3 tools/task-time.py phase RM-102-RETURNED_ID implementation
python3 tools/task-time.py phase RM-102-RETURNED_ID verification
python3 tools/task-time.py phase RM-102-RETURNED_ID blocked --note "Awaiting the named contract decision"
python3 tools/task-time.py phase RM-102-RETURNED_ID implementation
python3 tools/task-time.py finish RM-102-RETURNED_ID --outcome complete --evidence docs/implementation-history.md
python3 tools/task-time.py report --task RM-102
```

Replace `RM-102-RETURNED_ID` with the actual returned ID, not a fabricated one.
Use `--actor-kind human` for separately measured human work and `--thread` for
an optional chat reference. Omit an estimate when it is unknown. Use a `META-`
ID for planning/measurement work outside a roadmap task; exclude it from
implementation calibration. Add `--model gpt-6-luna --reasoning xhigh` to `start`
only when those are the actual selected settings; the example is not a default.
Omit unknown fields rather than copying the workflow's recommendation. Reports
include `model` and `reasoning`; older records retain null values. Record planning
depth and its reason in a phase note. Records are local and contain no credentials
or transcript content.

1. Start immediately before substantive work. Record the estimate before seeing
   the result. Use the same task ID for its later work, a unique actor per chat,
   and a new run per resumed session or rework attempt.
   If the model or effort changes, finish the old run with an honest disposition
   and start another under the same task ID with the new actual settings.
2. Switch to `verification` for tests and `review` for evidence/doc review.
   Verification/review are subsets of active time; never add them to active
   time again. `planning` and `implementation` also count as active.
3. Mark `blocked` when genuinely waiting on a named dependency. Mark `paused`
   before switching tasks or yielding for a non-work interval. Resume with the
   appropriate work phase. One actor must not run two active timers.
4. Finish with `complete`, `partial` or `abandoned`, and a report/file/commit or
   explicit disposition. Completing a run does not automatically close the
   roadmap task; its acceptance gate still governs. A verified sub-slice of an
   unfinished task should normally have outcome `partial`.
5. Never backfill a guessed start. If a session was interrupted, close it with
   `finish ... --outcome partial --unobserved-gap --evidence "Interrupted; last interval unknown"`.
   This preserves the last interval as unknown rather than counting an overnight
   gap as active work. Open records exclude their unfinished tail. Retain errors
   or corrections explicitly; do not rewrite historical events to improve a ratio.
6. Link run IDs in the task's completion/history entry. Keep retries and failed
   attempts under the same task ID. Sum their active time, not their repeated
   estimates. Overlapping agents contribute agent-hours; elapsed project time
   is the union of intervals, not the sum. Do not pool human and agent hours.

Historical closeout annotations may repeat `finished` with the same outcome.
The recorder retains those events and their latest evidence, but the first
finish ends the measured run. Later annotation intervals appear separately as
`post_close_annotation_minutes`; they add no active or elapsed task time.
A later work phase or different completion outcome is an error, not an implicit
reopened session. Start a fresh run for additional work.

## Learn from the records

After five fully measured task completions in a comparable category, compare
original forecast with total active time across **all** its runs, including
rework. Exclude tasks with unknown intervals from precise actual/estimate
ratios, but report those missing observations and blocked/abandoned work.
Separate bounded fixes, source-only migrations, complete runtime integration,
new subsystems and external studies. Report sample size, median, range and
forecast misses; keep model/concurrency/environment changes visible.

Revise remaining forecasts from comparable work, retaining the old forecasts
and the reason for the revision. Do not silently change the original estimate
or lower the completion standard to make estimates appear accurate. Scheduling
waits remain a separate forecast. This workflow collects evidence; it does not
automatically time another already-running chat or enforce acceptance gates.
