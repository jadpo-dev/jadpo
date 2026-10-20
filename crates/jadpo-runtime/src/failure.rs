//! A domain failure at runtime (area 4): which declared failure, its client
//! and internal fields, where it was raised and the path it travelled, and
//! the failure it was mapped from. [`Failure::of`] builds one from its
//! failure plan, as both executors raise a declared failure.

use jadpo_std::types::{FailurePlan, Section};
use jadpo_value::Value;

use crate::{Frame, Places};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// The failure's identity key: `type QuantityUnavailable`.
    pub key: String,
    /// Its name as written: `QuantityUnavailable`.
    pub name: String,
    /// Its public code: `quantity_unavailable`.
    pub code: String,
    /// `FailureKind.conflict` as `conflict`.
    pub kind: Option<String>,
    /// The HTTP status its kind sets.
    pub status: Option<u16>,
    pub message: Option<String>,
    /// Sent to the caller, in the order the failure declares them.
    pub client: Vec<(String, Value)>,
    /// Never leaves the server.
    pub internal: Vec<(String, Value)>,
    /// For a failure a construction's `attempt` maps a rule to: the rule
    /// the constructed value broke, as written (`max: 1000`), never the
    /// value.
    pub broken: Option<String>,
    /// The shadow stack where it was raised, innermost first: the first
    /// frame's site is where, and the callables are the path it travelled.
    pub frames: Vec<Frame>,
    /// The failure it was mapped from, kept as an internal cause (area 4,
    /// question 8).
    pub cause: Option<Box<Failure>>,
    /// For a built-in failure that says when to try again, the seconds to
    /// wait, which a route's response gives as `Retry-After`: magic links'
    /// `rate_limited` and `sign_in_codes_disabled` (chunk 7.3b3).
    pub retry_after: Option<i64>,
}

impl Failure {
    /// A failure of `name` with its code, before it is raised.
    pub fn new(
        key: impl Into<String>,
        name: impl Into<String>,
        code: impl Into<String>,
    ) -> Failure {
        Failure {
            key: key.into(),
            name: name.into(),
            code: code.into(),
            kind: None,
            status: None,
            message: None,
            client: Vec::new(),
            internal: Vec::new(),
            broken: None,
            frames: Vec::new(),
            cause: None,
            retry_after: None,
        }
    }

    /// Declared failure `plan`, with the fields given in `values` placed
    /// by `slots` (each value's place among the failure's fields), before
    /// it is raised; `None` when a field is left without a value.
    pub fn of(plan: &FailurePlan, slots: &[Option<u32>], values: Vec<Value>) -> Option<Failure> {
        let mut given: Vec<Option<Value>> = vec![None; plan.fields.len()];
        for (slot, value) in slots.iter().zip(values) {
            if let Some(place) = slot.and_then(|slot| given.get_mut(slot as usize))
                && place.is_none()
            {
                *place = Some(value);
            }
        }
        let mut failure = Failure::new(plan.key.as_str(), plan.name.as_str(), plan.code.as_str());
        failure.kind = plan.kind.clone();
        failure.status = plan.status;
        failure.message = plan.message.clone();
        for (field, value) in plan.fields.iter().zip(given) {
            let entry = (field.name.clone(), value?);
            match field.section {
                Some(Section::Client) => failure.client.push(entry),
                _ => failure.internal.push(entry),
            }
        }
        Some(failure)
    }

    /// The failure as the value a `match` arm binds it to (area 4): a
    /// record of its name holding its client fields, then its internal
    /// ones, in the order it declares them.
    pub fn record(&self) -> Value {
        let fields = self
            .client
            .iter()
            .chain(&self.internal)
            .map(|(name, value)| (std::rc::Rc::from(name.as_str()), value.clone()))
            .collect();
        Value::record(&self.name, fields)
    }

    /// Where it was raised.
    pub fn site(&self) -> Option<Frame> {
        self.frames.first().copied()
    }

    /// The callables it travelled through, innermost first, as area 4's
    /// log lists them: `action reserve`, `action place_order`.
    pub fn trace(&self, places: &Places) -> Vec<String> {
        self.frames
            .iter()
            .map(|frame| places.label(frame.callable).to_string())
            .collect()
    }
}
