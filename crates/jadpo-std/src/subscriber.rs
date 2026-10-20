//! The subscriber table (area 12; chunk 6.1b): plain data the plan
//! decides and the runtime runs, so both executors deliver an event to
//! the same subscribers in the same way.

/// Every subscriber, and where an action that writes no store keeps the
/// events it emits.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EventsPlan {
    /// In key order.
    pub subscribers: Vec<SubscriberPlan>,
    /// `application { events: { store: } }`.
    pub store: Option<String>,
    /// Every email and how it is sent (area 16; chunk 7.3): each has a
    /// subscriber whose target is [`Target::Email`].
    pub emails: crate::email::EmailsPlan,
}

impl EventsPlan {
    /// The subscribers of the event type whose identity key is `event`.
    pub fn subscribers_of<'a>(
        &'a self,
        event: &'a str,
    ) -> impl Iterator<Item = &'a SubscriberPlan> {
        self.subscribers
            .iter()
            .filter(move |subscriber| subscriber.event == event)
    }

    /// The subscriber whose identity key is `key`.
    pub fn subscriber(&self, key: &str) -> Option<&SubscriberPlan> {
        self.subscribers
            .iter()
            .find(|subscriber| subscriber.key == key)
    }
}

/// One subscriber (area 12, "Subscribers").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubscriberPlan {
    /// Its identity key, which each delivery record names: `subscriber
    /// planning.update_statistics`.
    pub key: String,
    /// As a log names it.
    pub name: String,
    /// The identity key of the event type it listens for.
    pub event: String,
    /// What a delivery hands the event to.
    pub target: Target,
    /// How many times a delivery is tried (area 12, `retry:`).
    pub attempts: u32,
    /// How long after its event was emitted a delivery is given up.
    pub within_ms: u64,
    /// The tenant a delivery is limited to.
    pub tenant: TenantFrom,
    /// The store its action writes, where its handled record commits with
    /// its writes (area 12, "Outbox placement, settled"), or, writing
    /// none, the one its events are kept in, where its handled record
    /// commits with them (chunk 6.6a); `None` when it does neither, as a
    /// capability's delivery, whose record alone says it was sent.
    pub store: Option<String>,
    /// The connections to providers its action calls, itself or through
    /// an action it calls (area 11): a delivery is held while one of
    /// their circuit breakers is open (area 12, question 18; chunk 7.2b).
    /// In name order.
    pub connections: Vec<String>,
    /// Where it is written, for logs.
    pub file: String,
    pub line: u32,
}

impl SubscriberPlan {
    /// The connections a delivery to it calls out through: its action's,
    /// or the capability's it hands the event to.
    pub fn calls_through(&self) -> Vec<String> {
        match &self.target {
            Target::Capability { connection, .. } | Target::Email { connection } => {
                vec![connection.clone()]
            }
            Target::Action { .. } => self.connections.clone(),
        }
    }
}

/// What a delivery hands its event to (area 12; chunk 7.0c): a
/// subscriber's action, or a capability's operation, as sending an email
/// is emitting it (area 16, "Email").
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// The action it runs, by callable number, each argument a path read
    /// from the event: empty for the event itself.
    Action {
        callable: u32,
        args: Vec<Vec<String>>,
    },
    /// An operation of the capability its connection fills, given the
    /// event whole: a test capability's (chunk 7.0c).
    Capability {
        /// The connection the host gives a driver for (area 11).
        connection: String,
        operation: String,
        repeat: Repeat,
    },
    /// An email, rendered whole and handed to `send` of the connection
    /// `application { email: }` names (area 16, "Email"; chunk 7.3): its
    /// plan is [`crate::email::EmailsPlan::email`] of the subscriber's
    /// event. Sent with no key, as SMTP and the mail APIs take none.
    Email { connection: String },
}

/// Whether an outside call may be sent again (area 11, "Idempotency";
/// area 12, "Delivery: at least once, safe to repeat"; chunk 7.0c).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Repeat {
    /// It changes nothing, or its package marks it safe to repeat.
    Safe,
    /// It changes something under an idempotency key its provider keeps:
    /// sent again under the same key, it acts once.
    Keyed,
    /// It changes something with no key (Postmark's, SES's sends): sent
    /// twice, it may act twice, so one whose outcome is unknown is parked
    /// for an operator, never sent again blindly (area 16).
    Unkeyed,
}

/// Where a delivery's tenant comes from (area 9, "Background work").
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TenantFrom {
    /// The application has no tenant: nothing limits the system.
    Untenanted,
    /// The event's field holding the tenant's id.
    Field(String),
    /// The event names no tenant: its subscribers reach no tenant's
    /// records.
    Nowhere,
}
