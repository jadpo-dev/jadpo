//! Whether a Rust test's live PostgreSQL part runs (docs/compiler-runtime.md
//! §2.7, chunk 5.8): one helper every test file with a live part calls, so
//! a skip is always said and never silent, and the gate finds those files
//! by their call to [`live`] and gives each one the run's server.
//!
//! `JADPO_SKIP_POSTGRES=1` (`tools/verify.py --profile quick`) skips the
//! live part, saying so on a `skipped:` line. Where every live test must
//! run, a skip fails the test instead: under CI (`CI` set), and in the
//! gate's full profile (`JADPO_REQUIRE_LIVE=1`), which gives every such
//! file a server, so a live test told to skip there is one the gate missed.
//!
//! A server too old for one test (Postgres.app's pgvector 0.7.0, say) is
//! [`skip_for_version`]: said and skipped on a developer's machine, even in
//! the full profile, and a failure under CI, whose image has every version
//! the tests need (tools/fixtures/postgres/Dockerfile).

use std::fmt::Display;

/// Set to `1`, the live parts of tests are skipped, each saying so.
pub const SKIP_VARIABLE: &str = "JADPO_SKIP_POSTGRES";

/// Set to `1` by the gate's full profile: no live part may be skipped.
pub const REQUIRE_VARIABLE: &str = "JADPO_REQUIRE_LIVE";

/// What a test with a live part does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Decision {
    Run,
    Skip,
    Fail,
}

/// The decision for a run whose variables say whether to skip, whether
/// it is CI, and whether the gate requires every live test.
fn decide(skip: bool, ci: bool, required: bool) -> Decision {
    match (skip, ci || required) {
        (false, _) => Decision::Run,
        (true, false) => Decision::Skip,
        (true, true) => Decision::Fail,
    }
}

fn set(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| value == "1")
}

fn ci() -> bool {
    std::env::var_os("CI").is_some_and(|value| !value.is_empty())
}

/// Whether this test's live part runs: `false`, after a `skipped:` line,
/// when `JADPO_SKIP_POSTGRES=1`; a panic when that skip isn't allowed.
pub fn live() -> bool {
    match decide(set(SKIP_VARIABLE), ci(), set(REQUIRE_VARIABLE)) {
        Decision::Run => true,
        Decision::Skip => {
            println!("skipped: {SKIP_VARIABLE} is set");
            false
        }
        Decision::Fail => panic!(
            "a live PostgreSQL test was told to skip ({SKIP_VARIABLE}=1) in a run where every live \
             test must run: tools/verify.py gives each test file that calls `jadpo_test::live()` \
             the run's server, so this file wasn't found or wasn't given it"
        ),
    }
}

/// A live test the server can't serve, `reason` saying why: a `skipped:`
/// line on a developer's machine, and a panic under CI.
pub fn skip_for_version(reason: impl Display) {
    assert!(
        !ci(),
        "skipped under CI, whose PostgreSQL has every version the tests need \
         (tools/fixtures/postgres/Dockerfile): {reason}"
    );
    println!("skipped: {reason}");
}

#[cfg(test)]
mod tests {
    use super::{Decision, decide};

    #[test]
    fn a_skip_is_allowed_only_outside_ci_and_the_full_gate() {
        assert_eq!(decide(false, false, false), Decision::Run);
        assert_eq!(decide(false, true, true), Decision::Run);
        assert_eq!(decide(true, false, false), Decision::Skip);
        assert_eq!(decide(true, true, false), Decision::Fail);
        assert_eq!(decide(true, false, true), Decision::Fail);
    }
}
