//! What each test runs under (area 13, "Writing a test": "Each test starts
//! from nothing: its own empty database, its own clock and its own fakes,
//! so tests never depend on each other and can run in any order"). The
//! runner makes one per test and hands it to the executor through the
//! runtime's context, so a test's environment never depends on which
//! thread runs it or what ran before.

use jadpo_runtime::{Clock, Env, MemoryHost};

/// The database a test runs against.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum TestDatabase {
    /// None: the project has no tables, or the test calls no action, so
    /// it can't touch a store.
    #[default]
    None,
    /// Its own copy of each of these stores' template databases, at the
    /// latest migration, handed to it the first time it uses one
    /// ([`crate::database`]).
    Copied { stores: Vec<String> },
}

/// The responses each integration operation gives a test, in order:
/// scripted with a test's `fakes:` setting once packages are built (area
/// 11, roadmap 7.2). Empty until then.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fakes {}

/// One test's environment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestEnv {
    /// The test's own seed and clock.
    pub env: Env,
    pub database: TestDatabase,
    pub fakes: Fakes,
}

impl TestEnv {
    /// The environment of the test with key `key`: its seed comes from
    /// the key alone, so every run gives the test the same identities,
    /// tokens, codes and shuffles wherever it falls in the order, as its
    /// clock is fixed (area 13, "Controlling time", amended by area 19,
    /// question 19); the run's order seed decides only the order. Its
    /// clock starts at the fixed default, 2000-01-01T00:00:00Z, which a
    /// test's `clock:` moves. The runtime's [`jadpo_runtime::test_run`]
    /// makes it the same way for the run itself.
    pub fn new(key: &str) -> TestEnv {
        TestEnv {
            env: Env {
                seed: Some(jadpo_runtime::test_run::seed(key)),
                clock: Clock::TEST,
            },
            database: TestDatabase::None,
            fakes: Fakes::default(),
        }
    }

    /// The host a test without a database runs on: no store, no
    /// subscribers. A test with one runs on [`crate::database::TestHost`].
    pub fn host(&self) -> MemoryHost {
        MemoryHost::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tests_seed_follows_the_test_not_the_run_or_the_order() {
        let a = TestEnv::new("test sizes \"a\"");
        assert_eq!(a, TestEnv::new("test sizes \"a\""));
        assert_ne!(a.env.seed, TestEnv::new("test sizes \"b\"").env.seed);
        assert_eq!(a.env.clock, Clock::TEST);
    }
}
