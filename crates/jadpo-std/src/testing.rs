//! The runtime plan's dev and test section (chunk 8.1e): what only
//! `jadpo test` and `jadpo dev` read of a program, kept apart from what a
//! deployable runs with. Each test's identity key and what it gives
//! itself (its `config:` and `clock:`, area 13), and the recordings: each
//! provider operation's, which `jadpo dev`'s fake answers from, every
//! recording of the app's own integrations, which `jadpo test` decodes,
//! and every recording of a prompt's answer (area 21, "Tests").
//!
//! It exists only with the `testing` feature, as
//! [`crate::RuntimePlan::testing`], and is written after the rest of the
//! plan's bytes ([`crate::RuntimePlan::encode`]), so a deployable built
//! without the feature holds none of it and refuses bytes that do.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::embed::PromptRecording;
use crate::provider::RecordedPlan;

/// The dev and test section: see the module.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestingPlan {
    /// Each test's number among the callables, by its identity key
    /// (`test main "a large size is rejected"`): the other callables'
    /// keys are in [`crate::RuntimePlan::callables`].
    pub tests: BTreeMap<String, u32>,
    /// What each test that gives itself anything gives, by the test's
    /// number.
    pub setups: BTreeMap<u32, TestSetup>,
    /// Each provider operation's recording's text,
    /// `recordings/<resource>.<operation>.json` in its package, by the
    /// operation's number: what `jadpo dev`'s fake answers from with its
    /// placeholders filled (chunk 7.2d2).
    pub operation_recordings: BTreeMap<u32, String>,
    /// Every recording of the app's own integrations (area 11,
    /// "Provenance and confirmation"; chunk 7.2d2), which `jadpo test` and
    /// the publishing bar decode with its operation's decoders.
    pub recorded: Vec<RecordedPlan>,
    /// Every recording of a prompt's answer, which `jadpo test` reads
    /// (area 21, "Tests").
    pub prompt_recordings: Vec<PromptRecording>,
}

/// What a test gives itself (area 13, "Test settings").
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestSetup {
    /// The settings its `config:` changes, each by its number in the
    /// plan's settings with its value in a variable's spelling, read as
    /// the setting's type reads one.
    pub settings: Vec<(u32, String)>,
    /// Where its `clock:` starts its clock, in milliseconds since
    /// 1970-01-01T00:00:00Z.
    pub clock: Option<i64>,
    /// The value its `flags:` gives each flag it names, by the flag's
    /// number in the plan's flags and the value's place among the flag's
    /// values (area 16, "What the compiler checks"; chunk B9).
    pub flags: Vec<(u32, u32)>,
}

impl TestingPlan {
    /// Whether it holds nothing: a plan with no tests or recordings, whose
    /// bytes then end with the rest of the plan.
    pub fn is_empty(&self) -> bool {
        *self == TestingPlan::default()
    }
}
