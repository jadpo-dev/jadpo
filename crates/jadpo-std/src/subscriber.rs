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
}

impl EventsPlan {
    /// The subscribers of the event type whose identity key is `event`.
    pub fn subscribers_of<'a>(&'a self, event: &'a str) -> impl Iterator<Item = &'a SubscriberPlan> {
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
    /// The action it runs, by callable number.
    pub callable: u32,
    /// Each argument, a path read from the event: empty for the event
    /// itself.
    pub args: Vec<Vec<String>>,
    /// How many times a delivery is tried (area 12, `retry:`).
    pub attempts: u32,
    /// How long after its event was emitted a delivery is given up.
    pub within_ms: u64,
    /// The tenant a delivery is limited to.
    pub tenant: TenantFrom,
    /// The store its action writes, where its handled record commits with
    /// its writes (area 12, "Outbox placement, settled"); `None` when it
    /// writes none.
    pub store: Option<String>,
    /// Where it is written, for logs.
    pub file: String,
    pub line: u32,
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
