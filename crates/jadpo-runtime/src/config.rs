//! Starting with the settings checked (area 10, "Checking values before
//! the app starts"): the runtime reads only the variables `config { }`
//! declares, checks each value against its type, and refuses to start if
//! any is missing or invalid. It never opens a port with bad
//! configuration, and it names the setting and its variable, never the
//! value.
//!
//! The environment is a host capability ([`Variables`]): a native host
//! gives the process's environment, `jadpo dev` the environment layered
//! over `.env.local`, a Workers deployable its bindings. The same code
//! runs for every executor and on every platform, so `jadpo config
//! check` and the app starting cannot disagree about what is valid.
//!
//! Values are kept as the text a variable holds, once checked; code reads
//! them decoded ([`decode`]). A secret's text is held in [`ConfigValues`]
//! and nowhere else: nothing here prints it, and its `Debug` leaves it out.

use std::collections::BTreeMap;
use std::fmt;

use jadpo_std::config::{ConfigField, ConfigPlan, Spelling, TestSecret, previous_variable};
use jadpo_value::json::{Codecs, Path, Problem as Rejected};
use jadpo_value::{Validator, Value};

/// Where a host reads the variables a deployment sets: the capability
/// configuration needs of its platform.
pub trait Variables {
    /// The value of variable `name`; `None` when it isn't set. A variable
    /// set to nothing counts as not set, the usual way to unset one.
    fn variable(&self, name: &str) -> Option<String>;
}

/// The variables a host gives ([`Host::variable`](crate::Host::variable)).
#[derive(Clone, Copy)]
pub struct HostVariables<'a>(pub &'a dyn crate::Host);

impl fmt::Debug for HostVariables<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("HostVariables")
    }
}

impl Variables for HostVariables<'_> {
    fn variable(&self, name: &str) -> Option<String> {
        self.0.variable(name).filter(|value| !value.is_empty())
    }
}

impl Variables for BTreeMap<String, String> {
    fn variable(&self, name: &str) -> Option<String> {
        self.get(name).filter(|value| !value.is_empty()).cloned()
    }
}

/// A function over names is a source of variables too.
impl<F: Fn(&str) -> Option<String>> Variables for F {
    fn variable(&self, name: &str) -> Option<String> {
        self(name).filter(|value| !value.is_empty())
    }
}

/// Where the values come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// A deployment, or `jadpo dev`: the variables, then each setting's
    /// `default:`.
    Environment,
    /// A test run (area 13): each setting's `test:` value, then its
    /// `default:`, and a throwaway value made at random for a secret. The
    /// environment is never read, so a test gives the same answer on every
    /// machine.
    Test,
}

/// Where a setting's value came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// The variable the setting names.
    Variable,
    Default,
    Test,
    /// Made at random for a test run.
    Generated,
}

/// How one setting stands, as `jadpo config check` lists it. Never holds
/// the value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Standing {
    pub field: String,
    pub variable: String,
    pub secret: bool,
    pub state: State,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// Set in the environment and valid.
    Set,
    /// Not set, and its `default:` holds.
    Default,
    /// A test run's value.
    Test,
    /// Made for a test run.
    Generated,
    /// Required, and not set.
    Missing,
    /// Set, and not valid: the rule it breaks (`format`, `max`, `type`,
    /// `variant`), never the value.
    Invalid {
        rule: String,
        /// The variable that held it: the setting's, or its `_PREVIOUS`.
        variable: String,
    },
    /// A test run found no value, and nothing reads the setting unless a
    /// test does.
    Absent,
}

impl State {
    pub fn is_problem(&self) -> bool {
        matches!(self, State::Missing | State::Invalid { .. })
    }
}

/// A setting that stops the application starting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartProblem {
    pub field: String,
    pub variable: String,
    pub secret: bool,
    pub kind: ProblemKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProblemKind {
    Missing,
    Invalid { rule: String },
}

/// The checked values of a start.
#[derive(Clone, Default)]
pub struct ConfigValues {
    entries: Vec<Entry>,
}

