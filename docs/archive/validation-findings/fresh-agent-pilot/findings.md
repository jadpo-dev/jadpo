# Internal cold-start pilot findings

One context-isolated agent implemented the predeclared SQLite card service using
only the frozen compiler and supplied public docs. The prompt, requirements,
grader and documentation were hashed before dispatch; the parent checked their
hashes again at submission. A parent-authored control passed all twelve grader
obligations before the participant began. The participant never saw that control
or grader implementation, received no parent hints and had no grader feedback
before submission.

Evidence: `build/validation/fresh-agent-pilot/20260930-wave3/` contains the frozen
input manifest, exact prompt, source, four compiler invocation logs, participant
report, parent build output and hidden-grader results. A fresh disposable SQLite
file was used for grading. Every one of the **12 acceptance obligations passed**,
including omitted versus null patches, rejection before mutation, exact domain
failure codes, unchanged state after invalid writes and persistence across a
fresh process.

The participant reported approximately **4 minutes 8 seconds** from first read
to submission and used **four compiler invocations**: a rejected check, successful
check, successful build, and a `test` invocation that correctly reported no
authored tests. The participant separately reported 32 ad hoc HTTP assertions
and a restart assertion; the parent result is the independently executed frozen
12-obligation grader, not an inference from that self-report. Exact model
invocation metadata, token use and cost were unavailable and remain unreported.

The initial error was a real project-role violation: failures had been placed
beside valid type declarations under `values/`. The compiler correctly rejected
that layout, but highlighted the beginning of the file instead of the offending
failure declaration. The participant moved failures to root `app.jadpo` and
continued without a hint. After scoring, seven regression cases drove a fix
that highlights the actual invalid declaration while preserving file-wide
locations for aggregate entity-count errors. Placement rules are unchanged.

The frozen documentation also mixed historical type/persist and top-level CRUD
examples with the accepted entity model, omitted clear placement advice for
shared failures, and did not describe executable authored tests. Follow-up docs
now identify compatibility examples explicitly, describe current failure/test
placement, show authored test/fixture syntax, and align the time prelude with
the existing implementation. The minimal documented test was executed and
passed. These are post-pilot corrections: the scored frozen inputs remain
unchanged, and no improved-docs success rate is claimed.

This is one small internal cold-start probe. It does not satisfy P10R approval,
P12's counterbalanced multi-trial TypeScript comparison, first-user review or
human comprehension thresholds. There was no randomization, human participant,
independent formal adjudicator, production workload, authentication requirement,
or Wasm target. Its successful outcome supports only this bounded task and the
recorded usability findings.
