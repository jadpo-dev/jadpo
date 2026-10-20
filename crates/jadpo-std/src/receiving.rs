//! Queues received as events, as plain data (area 12, "Events from
//! outside"; area 11, "Queues"; chunk 7.7c): each queue `receiving:`
//! names, where each message's type and id are in its body, the events it
//! may carry, the store they are kept in, and how its connection's package
//! fetches and acknowledges messages (`provides/receiving.jadpo`).

/// One queue the application receives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueuePlan {
    /// The queue's connection, `warehouse_queue`, which names it.
    pub connection: String,
    /// Where each message's type is in its body, dotted from its root;
    /// `None` for a queue of one kind, whose every message is its one
    /// event.
    pub type_at: Option<String>,
    /// Where each message's id is in its body, dotted: a repeat of an id
    /// already received is dropped.
    pub id_at: String,
    /// The events its messages may be, each by the type it is named by.
    pub events: Vec<QueueEvent>,
    /// The store the events, the ids received and the messages parked are
    /// kept in.
    pub store: String,
    pub receive: ReceivePlan,
    pub acknowledge: AcknowledgePlan,
}

impl QueuePlan {
    /// Where its ids are recorded as received, `queue warehouse_queue`.
    pub fn source(&self) -> String {
        format!("queue {}", self.connection)
    }
}

/// An event a queue's messages may be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueueEvent {
    /// Its `wire_name:`, which the type at `type_at:` is matched against;
    /// `None` for the one event of a queue of one kind that gives none.
    pub wire_name: Option<String>,
    /// The event type's identity key, which its outbox row records.
    pub key: String,
    /// Its number in the plan's JSON table: a message's body is read with
    /// its codec, and its payload kept with it.
    pub number: u32,
}

/// How a queue's package fetches messages (`receive:` in
/// `provides/receiving.jadpo`): the operation it calls, its input, and
/// where the messages, each one's body and its handle are in the answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceivePlan {
    /// `messages.receive_message`.
    pub operation: String,
    /// Each member of its input, by its spelling on the wire.
    pub input: Vec<(String, Mapped)>,
    /// Where the list of messages is in its output, by field names.
    pub messages: Vec<String>,
    /// Where each message's body, its text, is, from the message.
    pub body: Vec<String>,
    /// Where the handle a message is acknowledged by is, from the message.
    pub handle: Vec<String>,
}

/// How a message is acknowledged once its event is kept (`acknowledge:`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcknowledgePlan {
    pub operation: String,
    pub input: Vec<(String, Mapped)>,
}

/// A value a mapping gives an operation's input: data, never code (area
/// 11, "How a package provides one").
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mapped {
    /// The value of the setting numbered here, from the connection's
    /// setting of the package (`settings.queue_url`).
    Setting(u32),
    Text(String),
    Int(i64),
    Bool(bool),
    /// The message's handle (`message.handle`).
    Handle,
}
