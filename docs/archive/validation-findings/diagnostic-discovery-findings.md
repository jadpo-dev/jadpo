# Diagnostic discovery and local operational errors

The prior catalogue scanner omitted CONFIG, POLICY and TEST prefixes, skipped
the source directory `core/src/target/`, and treated diagnostic-looking text in
comments or helper functions after a test as test evidence. Correcting discovery
exposed missing references and three placeholder test-command messages. The
new inventory contains **480 codes**. Public copy is authored for all of them;
configuration/policy wording also passed the existing plain-language checks.

Discovery now parses Rust string literals, distinguishes production from
`#[test]`/test-only cfg code, ignores documentation attributes and integration
tests as emitters, and includes source target modules. Test references stay
within real test bodies and exclude ignored tests and nested helper declarations.
Compile fixture references come from the parsed diagnostics array, regardless
of whitespace/layout, rather than any line containing a `code` field. Five
focused discovery tests cover these boundaries. Existing serde_json and syn
versions are reused from the lockfile for build/test tooling; generated runtime
dependencies are unchanged.

A reference is not proof of an executed assertion, reachable emitter, branch
coverage or arbitrary macro expansion. The guide now states this limitation
instead of calling the static index exhaustive trigger evidence. Real closure
cases comprise 16 policy tests, six configuration tests, four local filesystem
tests, three CLI test-process cases and the strengthened config-check CLI case.
They trigger actual source/API/process paths. A missing runtime, absent authored
tests and unsuccessful test child are distinct CLI outcomes. The process-status
case uses a controlled child exit and does not claim to execute failing business
logic; separate authored fixture/runtime tests cover that behavior.

Independent review then reproduced a configuration staging race and false
references from ignored tests/nested helpers/compound test cfg. Those were fixed,
and the common gate executes their regressions. The configuration findings
retain the final check/rename atomic-CAS limitation. See `closure-findings.md`
for before/after public-API race evidence, and `diagnostic-mutation-findings.md`
for two compiling mutants caught by unchanged assertions with passing baselines
and restored sources.