#[derive(Clone)]
struct Entry {
    name: String,
    secret: bool,
    /// The text, in a variable's spelling; `None` for a setting no value
    /// reached in a test run.
    text: Option<String>,
    /// A signing key's previous value, still accepted.
    previous: Option<String>,
    source: Option<Source>,
}

impl fmt::Debug for ConfigValues {
    /// Names and where each came from; never a value.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut map = f.debug_map();
        for entry in &self.entries {
            map.entry(&entry.name, &entry.source);
        }
        map.finish()
    }
}

impl ConfigValues {
    /// The text of setting `name`, in the spelling of a variable. A secret
    /// is for the places the compiler owns (a connection, a signing key);
    /// nothing else asks for it.
    pub fn text(&self, name: &str) -> Option<&str> {
        self.entry(name)?.text.as_deref()
    }

    /// A signing key's previous value, when one is given (area 10,
    /// "Signing key rotation").
    pub fn previous(&self, name: &str) -> Option<&str> {
        self.entry(name)?.previous.as_deref()
    }

    pub fn source(&self, name: &str) -> Option<Source> {
        self.entry(name)?.source
    }

    /// The text of setting number `at` in the plan.
    pub fn text_at(&self, at: u32) -> Option<&str> {
        self.entries.get(at as usize)?.text.as_deref()
    }

    pub fn is_secret(&self, name: &str) -> bool {
        self.entry(name).is_some_and(|entry| entry.secret)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn entry(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.name == name)
    }
}

/// How each setting stands against `variables`: set, defaulted, missing or
/// invalid, in the plan's order. The values themselves stay with
/// [`load`].
pub fn check(
    plan: &ConfigPlan,
    validators: &[Validator],
    variables: &dyn Variables,
    mode: Mode,
    random: &dyn Fn(&mut [u8]) -> bool,
) -> Vec<Standing> {
    resolve(plan, validators, variables, mode, random)
        .into_iter()
        .map(|(standing, _)| standing)
        .collect()
}

/// The checked values of a start, or every setting that stops it.
pub fn load(
    plan: &ConfigPlan,
    validators: &[Validator],
    variables: &dyn Variables,
    mode: Mode,
    random: &dyn Fn(&mut [u8]) -> bool,
) -> Result<ConfigValues, Vec<StartProblem>> {
    let resolved = resolve(plan, validators, variables, mode, random);
    let mut problems = Vec::new();
    let mut entries = Vec::with_capacity(resolved.len());
    for (field, (standing, entry)) in plan.fields.iter().zip(resolved) {
        match &standing.state {
            State::Missing => problems.push(StartProblem {
                field: field.name.clone(),
                variable: field.variable.clone(),
                secret: field.secret,
                kind: ProblemKind::Missing,
            }),
            State::Invalid { rule, variable } => problems.push(StartProblem {
                field: field.name.clone(),
                variable: variable.clone(),
                secret: field.secret,
                kind: ProblemKind::Invalid { rule: rule.clone() },
            }),
            _ => {}
        }
        entries.push(entry);
    }
    if problems.is_empty() {
        Ok(ConfigValues { entries })
    } else {
        Err(problems)
    }
}

fn resolve(
    plan: &ConfigPlan,
    validators: &[Validator],
    variables: &dyn Variables,
    mode: Mode,
    random: &dyn Fn(&mut [u8]) -> bool,
) -> Vec<(Standing, Entry)> {
    plan.fields
        .iter()
        .map(|field| {
            let (state, text, source) = value_of(plan, validators, field, variables, mode, random);
            let mut previous = None;
            let mut state = state;
            if field.rotates && mode == Mode::Environment && !state.is_problem() {
                let name = previous_variable(&field.variable);
                if let Some(text) = variables.variable(&name) {
                    match decode(plan, validators, field, &text) {
                        Ok(_) => previous = Some(text),
                        Err(rule) => {
                            state = State::Invalid {
                                rule,
                                variable: name,
                            };
                        }
                    }
                }
            }
            (
                Standing {
                    field: field.name.clone(),
                    variable: field.variable.clone(),
                    secret: field.secret,
                    state,
                },
                Entry {
                    name: field.name.clone(),
                    secret: field.secret,
                    text,
                    previous,
                    source,
                },
            )
        })
        .collect()
}

