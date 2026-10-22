//! Queues received as events, as plain data (area 12, "Events from
//! outside"; area 11, "Queues"; chunk 7.7c): each queue `receiving:`
//! names, where each message's type and id are in its body, the events it
//! may carry, the store they are kept in, and how its messages arrive:
//! fetched and acknowledged through its connection's package
//! (`provides/receiving.jadpo`), or pushed in batches to a consumer on
//! Workers by Cloudflare Queues (`Driver.cloudflare_queues`, chunk 7.7c2),
//! whose consumer, and its dead-letter queue's (chunk 8.4b),
//! [`wrangler_queues`] writes for the Wrangler file.

/// One queue the application receives.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    /// How its messages arrive.
    pub source: QueueSource,
}

/// How a queue's messages arrive.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum QueueSource {
    /// Fetched on the tick through the connection's package, and each
    /// acknowledged once its event is kept (chunk 7.7c).
    Package {
        receive: ReceivePlan,
        acknowledge: AcknowledgePlan,
    },
    /// Pushed by Cloudflare Queues to the Workers host's consumer, a batch
    /// at a time, each message acknowledged or retried by the batch's
    /// answer (chunk 7.7c2).
    Cloudflare(CloudflareQueue),
}

impl QueuePlan {
    /// Where its ids are recorded as received, `queue warehouse_queue`.
    pub fn source(&self) -> String {
        format!("queue {}", self.connection)
    }
}

/// An event a queue's messages may be.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReceivePlan {
    /// `messages.receive_message`.
    pub operation: String,
    /// Its number in the plan's provider operations (chunk 8.1d2).
    pub called: u32,
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
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AcknowledgePlan {
    pub operation: String,
    /// Its number in the plan's provider operations (chunk 8.1d2).
    pub called: u32,
    pub input: Vec<(String, Mapped)>,
}

/// A value a mapping gives an operation's input: data, never code (area
/// 11, "How a package provides one").
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

/// A queue on Cloudflare Queues, as its connection names it
/// (`driver: Driver.cloudflare_queues`, chunk 7.7c2).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CloudflareQueue {
    /// Its name in Cloudflare, `warehouse-events`, which a batch handed
    /// to the consumer names.
    pub queue: String,
    /// The queue Cloudflare moves a message to once its retries are
    /// spent, so no message is dropped.
    pub dead_letter_queue: String,
}

/// The most messages Cloudflare hands the consumer in one batch, as many
/// as one receive asks a package's queue for.
pub const CONSUMER_BATCH: u32 = 10;

/// The most seconds Cloudflare waits to fill a batch.
pub const CONSUMER_WAIT_SECONDS: u32 = 5;

/// How many times Cloudflare hands a message again that the consumer
/// retried, before it moves it to the dead-letter queue (chunk 8.4b): with
/// the delays the runtime gives (`jadpo_runtime::receiving::retry_delay`,
/// 10 seconds doubled for each attempt, at most an hour), about 22 hours,
/// long enough to outlast a deploy's migration and short enough that the
/// message reaches the dead-letter queue inside the 24 hours a queue on
/// Workers' Free plan keeps it.
pub const CONSUMER_RETRIES: u32 = 30;

/// How many times Cloudflare hands a message of a dead-letter queue again
/// that its consumer retried, the store unable to park it: Cloudflare's
/// most, since a dead-letter queue has none of its own and a message whose
/// retries are spent there is gone (chunk 8.4b).
pub const DEAD_LETTER_RETRIES: u32 = 100;

/// The `queues` member of the Wrangler file for `queues`: for each one on
/// Cloudflare Queues a consumer, and one for its dead-letter queue, whose
/// messages the consumer parks for an operator (chunk 8.4b); none for a
/// package's. `None` when there is none, so the file names no consumer.
pub fn wrangler_queues(queues: &[QueuePlan]) -> Option<serde_json::Value> {
    let consumers: Vec<serde_json::Value> = queues
        .iter()
        .flat_map(|queue| match &queue.source {
            QueueSource::Cloudflare(cloudflare) => vec![
                serde_json::json!({
                    "queue": cloudflare.queue,
                    "max_batch_size": CONSUMER_BATCH,
                    "max_batch_timeout": CONSUMER_WAIT_SECONDS,
                    "max_retries": CONSUMER_RETRIES,
                    "dead_letter_queue": cloudflare.dead_letter_queue,
                }),
                serde_json::json!({
                    "queue": cloudflare.dead_letter_queue,
                    "max_batch_size": CONSUMER_BATCH,
                    "max_batch_timeout": CONSUMER_WAIT_SECONDS,
                    "max_retries": DEAD_LETTER_RETRIES,
                }),
            ],
            QueueSource::Package { .. } => Vec::new(),
        })
        .collect();
    (!consumers.is_empty()).then(|| serde_json::json!({ "consumers": consumers }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queue(connection: &str, source: QueueSource) -> QueuePlan {
        QueuePlan {
            connection: connection.into(),
            type_at: None,
            id_at: "id".into(),
            events: Vec::new(),
            store: "main".into(),
            source,
        }
    }

    fn cloudflare(name: &str) -> QueueSource {
        QueueSource::Cloudflare(CloudflareQueue {
            queue: name.into(),
            dead_letter_queue: format!("{name}-dead"),
        })
    }

    /// Each queue on Cloudflare Queues is a consumer, retried for about 22
    /// hours before Cloudflare moves a message to its dead-letter queue,
    /// and that queue a consumer too, retried as long as Cloudflare allows.
    #[test]
    fn each_cloudflare_queue_is_a_consumer_in_the_wrangler_file() {
        let package = QueueSource::Package {
            receive: ReceivePlan {
                operation: "messages.receive_message".into(),
                called: 0,
                input: Vec::new(),
                messages: vec!["messages".into()],
                body: vec!["body".into()],
                handle: vec!["handle".into()],
            },
            acknowledge: AcknowledgePlan {
                operation: "messages.delete_message".into(),
                called: 1,
                input: Vec::new(),
            },
        };
        let queues = [
            queue("warehouse_queue", cloudflare("warehouse-events")),
            queue("sqs", package.clone()),
            queue("billing_queue", cloudflare("billing")),
        ];
        assert_eq!(
            wrangler_queues(&queues),
            Some(serde_json::json!({
                "consumers": [
                    {
                        "queue": "warehouse-events",
                        "max_batch_size": 10,
                        "max_batch_timeout": 5,
                        "max_retries": 30,
                        "dead_letter_queue": "warehouse-events-dead",
                    },
                    {
                        "queue": "warehouse-events-dead",
                        "max_batch_size": 10,
                        "max_batch_timeout": 5,
                        "max_retries": 100,
                    },
                    {
                        "queue": "billing",
                        "max_batch_size": 10,
                        "max_batch_timeout": 5,
                        "max_retries": 30,
                        "dead_letter_queue": "billing-dead",
                    },
                    {
                        "queue": "billing-dead",
                        "max_batch_size": 10,
                        "max_batch_timeout": 5,
                        "max_retries": 100,
                    },
                ]
            }))
        );
        assert_eq!(wrangler_queues(&[queue("sqs", package)]), None);
        assert_eq!(wrangler_queues(&[]), None);
    }
}
