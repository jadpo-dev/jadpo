//! What each test runs under (area 13, "Writing a test": "Each test starts
//! from nothing: its own empty database, its own clock and its own fakes,
//! so tests never depend on each other and can run in any order"). The
//! runner makes one per test and hands it to the executor through the
//! runtime's context, so a test's environment never depends on which
//! thread runs it or what ran before.

use jadpo_runtime::{Clock, Env, MemoryHost};

use crate::select::splitmix;

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

/// A stable digest of `text` (FNV-1a), the same on every platform.
fn digest(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

impl TestEnv {
    /// The environment of the test with key `key` in a run seeded `seed`:
    /// its seed comes from both, so a rerun with the same seed gives it the
    /// same one wherever it falls in the order. Its clock starts at the
    /// fixed default until a test's `clock:` setting is built (roadmap 5.2).
    pub fn new(seed: u64, key: &str) -> TestEnv {
        TestEnv {
            env: Env {
                seed: splitmix(seed ^ digest(key)),
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
    fn a_tests_seed_follows_the_run_and_the_test_not_the_order() {
        let a = TestEnv::new(1, "test sizes \"a\"");
        assert_eq!(a, TestEnv::new(1, "test sizes \"a\""));
        assert_ne!(a.env.seed, TestEnv::new(2, "test sizes \"a\"").env.seed);
        assert_ne!(a.env.seed, TestEnv::new(1, "test sizes \"b\"").env.seed);
        assert_eq!(a.env.clock, Clock::TEST);
    }
}