/// One setting's state, its text and where the text came from.
fn value_of(
    plan: &ConfigPlan,
    validators: &[Validator],
    field: &ConfigField,
    variables: &dyn Variables,
    mode: Mode,
    random: &dyn Fn(&mut [u8]) -> bool,
) -> (State, Option<String>, Option<Source>) {
    let (text, source) = match mode {
        Mode::Environment => match variables.variable(&field.variable) {
            Some(text) => (Some(text), Source::Variable),
            None => (field.default.clone(), Source::Default),
        },
        Mode::Test => match (&field.test, &field.default) {
            (Some(test), _) => (Some(test.clone()), Source::Test),
            (None, Some(default)) => (Some(default.clone()), Source::Default),
            (None, None) => (
                field.generated.and_then(|kind| generate(kind, random)),
                Source::Generated,
            ),
        },
    };
    let Some(text) = text else {
        return match mode {
            Mode::Environment => (State::Missing, None, None),
            Mode::Test => (State::Absent, None, None),
        };
    };
    let state = match decode(plan, validators, field, &text) {
        Ok(_) => match source {
            Source::Variable => State::Set,
            Source::Default => State::Default,
            Source::Test => State::Test,
            Source::Generated => State::Generated,
        },
        Err(rule) => State::Invalid {
            rule,
            variable: field.variable.clone(),
        },
    };
    (state, Some(text), Some(source))
}

/// The value `text` is for setting `field`, or the rule it breaks (`type`,
/// `format`, `variant`, `max`...), never the text.
pub fn decode(
    plan: &ConfigPlan,
    validators: &[Validator],
    field: &ConfigField,
    text: &str,
) -> Result<Value, String> {
    let codecs = Codecs {
        codecs: &plan.codecs,
        validators,
        shapes: &[],
    };
    let Some(read) = (match field.spelling {
        Spelling::Plain => Some(text.to_string()),
        other => other.variant(text),
    }) else {
        return Err("variant".to_string());
    };
    let mut problems: Vec<Rejected> = Vec::new();
    let value = codecs.read_text(field.codec, &read, &Path::field(&field.name), &mut problems);
    match (value, problems.first()) {
        (Some(value), None) => Ok(value),
        (_, Some(problem)) => Err(problem.rule.clone()),
        (None, None) => Err("type".to_string()),
    }
}

/// A throwaway value for a secret of a test run: random, never written
/// anywhere, and not worth anything outside the run.
fn generate(kind: TestSecret, random: &dyn Fn(&mut [u8]) -> bool) -> Option<String> {
    let hex = |length: usize| -> Option<String> {
        let mut bytes = vec![0u8; length.div_ceil(2)];
        if !random(&mut bytes) {
            return None;
        }
        let mut out: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        out.truncate(length);
        Some(out)
    };
    match kind {
        TestSecret::Hex { min, max } => {
            let length = (min as usize).max(jadpo_std::SIGNING_KEY_MIN * 2);
            let length = max.map_or(length, |max| length.min(max as usize));
            hex(length)
        }
        TestSecret::PostgresUrl => Some(format!("postgres://test:{}@127.0.0.1:1/test", hex(32)?)),
        TestSecret::RedisUrl => Some(format!("redis://:{}@127.0.0.1:1/0", hex(32)?)),
    }
}

#[cfg(test)]
mod tests {
    use jadpo_std::Builtin;
    use jadpo_std::config::ConfigField;
    use jadpo_std::http::{Codec, CodecId};
    use jadpo_std::types::ValidatorId;

    use super::*;

    fn scalar(builtin: Builtin) -> Codec {
        Codec::Scalar {
            builtin,
            validator: ValidatorId(0),
            int_as_number: true,
        }
    }

    fn field(name: &str, secret: bool, default: Option<&str>) -> ConfigField {
        ConfigField {
            name: name.into(),
            variable: name.to_ascii_uppercase(),
            secret,
            codec: CodecId(0),
            spelling: Spelling::Plain,
            default: default.map(str::to_string),
            test: None,
            generated: None,
            rotates: false,
        }
    }

