# First-user structured review guide

**Status:** candidate P10R research instrument  
**Required sample:** at least five participants matching the screening criteria

This review tests the proposed user, problem, trust objections, and smallest
adoption path. It does not prove compiler safety and must not be presented as a
sales demonstration.

## 1. Screening

Include participants who:

- have recent professional responsibility for SaaS/backend delivery;
- use coding agents for material implementation work, not only completion;
- personally review agent-generated changes or remain accountable for their
  security/reliability; and
- can discuss a greenfield or newly isolated service decision.

Record partial matches rather than quietly broadening the profile. Exclude the
project author and anyone who helped design the language from the five-person
minimum. Do not collect employer-confidential source, credentials, customer
data, or unnecessary personal identifiers.

## 2. Session structure

Target 50–60 minutes:

1. **Context without pitch (10 minutes):** current agent workflow, review volume,
   incidents, controls, and ownership.
2. **Current alternative (10 minutes):** strongest real TypeScript/framework
   workflow, not a hypothetical weak stack.
3. **Artifact tasks (20 minutes):** inspect the golden todo source, expected
   audit, and one behavioural policy change without coaching.
4. **Adoption/trust discussion (10 minutes):** switching cost, interoperability,
   debugging, escape hatches, operational ownership, and approval flow.
5. **Disconfirmation (10 minutes):** strongest reason the product is unnecessary
   or actively harmful; what evidence would change that view.

Ask permission to retain notes. Store a pseudonymous participant ID and broad
role/experience bands only.

## 3. Core questions

Ask these before explaining the proposed solution:

1. Describe the last agent-generated backend change you could not confidently
   review. What made confidence difficult?
2. Which risks do your present compiler, schemas, tests, policy layer, CI, and
   review process actually prevent? Which merely detect?
3. Who is accountable when generated code changes authorization, data lifecycle,
   secrets, or external effects?
4. How often does review become a plausibility check because the diff is too
   large or behaviour is distributed?
5. What is the strongest version of your current TypeScript solution?

After artifact tasks:

6. What can the owner, another authenticated user, and an unauthenticated caller
   do?
7. What happens when a todo, user, due date, reminder, or credential changes?
8. Which claims appear proved, runtime validated, test-supported, assumed, or
   unresolved? Note any category confusion.
9. For “make the list public,” what information would you require before making
   or approving the decision?
10. What important behaviour is still hard to find or distrust?
11. Would a new isolated service be a credible starting point? What switching
    cost is acceptable?
12. Which missing ecosystem, debugging, deployment, or escape capability makes
    adoption implausible?
13. Could a strong TypeScript framework provide the same result? Be specific.
14. What is the strongest reason to stop or pivot this project?

## 4. Artifact tasks

Do not teach the answers. Record time, answer, confidence, clarification request,
and evidence path used.

- Identify every public route and protected route.
- Determine whether Alice can read or patch Bob's todo.
- Explain omission versus `none` in the due-date patch.
- Explain what happens to todos when a user is deleted/disabled.
- Trace the reminder email's recipient, secret, retry, and idempotency behavior.
- Review adversarial change C09 (“make the list public”) and state whether it is
  implementable, rejected, or requires a decision.
- Find one claim that depends on runtime/operations rather than static proof.
- Find one residual risk the proposed artifacts do not eliminate.

## 5. Measures

Capture:

- screening fit and current toolchain;
- unaided correctness for each artifact task;
- time and confidence per task;
- risks noticed and important risks missed;
- false-confidence instances;
- clarification requests;
- pain severity and frequency, each on a 1–5 anchored scale;
- usefulness of source, audit, and decision view separately;
- acceptable initial adoption/switching cost;
- blocking trust and interoperability objections;
- preference for new language, TypeScript framework, either, or neither;
- verbatim short disconfirming observations where consent permits; and
- interviewer interpretation separately from participant statements.

Anchors: `1` means absent/not useful/not credible; `3` means material but
workaround exists; `5` means frequent/decisive/credible enough to change tools.

## 6. Bias controls

- Describe this as a hypothesis being tested, not a product being validated.
- Ask about concrete past behavior before future interest.
- Present the TypeScript counter-hypothesis explicitly.
- Do not count compliments, feature ideas, or willingness to “keep in touch” as
  adoption evidence.
- Preserve negative and ambiguous evidence.
- Do not change questions or artifacts midway without versioning the instrument;
  report pre/post-change sessions separately.
- Have someone other than the product advocate review scoring when practical.

## 7. Synthesis rule

After five qualifying sessions, publish the individual pseudonymous records and
a synthesis that states profile fit, common pain, contrary evidence, trust
objections, comprehension errors, switching-cost range, and whether the initial
user hypothesis is retained, narrowed, replaced, or rejected.

The gate is not “five interviews completed.” It is a recorded decision that
confronts disconfirming evidence and does not relabel product interest as proof
of the assurance model.

