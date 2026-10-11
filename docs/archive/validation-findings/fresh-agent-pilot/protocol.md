# Internal cold-start agent pilot

This is one bounded usability probe, not the P10R/P12 comparison, a human
comprehension study, or evidence that Jadpo outperforms TypeScript. The agent
starts without the parent conversation and receives the task and a frozen
subset of public repository documentation. The compiler and documentation are
hashed before the participant begins. Existing compiler implementation, tests,
examples and generated application code are excluded as learning inputs.

The participant has at most 20 minutes and 20 compiler invocations. Every
invocation goes through a logging wrapper. The participant may author tests but
may not edit the compiler or generated output. No parent hints or hidden grader
feedback are provided before the first submission. Missing or ambiguous docs
must be recorded. The parent independently builds and grades the submitted
source against the predeclared JSON requirements and black-box check script.

Retain source, exact prompt, input digests, compiler/runtime versions, attempt
commands/status/output/durations, submitted findings and grader results under a
fresh `build/validation/fresh-agent-pilot/<run>/` directory. Record unavailable
model/token/cost metrics as unavailable. Wall time includes reading and repair;
compiler attempt durations are separate. A task-specific duration cap is a
stopping rule, not a performance target.

Acceptance is binary per listed obligation; all must pass for this one task to
pass. Unexpected failures remain failures. Do not edit the grader after seeing
participant output; any evaluator defect must be recorded and a corrected run
clearly distinguished. Record syntax/semantic/runtime problems separately.
Even complete success leaves the formal randomized/counterbalanced comparative
trials, independent adjudication and human comprehension gates open.