    fn plan(fields: Vec<ConfigField>, builtin: Builtin) -> (ConfigPlan, Vec<Validator>) {
        (
            ConfigPlan {
                fields,
                codecs: vec![scalar(builtin)],
                connections: Vec::new(),
            },
            vec![Validator::new(Some(builtin), Vec::new())],
        )
    }

    fn vars(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    const NO_RANDOM: &dyn Fn(&mut [u8]) -> bool = &|_| false;

    #[test]
    fn a_value_default_or_gap_is_told_apart() {
        let (plan, validators) = plan(
            vec![
                field("port", false, Some("25")),
                field("limit", false, None),
            ],
            Builtin::Int,
        );
        let standing = check(
            &plan,
            &validators,
            &vars(&[("PORT", "587")]),
            Mode::Environment,
            NO_RANDOM,
        );
        assert_eq!(standing[0].state, State::Set);
        assert_eq!(standing[1].state, State::Missing);
        let fallen = check(&plan, &validators, &vars(&[]), Mode::Environment, NO_RANDOM);
        assert_eq!(fallen[0].state, State::Default);
    }

    #[test]
    fn an_invalid_value_names_the_rule_and_never_the_value() {
        let (plan, validators) = plan(vec![field("port", false, None)], Builtin::Int);
        let problems = load(
            &plan,
            &validators,
            &vars(&[("PORT", "many")]),
            Mode::Environment,
            NO_RANDOM,
        )
        .expect_err("not a number");
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].variable, "PORT");
        assert_eq!(
            problems[0].kind,
            ProblemKind::Invalid {
                rule: "type".into()
            }
        );
        assert!(!format!("{problems:?}").contains("many"));
    }

    #[test]
    fn an_empty_variable_is_not_set() {
        let (plan, validators) = plan(vec![field("name", false, None)], Builtin::Text);
        let problems = load(
            &plan,
            &validators,
            &vars(&[("NAME", "")]),
            Mode::Environment,
            NO_RANDOM,
        )
        .expect_err("empty is unset");
        assert_eq!(problems[0].kind, ProblemKind::Missing);
    }

    #[test]
    fn a_secret_is_never_in_debug_output() {
        let (plan, validators) = plan(vec![field("api_key", true, None)], Builtin::Text);
        let values = load(
            &plan,
            &validators,
            &vars(&[("API_KEY", "hunter2-hunter2")]),
            Mode::Environment,
            NO_RANDOM,
        )
        .expect("set");
        assert_eq!(values.text("api_key"), Some("hunter2-hunter2"));
        assert!(!format!("{values:?}").contains("hunter2"));
    }

    #[test]
    fn a_test_run_ignores_the_environment_and_makes_its_secrets() {
        let mut secret = field("api_key", true, None);
        secret.generated = Some(TestSecret::Hex {
            min: 8,
            max: Some(20),
        });
        let (plan, validators) = plan(vec![secret], Builtin::Text);
        let values = load(
            &plan,
            &validators,
            &vars(&[("API_KEY", "from-the-machine")]),
            Mode::Test,
            &|bytes: &mut [u8]| {
                bytes.fill(0xab);
                true
            },
        )
        .expect("generated");
        let text = values.text("api_key").expect("a value");
        assert_eq!(text.len(), 20);
        assert_ne!(text, "from-the-machine");
        assert_eq!(values.source("api_key"), Some(Source::Generated));
    }

    #[test]
    fn a_signing_key_accepts_its_previous_value() {
        let mut key = field("session_key", true, None);
        key.rotates = true;
        let (plan, validators) = plan(vec![key], Builtin::Text);
        let values = load(
            &plan,
            &validators,
            &vars(&[("SESSION_KEY", "current"), ("SESSION_KEY_PREVIOUS", "old")]),
            Mode::Environment,
            NO_RANDOM,
        )
        .expect("both set");
        assert_eq!(values.text("session_key"), Some("current"));
        assert_eq!(values.previous("session_key"), Some("old"));
    }
}
