//! What one invocation runs under besides its host: in `jadpo test`, each
//! test's own seed and clock (area 13, "Writing a test": "its own empty
//! database, its own clock and its own fakes"); in a deployable, a
//! request's.

/// An invocation's environment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Env {
    /// Seeds what an invocation decides at random, such as new identities,
    /// so a test that uses them reproduces. A test's comes from the run's
    /// order seed and the test's key, never from the order tests run in.
    /// `None` in a deployable: what it decides at random takes the host's
    /// random bytes ([`crate::Host::random`]), since the seeded generator
    /// can be run backwards from what it gave.
    pub seed: Option<u64>,
    pub clock: Clock,
}

impl Default for Env {
    /// A test's environment: seed 0 and the test clock.
    fn default() -> Env {
        Env {
            seed: Some(0),
            clock: Clock::TEST,
        }
    }
}

/// The time `clock.now` reads. A test's clock is fixed and moves only when
/// the test moves it (area 13, "Controlling time").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clock {
    /// Milliseconds since 1970-01-01T00:00:00Z.
    millis: i64,
}

impl Clock {
    /// Where a test's clock starts without a `clock:` setting: a fixed
    /// time, never the real one (area 13). 2000-01-01T00:00:00Z, as the
    /// archived compiler used, until chunk 5.2 builds the test clock and
    /// documents its default.
    pub const TEST: Clock = Clock {
        millis: 946_684_800_000,
    };

    /// A clock stopped at `millis`.
    pub fn fixed(millis: i64) -> Clock {
        Clock { millis }
    }

    /// Now, in milliseconds since 1970-01-01T00:00:00Z.
    pub fn now(&self) -> i64 {
        self.millis
    }
}
