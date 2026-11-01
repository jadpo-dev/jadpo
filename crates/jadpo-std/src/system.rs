//! The system tables (area 5, "System tables"; chunk 6.8): the tables the
//! runtime keeps its own records in, prefixed `jadpo_`, defined once,
//! here. The checker puts each in the stores that need it, so migrations
//! create them and drift in them fails `jadpo check`; the runtime's
//! statements read and write them, and a live test prepares every one of
//! those statements against the schema these give.
//!
//! Each table is a constant, and [`ALL`] lists them: a name no table has
//! is no table ([`by_name`]), never another one.

use crate::SqlType;

/// A system table: its columns in order, its primary key, and its unique
/// rules, foreign keys and indexes, each named from its table and columns
/// as an entity's are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemTable {
    /// Its name in the store: `jadpo_outbox`.
    pub table: &'static str,
    pub columns: &'static [SystemColumn],
    pub primary_key: &'static [&'static str],
    pub unique: &'static [SystemUnique],
    pub foreign_keys: &'static [SystemForeignKey],
    pub indexes: &'static [SystemIndex],
}

/// A column: its name, its type and whether it may be `NULL`. The type is
/// a store's own, never an engine's text: PostgreSQL's column is
/// [`SqlType::postgres_column`], so a table added here adds no SQL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemColumn {
    pub name: &'static str,
    pub sql_type: SqlType,
    pub nullable: bool,
}

/// A unique rule over `columns`, with what it is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemUnique {
    pub columns: &'static [&'static str],
    pub rule: &'static str,
}

/// A foreign key from `columns` to `table`'s primary key: deleting the
/// row it names deletes this one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemForeignKey {
    pub columns: &'static [&'static str],
    /// The table it names, with its prefix: `jadpo_outbox`.
    pub table: &'static str,
}

/// An index of `columns`, ascending, with what it is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemIndex {
    pub columns: &'static [&'static str],
    pub reason: &'static str,
}

impl SystemTable {
    /// Its name without the `jadpo_` prefix, as a schema says what a
    /// system table holds: `outbox`, `batch_runs`.
    pub fn name(&self) -> &'static str {
        self.table.strip_prefix("jadpo_").unwrap_or(self.table)
    }

    /// Its column `name`.
    pub fn column(&self, name: &str) -> Option<&'static SystemColumn> {
        self.columns.iter().find(|column| column.name == name)
    }
}

const fn column(name: &'static str, sql_type: SqlType, nullable: bool) -> SystemColumn {
    SystemColumn {
        name,
        sql_type,
        nullable,
    }
}

/// The event outbox (area 12, "Emitting an event"): each event an action
/// emitted, written with its writes.
pub const OUTBOX: SystemTable = SystemTable {
    table: "jadpo_outbox",
    columns: &[
        column("id", SqlType::Uuid, false),
        // The event type's identity key, which survives a rename.
        column("event", SqlType::Text, false),
        column("payload", SqlType::Json, false),
        column("emitted_at", SqlType::Timestamp, false),
        // W3C trace context of the action that emitted it.
        column("traceparent", SqlType::Text, true),
        // The encoding its payload was written in (area 19, "Changing a
        // standard type's encoding"; chunk 5.9), which a delivery reads
        // it by (chunk 6.8). The runtime always writes it; a migration
        // adds it to a table that may hold rows already, so it may be
        // `NULL`, a payload written before, read as encoding 1.
        column("encoding", SqlType::Int2, true),
        // How many events led to it from the request that set the chain
        // off, which stops a loop at 16 (area 12, "Limits"; chunk 6.1b).
        // Added to a table that may hold rows, so it may be `NULL`, read
        // as 0.
        column("depth", SqlType::Int4, true),
    ],
    primary_key: &["id"],
    unique: &[],
    foreign_keys: &[],
    indexes: &[SystemIndex {
        columns: &["emitted_at"],
        reason: "system outbox",
    }],
};

/// The delivery of each event to each subscriber (area 12, "Delivery: at
/// least once, safe to repeat").
pub const DELIVERIES: SystemTable = SystemTable {
    table: "jadpo_deliveries",
    columns: &[
        column("id", SqlType::Uuid, false),
        column("outbox_id", SqlType::Uuid, false),
        column("subscriber", SqlType::Text, false),
        column("status", SqlType::Text, false),
        column("attempts", SqlType::Int4, false),
        column("next_attempt_at", SqlType::Timestamp, true),
        column("last_failure", SqlType::Json, true),
        // `NULL` until it is first tried (chunk 6.1b).
        column("last_tried_at", SqlType::Timestamp, true),
        // When an operator last retried it, or resolved it as not done
        // (`jadpo deliveries`; chunk 6.12): its `within:` counts from
        // then rather than from its event. `NULL` for one never retried.
        column("retried_at", SqlType::Timestamp, true),
        // While it is `held` by its connection's circuit breaker (area
        // 12, question 18; chunk 7.2b): the connection, and since when.
        // `NULL` otherwise.
        column("held_for", SqlType::Text, true),
        column("held_since", SqlType::Timestamp, true),
        // How long it has been held before, in milliseconds, which its
        // `within:` doesn't count: held deliveries keep their budget.
        // `NULL` for one never held.
        column("held_ms", SqlType::Int8, true),
    ],
    primary_key: &["id"],
    unique: &[SystemUnique {
        columns: &["outbox_id", "subscriber"],
        rule: "system deliveries",
    }],
    foreign_keys: &[SystemForeignKey {
        columns: &["outbox_id"],
        table: OUTBOX.table,
    }],
    indexes: &[SystemIndex {
        columns: &["status", "next_attempt_at"],
        reason: "system deliveries",
    }],
};

/// Each connection's circuit breaker (area 12, question 18; chunk 7.2b),
/// beside the deliveries it holds: `closed`, counting `failures`, the
/// deliveries in a row whose call through the connection found its
/// provider unavailable; `open` once that run is long enough, holding the
/// deliveries that call it, since `opened_at`, until `probe_at`, after
/// `pauses` pauses; `probing` while one delivery, `probe_id`, is tried,
/// its claim lasting until `probe_until`. `alerted_at` is when a tick
/// raised its alert, so of several instances one raises it, once per
/// outage (chunk 6.9's alerts, recorded on state).
pub const CIRCUIT_BREAKERS: SystemTable = SystemTable {
    table: "jadpo_circuit_breakers",
    columns: &[
        column("connection", SqlType::Text, false),
        column("state", SqlType::Text, false),
        column("failures", SqlType::Int4, false),
        column("opened_at", SqlType::Timestamp, true),
        column("pauses", SqlType::Int4, false),
        column("probe_at", SqlType::Timestamp, true),
        column("probe_id", SqlType::Uuid, true),
        column("probe_until", SqlType::Timestamp, true),
        column("alerted_at", SqlType::Timestamp, true),
    ],
    primary_key: &["connection"],
    unique: &[],
    foreign_keys: &[],
    indexes: &[],
};

/// A subscriber's handled record (area 12, "Outbox placement";
/// chunk 6.1b): in the store its action writes, committed with its
/// writes, so a repeated delivery finds it and does nothing. Keyed by the
/// delivery's `id`, which the outbox's store holds: no reference across
/// stores.
pub const HANDLED_DELIVERIES: SystemTable = SystemTable {
    table: "jadpo_handled_deliveries",
    columns: &[
        column("id", SqlType::Uuid, false),
        column("handled_at", SqlType::Timestamp, false),
    ],
    primary_key: &["id"],
    unique: &[],
    foreign_keys: &[],
    indexes: &[],
};

/// The events received from outside, by where they came from and the
/// sender's own id (area 11, "Receiving from outside"; chunk 7.7): a
/// webhook's provider event id, under `webhook <connection>`, so a repeat
/// is dropped. Committed with the event's outbox row, in the store webhook
/// events are kept in, and deleted by the tick once a provider no longer
/// sends it again. A webhook that needs a reply keeps the reply its
/// action gave, so a repeat is answered with it (chunk 7.7b): none while
/// the action runs.
pub const RECEIVED_EVENTS: SystemTable = SystemTable {
    table: "jadpo_received_events",
    columns: &[
        column("source", SqlType::Text, false),
        column("event_id", SqlType::Text, false),
        column("received_at", SqlType::Timestamp, false),
        column("reply", SqlType::Text, true),
    ],
    primary_key: &["source", "event_id"],
    unique: &[],
    foreign_keys: &[],
    indexes: &[SystemIndex {
        columns: &["received_at"],
        reason: "system received events",
    }],
};

/// Queue messages parked for an operator (area 12, "Events from outside";
/// chunk 7.7c): one whose type the queue doesn't list, or that can't be
/// read as its event, kept whole so nothing is dropped, in the store its
/// queue's events are kept in, once per queue and body. `status` is
/// `failed` until an operator retries it (`pending`, for the app's tick)
/// or dismisses it (`dismissed`, deleted by the tick 30 days on).
pub const PARKED_MESSAGES: SystemTable = SystemTable {
    table: "jadpo_parked_messages",
    columns: &[
        column("id", SqlType::Uuid, false),
        column("queue", SqlType::Text, false),
        column("digest", SqlType::Text, false),
        column("body", SqlType::Text, false),
        column("status", SqlType::Text, false),
        column("reason", SqlType::Text, false),
        column("event", SqlType::Text, true),
        column("message_id", SqlType::Text, true),
        column("problems", SqlType::Json, true),
        column("parked_at", SqlType::Timestamp, false),
        column("retried_at", SqlType::Timestamp, true),
    ],
    primary_key: &["id"],
    unique: &[SystemUnique {
        columns: &["queue", "digest"],
        rule: "system parked messages",
    }],
    foreign_keys: &[],
    indexes: &[SystemIndex {
        columns: &["status", "parked_at"],
        reason: "system parked messages",
    }],
};

/// What each poll last saw (area 11, "Polls"; chunk 7.6c), by its name,
/// `poll <connection>.<poll>`, in the store its events are kept in: when
/// it is next due, and the digest of the event its last answer made, so an
/// answer that changes nothing keeps nothing. A digest changes in the
/// transaction that keeps the event.
pub const POLLS: SystemTable = SystemTable {
    table: "jadpo_polls",
    columns: &[
        column("poll", SqlType::Text, false),
        column("next_at", SqlType::Timestamp, false),
        column("digest", SqlType::Text, true),
        column("last_change", SqlType::Timestamp, true),
        // The interval it was last claimed under, so only one claimed
        // under another counts as shortened (chunk 7.9c). Added to a
        // table that may hold rows, so it may be `NULL`: a row claimed
        // before.
        column("every_ms", SqlType::Int8, true),
    ],
    primary_key: &["poll"],
    unique: &[],
    foreign_keys: &[],
    indexes: &[],
};

/// Idempotency keys (area 7): one stored response per key and caller.
pub const IDEMPOTENCY_KEYS: SystemTable = SystemTable {
    table: "jadpo_idempotency_keys",
    columns: &[
        column("scope", SqlType::Text, false),
        column("key", SqlType::Text, false),
        column("request_digest", SqlType::Text, false),
        column("status", SqlType::Int2, true),
        column("response", SqlType::Json, true),
        column("stored_at", SqlType::Timestamp, false),
        column("kept_until", SqlType::Timestamp, false),
    ],
    primary_key: &["scope", "key"],
    unique: &[],
    foreign_keys: &[],
    indexes: &[SystemIndex {
        columns: &["kept_until"],
        reason: "system idempotency keys",
    }],
};

/// Each schedule's state (area 12, "Schedules"; chunk 6.2): the interval
/// its due times were worked out with, when it is next due, whether an
/// operator paused it, the lease that lets one instance run it at a time
/// and the run holding it, the last tick that reached it, and when its
/// last successful run started, which the alerts read (chunk 6.2b). In
/// `schedules: { store: }`.
pub const SCHEDULES: SystemTable = SystemTable {
    table: "jadpo_schedules",
    columns: &[
        column("schedule", SqlType::Text, false),
        column("every_ms", SqlType::Int8, false),
        column("next_run_at", SqlType::Timestamp, false),
        column("paused", SqlType::Bool, false),
        column("lease_until", SqlType::Timestamp, true),
        column("run_id", SqlType::Uuid, true),
        column("last_tick_at", SqlType::Timestamp, true),
        column("last_succeeded_at", SqlType::Timestamp, true),
        // The run an operator asked for with `jadpo schedules run`
        // against a deployment (chunk 6.2d): the id its record will have,
        // until the application's own tick takes it.
        column("run_requested", SqlType::Uuid, true),
        // The alerts a tick raised that still hold, each with when it was
        // raised (chunk 6.9), so of several instances' ticks one raises
        // each.
        column("alerts", SqlType::Json, true),
    ],
    primary_key: &["schedule"],
    unique: &[],
    foreign_keys: &[],
    indexes: &[],
};

/// Each run of a schedule, and each due tick it skipped and why (area 12,
/// "Watching schedules"; chunk 6.2): when it was due, started and ended,
/// how it ended with its failure's code and kind, never a value, how many
/// events it emitted, its trace, and how many items it processed. Kept 30
/// days.
pub const SCHEDULE_RUNS: SystemTable = SystemTable {
    table: "jadpo_schedule_runs",
    columns: &[
        column("id", SqlType::Uuid, false),
        column("schedule", SqlType::Text, false),
        column("due_slot", SqlType::Timestamp, false),
        column("started_at", SqlType::Timestamp, false),
        column("ended_at", SqlType::Timestamp, true),
        column("outcome", SqlType::Text, false),
        column("reason", SqlType::Text, true),
        column("failure", SqlType::Json, true),
        column("events", SqlType::Int4, true),
        column("traceparent", SqlType::Text, true),
        // The items a run processed, for a run that counts them: the
        // records a purge deleted (area 5; chunk 6.2c).
        column("items", SqlType::Int4, true),
    ],
    primary_key: &["id"],
    unique: &[],
    foreign_keys: &[],
    indexes: &[
        SystemIndex {
            columns: &["schedule", "started_at"],
            reason: "system schedule runs",
        },
        SystemIndex {
            columns: &["started_at"],
            reason: "system schedule runs",
        },
    ],
};

/// Browser sessions and API tokens (area 8): each found by its token's
/// verifier, never the token itself, with a browser session's CSRF
/// token's; an API token has none.
pub const SESSIONS: SystemTable = SystemTable {
    table: "jadpo_sessions",
    columns: &[
        column("id", SqlType::Uuid, false),
        column("principal_id", SqlType::Uuid, false),
        column("verifier", SqlType::Text, false),
        column("csrf_verifier", SqlType::Text, true),
        column("started_at", SqlType::Timestamp, false),
        column("ends_at", SqlType::Timestamp, false),
        // The `SignInMethod` it was started through, by name (area 8,
        // "How a session started"; chunk A10): `NULL` for one
        // `sessions.start` or `sessions.issue_token` started, and for
        // one kept before methods were recorded.
        column("sign_in_method", SqlType::Text, true),
        // `multi_factor` when that sign-in proved more than one factor (a
        // passkey, or a provider's `amr` naming a second one); `NULL` for
        // one factor.
        column("strength", SqlType::Text, true),
    ],
    primary_key: &["id"],
    unique: &[SystemUnique {
        columns: &["verifier"],
        rule: "system sessions",
    }],
    foreign_keys: &[],
    indexes: &[
        SystemIndex {
            columns: &["principal_id"],
            reason: "system sessions",
        },
        SystemIndex {
            columns: &["ends_at"],
            reason: "system sessions",
        },
    ],
};

/// Service keys (area 8): each found by its verifier, never the key
/// itself, with the service it signs in and when it expires.
pub const SERVICE_KEYS: SystemTable = SystemTable {
    table: "jadpo_service_keys",
    columns: &[
        column("id", SqlType::Uuid, false),
        column("principal_id", SqlType::Uuid, false),
        column("verifier", SqlType::Text, false),
        column("started_at", SqlType::Timestamp, false),
        column("ends_at", SqlType::Timestamp, false),
    ],
    primary_key: &["id"],
    unique: &[SystemUnique {
        columns: &["verifier"],
        rule: "system service keys",
    }],
    foreign_keys: &[],
    indexes: &[SystemIndex {
        columns: &["principal_id"],
        reason: "system service keys",
    }],
};

/// Passkeys (area 8, "Passkeys"): each found by its credential ID, kept
/// as the `WebAuthnCredential` it is, with its user and the user handle
/// its device keeps. Nothing in it signs anyone in.
pub const PASSKEYS: SystemTable = SystemTable {
    table: "jadpo_passkeys",
    columns: &[
        column("id", SqlType::Uuid, false),
        column("principal_id", SqlType::Uuid, false),
        column("credential_id", SqlType::Bytes, false),
        column("user_handle", SqlType::Bytes, false),
        column("credential", SqlType::Json, false),
        column("registered_at", SqlType::Timestamp, false),
        // The encoding `credential` was written in (chunk 5.9); `NULL`
        // for a passkey kept before encodings were recorded.
        column("encoding", SqlType::Int2, true),
    ],
    primary_key: &["id"],
    unique: &[SystemUnique {
        columns: &["credential_id"],
        rule: "system passkeys",
    }],
    foreign_keys: &[],
    indexes: &[SystemIndex {
        columns: &["principal_id"],
        reason: "system passkeys",
    }],
};

/// Passkey ceremonies under way (area 8, "Passkeys"): each found by its
/// challenge's verifier and used once, bound to the browser or session
/// that asked, with the state the ceremony checks the answer against.
pub const PASSKEY_CHALLENGES: SystemTable = SystemTable {
    table: "jadpo_passkey_challenges",
    columns: &[
        column("id", SqlType::Uuid, false),
        column("challenge", SqlType::Text, false),
        column("purpose", SqlType::Text, false),
        column("binding", SqlType::Text, false),
        column("principal_id", SqlType::Uuid, true),
        column("user_handle", SqlType::Bytes, true),
        column("state", SqlType::Text, false),
        column("expires_at", SqlType::Timestamp, false),
    ],
    primary_key: &["id"],
    unique: &[
        SystemUnique {
            columns: &["challenge"],
            rule: "system passkey challenges",
        },
        SystemUnique {
            columns: &["purpose", "binding"],
            rule: "system passkey challenges",
        },
    ],
    foreign_keys: &[],
    indexes: &[SystemIndex {
        columns: &["expires_at"],
        reason: "system passkey challenges",
    }],
};

/// Magic-link sign-ins (area 8, "Magic links"; chunk 7.3b): one row per
/// address, found by the address's digest, never the address. It holds
/// the latest request's link and code, each only as a verifier and filled
/// as its email is sent, the address sealed under the link's token so a
/// confirmed link can name it, where to go after signing in, the browser
/// a `same_browser:` link is bound to, and the counts the send and
/// wrong-code limits read. Nothing in it signs anyone in.
pub const SIGN_IN_REQUESTS: SystemTable = SystemTable {
    table: "jadpo_sign_in_requests",
    columns: &[
        column("id", SqlType::Uuid, false),
        // The address's digest in lower case, so the limits count every
        // way of writing it as one; and as it was written, which the
        // address a code is confirmed with must match.
        column("address", SqlType::Text, false),
        column("exact", SqlType::Text, true),
        // The latest request, which its email's delivery names; `NULL`
        // once its link or code is used.
        column("request", SqlType::Uuid, true),
        column("token", SqlType::Text, true),
        column("code", SqlType::Text, true),
        column("sealed", SqlType::Bytes, true),
        column("return_to", SqlType::Text, true),
        column("binding", SqlType::Text, true),
        column("wrong_codes", SqlType::Int4, false),
        column("requested_at", SqlType::Timestamp, true),
        column("expires_at", SqlType::Timestamp, true),
        // The hour the sends are counted in, and how many.
        column("sends_since", SqlType::Timestamp, true),
        column("sends", SqlType::Int4, false),
        // The UTC day the wrong codes are counted in, and how many.
        column("failed_since", SqlType::Timestamp, true),
        column("failed", SqlType::Int4, false),
        // When nothing in the row counts any more.
        column("kept_until", SqlType::Timestamp, false),
    ],
    primary_key: &["id"],
    unique: &[
        SystemUnique {
            columns: &["address"],
            rule: "system sign-in requests",
        },
        SystemUnique {
            columns: &["request"],
            rule: "system sign-in requests",
        },
        SystemUnique {
            columns: &["token"],
            rule: "system sign-in requests",
        },
    ],
    foreign_keys: &[],
    indexes: &[SystemIndex {
        columns: &["kept_until"],
        reason: "system sign-in requests",
    }],
};

/// Sign-ins through a provider under way (area 8, "Providers"; chunk
/// A7b): each found by its `state`'s digest and taken once, bound to the
/// browser that started it by its cookie's digest, with where to go
/// next. The PKCE verifier and the nonce are worked out from the cookie,
/// so nothing in it signs anyone in or can be exchanged.
pub const PROVIDER_SIGN_INS: SystemTable = SystemTable {
    table: "jadpo_provider_sign_ins",
    columns: &[
        column("id", SqlType::Uuid, false),
        column("provider", SqlType::Text, false),
        column("state", SqlType::Text, false),
        column("binding", SqlType::Text, false),
        column("return_to", SqlType::Text, false),
        column("expires_at", SqlType::Timestamp, false),
    ],
    primary_key: &["id"],
    unique: &[SystemUnique {
        columns: &["state"],
        rule: "system provider sign-ins",
    }],
    foreign_keys: &[],
    indexes: &[SystemIndex {
        columns: &["expires_at"],
        reason: "system provider sign-ins",
    }],
};

/// Who a first sign-in linked (area 8, "A person the app has not seen";
/// chunk A7): each way in and the subject it names, a provider's or an
/// external token's, to the user `on_first_sign_in:`'s action gave, so
/// the next sign-in finds them without it. The way in is
/// `SignInMethod`'s name, the subject as the token sent it. Nothing in it
/// signs anyone in on its own: the user's record is read beside it.
pub const SIGN_IN_LINKS: SystemTable = SystemTable {
    table: "jadpo_sign_in_links",
    columns: &[
        column("method", SqlType::Text, false),
        column("subject", SqlType::Text, false),
        column("user_id", SqlType::Uuid, false),
        column("linked_at", SqlType::Timestamp, false),
    ],
    primary_key: &["method", "subject"],
    unique: &[],
    foreign_keys: &[],
    indexes: &[SystemIndex {
        columns: &["user_id"],
        reason: "system sign-in links",
    }],
};

/// Batch runs (area 12, "How a run goes"; chunk 6.4): which `batches.each`
/// call each is and the shape it saved under, its `key:` and saved values
/// as their types' JSON, how far it got (the cursor of the next page, or
/// that it read them all) and its counts, its state, the lease its runner
/// holds it by, and the depth in a chain of events it started at.
pub const BATCH_RUNS: SystemTable = SystemTable {
    table: "jadpo_batch_runs",
    columns: &[
        column("id", SqlType::Uuid, false),
        column("batch", SqlType::Text, false),
        column("shape", SqlType::Text, false),
        column("key", SqlType::Text, true),
        column("captured", SqlType::Json, false),
        column("state", SqlType::Text, false),
        column("position", SqlType::Text, true),
        column("read_all", SqlType::Bool, false),
        column("items_done", SqlType::Int8, false),
        column("problems", SqlType::Int8, false),
        column("failure", SqlType::Text, true),
        column("lease_until", SqlType::Timestamp, true),
        column("started_at", SqlType::Timestamp, false),
        column("stepped_at", SqlType::Timestamp, false),
        column("finished_at", SqlType::Timestamp, true),
        // How deep in a chain of events the action that started it was,
        // which its items run at (chunk 6.6a); `NULL` in a run kept
        // before, which counts from the start.
        column("depth", SqlType::Int4, true),
        // The tenant the run is limited to, all zeros for none, which its
        // `key:` is unique within (chunk 6.6c); `NULL` in a run kept
        // before, which no later start finds.
        column("tenant", SqlType::Uuid, true),
        // The encoding its saved values and its `key:` were written in
        // (area 19, "Changing a standard type's encoding"; chunk 6.8);
        // `NULL` in a run kept before, whose values are encoding 1's.
        column("encoding", SqlType::Int2, true),
        // How many items the run is expected to hold, which
        // `batches.status` gives (chunk 6.4b3): its query's count, taken
        // as its first page is read, or worked out from its file's size
        // and how far into it the run is; `NULL` until then, and in a run
        // kept before.
        column("estimated_total", SqlType::Int8, true),
    ],
    primary_key: &["id"],
    unique: &[SystemUnique {
        columns: &["batch", "tenant", "key"],
        rule: "system batch runs",
    }],
    foreign_keys: &[],
    indexes: &[SystemIndex {
        columns: &["state", "lease_until"],
        reason: "system batch runs",
    }],
};

/// Each item of a run that had a problem: its row, column and rule, never
/// its value (area 12, "Problems never stop a run").
pub const BATCH_PROBLEMS: SystemTable = SystemTable {
    table: "jadpo_batch_problems",
    columns: &[
        column("run_id", SqlType::Uuid, false),
        column("row", SqlType::Int8, false),
        column("column", SqlType::Text, false),
        column("rule", SqlType::Text, false),
    ],
    primary_key: &["run_id", "row"],
    unique: &[],
    foreign_keys: &[SystemForeignKey {
        columns: &["run_id"],
        table: BATCH_RUNS.table,
    }],
    indexes: &[],
};

/// The runs `batches.cancel` asked to stop: a row of its own, so asking
/// never waits for the batch in hand, which stops the run once it
/// commits.
pub const BATCH_CANCELS: SystemTable = SystemTable {
    table: "jadpo_batch_cancels",
    columns: &[
        column("run_id", SqlType::Uuid, false),
        column("requested_at", SqlType::Timestamp, false),
    ],
    primary_key: &["run_id"],
    unique: &[],
    foreign_keys: &[],
    indexes: &[],
};

/// What an operator did to a delivery or a schedule's run with `jadpo
/// deliveries` (area 12, "What an operator sees"; area 14, "Operator commands";
/// chunk 6.12), in the store the item is in, written by the statement
/// that acts on it: when, what (`retry`, `dismiss`, `cancel`,
/// `resolve_done`, `resolve_not_done`), the item and its state then,
/// and who did it: the name the operator's machine gives, and the
/// database role PostgreSQL authenticated, which the statement reads
/// itself. The tick deletes a row 30 days after it was written, once its
/// delivery or run is gone (chunk 6.13).
pub const OPERATOR_ACTIONS: SystemTable = SystemTable {
    table: "jadpo_operator_actions",
    columns: &[
        column("id", SqlType::Uuid, false),
        column("acted_at", SqlType::Timestamp, false),
        column("action", SqlType::Text, false),
        // `delivery` or `schedule_run`.
        column("kind", SqlType::Text, false),
        column("item", SqlType::Uuid, false),
        // The subscriber's or the schedule's identity key, and a
        // delivery's event type's.
        column("target", SqlType::Text, false),
        column("event", SqlType::Text, true),
        column("was", SqlType::Text, false),
        column("operator", SqlType::Text, false),
        column("role", SqlType::Text, false),
    ],
    primary_key: &["id"],
    unique: &[],
    foreign_keys: &[],
    indexes: &[SystemIndex {
        columns: &["item"],
        reason: "system operator actions",
    }],
};

/// The record of an action with several changing calls (area 12,
/// "Several changing calls in one action"; chunk 7.5b): one row for each
/// run that has sent a step, keyed by the action's key, in the store the
/// action writes, beside its outbox. Its id is a digest of its scope and
/// key; it is deleted in the action's commit, its steps with it.
pub const STEP_RECORDS: SystemTable = SystemTable {
    table: "jadpo_step_records",
    columns: &[
        column("id", SqlType::Uuid, false),
        // Whose key it is (`delivery`, a route's caller's scope) and the
        // action's key (a delivery's id, a client's `Idempotency-Key`).
        column("scope", SqlType::Text, false),
        column("key", SqlType::Text, false),
        // The identity key of the action that is run.
        column("action", SqlType::Text, false),
        // The build it started under, and a digest of its action's step
        // shape (area 11, "Deploys").
        column("build", SqlType::Text, false),
        column("shape", SqlType::Text, false),
        // When its first run started: `clock.now` for every rerun.
        column("started_at", SqlType::Timestamp, false),
        // The generation of the runner that holds it, which every write
        // checks: a delivery's attempt, a route's claim.
        column("generation", SqlType::Int8, false),
        // `running`, or `diverged` with what differed (`problem`: the
        // step, its operation and the fields, never a value).
        column("state", SqlType::Text, false),
        column("problem", SqlType::Json, true),
        column("taken_at", SqlType::Timestamp, false),
        // When a tick raised the alert on unfinished records for it, once
        // it had been unfinished a day (chunk 7.5f).
        column("alerted_at", SqlType::Timestamp, true),
    ],
    primary_key: &["id"],
    unique: &[SystemUnique {
        columns: &["scope", "key"],
        rule: "system step records",
    }],
    foreign_keys: &[],
    indexes: &[],
};

/// Each step of a record (chunk 7.5b): written before it is sent, with
/// its operation, a keyed fingerprint of each top-level field of its
/// request and when it was first sent; its answer, encrypted, once it
/// comes. A step with no answer is in doubt.
pub const STEPS: SystemTable = SystemTable {
    table: "jadpo_steps",
    columns: &[
        column("record_id", SqlType::Uuid, false),
        column("position", SqlType::Int4, false),
        // `pay.payments.create`, `sheets.put`.
        column("operation", SqlType::Text, false),
        column("fingerprint", SqlType::Json, false),
        // How many times its key was replaced (a provider's
        // `new_key_on:`).
        column("key_generation", SqlType::Int4, false),
        column("dispatched_at", SqlType::Timestamp, false),
        // `answered` or `failed` once answered; `NULL` while in doubt.
        column("outcome", SqlType::Text, true),
        column("answer", SqlType::Bytes, true),
        column("answered_at", SqlType::Timestamp, true),
    ],
    primary_key: &["record_id", "position"],
    unique: &[],
    foreign_keys: &[SystemForeignKey {
        columns: &["record_id"],
        table: STEP_RECORDS.table,
    }],
    indexes: &[],
};

/// What the application's `ai` calls used (area 21, "Budgets"; chunk
/// 5.5v5), in the store `ai: { limits: }` keeps its counts in: one row
/// for each day (UTC) and window, each change one statement, so every
/// instance shares the counts and none is lost. A window is the
/// application's (`app`, no key), a caller's (`caller`, a digest of who
/// it is, never the caller), or a prompt's with its model (`prompt`,
/// `<prompt> <model>`), which `jadpo ai usage` reads. `reserved_*` is
/// what calls under way may still use, taken before each is sent and
/// given back when it is charged what it used. Never a prompt or an
/// answer.
pub const AI_USAGE: SystemTable = SystemTable {
    table: "jadpo_ai_usage",
    columns: &[
        column("day", SqlType::Date, false),
        column("scope", SqlType::Text, false),
        column("key", SqlType::Text, false),
        column("calls", SqlType::Int4, false),
        column("input_tokens", SqlType::Int8, false),
        column("output_tokens", SqlType::Int8, false),
        column("cached_tokens", SqlType::Int8, false),
        column("reserved_tokens", SqlType::Int8, false),
        // The estimated cost in millionths of `currency`'s major unit:
        // `NULL` currency while no call has had a price.
        column("cost_micros", SqlType::Int8, false),
        column("reserved_micros", SqlType::Int8, false),
        column("currency", SqlType::Text, true),
        column("touched_at", SqlType::Timestamp, false),
    ],
    primary_key: &["day", "scope", "key"],
    unique: &[],
    foreign_keys: &[],
    indexes: &[],
};

/// An audited entity's history (area 16, "Audit trail"; chunk B11): one
/// row per change to one of its records, written in the transaction that
/// makes the change, in the store the entity lives in. Rows are only ever
/// added, then deleted when their entity's `audit: { retention: }` ends
/// or with their record when erasure deletes it.
pub const AUDIT: SystemTable = SystemTable {
    table: "jadpo_audit",
    columns: &[
        // The entity's born name, which survives a rename.
        column("entity", SqlType::Text, false),
        column("record_id", SqlType::Uuid, false),
        // The change's place among the record's: 1 for its first. A change
        // holds its record's row lock, so two never take the same place.
        column("seq", SqlType::Int8, false),
        // The operation's time, as `clock.now` reads it.
        column("at", SqlType::Timestamp, false),
        // `AuditChange`'s variant: `created`, `updated` or `deleted`.
        column("change", SqlType::Text, false),
        // `AuditBy`'s variant, with the signed-in user's or service's `id`.
        column("by", SqlType::Text, false),
        column("user_id", SqlType::Uuid, true),
        column("service_id", SqlType::Uuid, true),
        // The outermost action the change was made through.
        column("action", SqlType::Text, true),
        // The id of the request that made it, when a request did.
        column("request", SqlType::Text, true),
        // The W3C trace id it was made in, which a subscriber's delivery
        // keeps from the request that emitted its event.
        column("trace", SqlType::Text, true),
        // Each changed field's name, and its value before and after as
        // canonical text, server-only fields left out.
        column("changes", SqlType::Json, false),
        // The encoding `changes` was written in (area 19, "Changing a
        // standard type's encoding").
        column("encoding", SqlType::Int2, false),
    ],
    primary_key: &["entity", "record_id", "seq"],
    unique: &[],
    foreign_keys: &[],
    indexes: &[SystemIndex {
        columns: &["entity", "at"],
        reason: "system audit trail",
    }],
};

/// One hourly cleanup lease per store. Completion is kept separately from the
/// lease: a failed, capped or interrupted pass is eligible on the next tick.
pub const HOUSEKEEPING: SystemTable = SystemTable {
    table: "jadpo_housekeeping",
    columns: &[
        column("slot", SqlType::Int4, false),
        column("lease_id", SqlType::Uuid, true),
        column("lease_until", SqlType::Timestamp, true),
        column("last_cleaned_at", SqlType::Timestamp, true),
    ],
    primary_key: &["slot"],
    unique: &[],
    foreign_keys: &[],
    indexes: &[],
};

/// Each feature flag's current state (area 16, "Changing a flag"; chunk
/// B9), in `flags: { store: }`: the value everyone gets, `NULL` for the
/// flag's `default:`; the named users' fixed values, a JSON object of
/// each user's `id` to a value; and the shares of users given a value by
/// a stable percentage, a JSON array of `[value, percent]` pairs taken in
/// order. A value is `true` or `false`, or an enum's variant by name. A
/// flag with no row gives everyone its `default:`.
pub const FLAGS: SystemTable = SystemTable {
    table: "jadpo_flags",
    columns: &[
        column("flag", SqlType::Text, false),
        column("value", SqlType::Text, true),
        column("users", SqlType::Json, true),
        column("shares", SqlType::Json, true),
        column("set_at", SqlType::Timestamp, false),
    ],
    primary_key: &["flag"],
    unique: &[],
    foreign_keys: &[],
    indexes: &[],
};

/// What must commit with an action's own writes, in every store that
/// holds an entity, since any of them can be written: the outbox, its
/// deliveries, handled records and idempotency keys; the log of what
/// operators did to those deliveries (chunk 6.12); and the circuit
/// breakers that hold them (chunk 7.2b).
pub const IN_EVERY_STORE: [&SystemTable; 6] = [
    &OUTBOX,
    &DELIVERIES,
    &HANDLED_DELIVERIES,
    &IDEMPOTENCY_KEYS,
    &OPERATOR_ACTIONS,
    &CIRCUIT_BREAKERS,
];

/// Schedules' state and runs, in `schedules: { store: }` (chunk 6.2), and
/// the log of what operators did to those runs (chunk 6.12).
pub const SCHEDULE_TABLES: [&SystemTable; 3] = [&SCHEDULES, &SCHEDULE_RUNS, &OPERATOR_ACTIONS];

/// What a store some batch walks keeps (chunk 6.4).
pub const BATCH_TABLES: [&SystemTable; 3] = [&BATCH_RUNS, &BATCH_PROBLEMS, &BATCH_CANCELS];

/// What the store an action with several changing calls keeps its record
/// in holds (chunk 7.5b).
pub const STEP_TABLES: [&SystemTable; 2] = [&STEP_RECORDS, &STEPS];

/// Every system table.
pub const ALL: [&SystemTable; 27] = [
    &OUTBOX,
    &DELIVERIES,
    &CIRCUIT_BREAKERS,
    &HANDLED_DELIVERIES,
    &IDEMPOTENCY_KEYS,
    &OPERATOR_ACTIONS,
    &SCHEDULES,
    &SCHEDULE_RUNS,
    &SESSIONS,
    &SERVICE_KEYS,
    &PASSKEYS,
    &PASSKEY_CHALLENGES,
    &SIGN_IN_REQUESTS,
    &SIGN_IN_LINKS,
    &PROVIDER_SIGN_INS,
    &BATCH_RUNS,
    &BATCH_PROBLEMS,
    &BATCH_CANCELS,
    &STEP_RECORDS,
    &STEPS,
    &RECEIVED_EVENTS,
    &PARKED_MESSAGES,
    &POLLS,
    &AI_USAGE,
    &AUDIT,
    &FLAGS,
    &HOUSEKEEPING,
];

/// The system table called `name`, without its prefix; `None` for a name
/// no system table has.
pub fn by_name(name: &str) -> Option<&'static SystemTable> {
    ALL.into_iter().find(|table| table.name() == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_table_is_listed_once_and_names_only_its_own_columns() {
        let mut names: Vec<&str> = ALL.iter().map(|table| table.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), ALL.len());
        for table in ALL {
            assert!(table.table.starts_with("jadpo_"), "{}", table.table);
            assert_eq!(by_name(table.name()), Some(table));
            let mut columns: Vec<&str> = table.columns.iter().map(|c| c.name).collect();
            columns.sort_unstable();
            columns.dedup();
            assert_eq!(columns.len(), table.columns.len(), "{}", table.table);
            let named = table
                .primary_key
                .iter()
                .chain(table.unique.iter().flat_map(|u| u.columns))
                .chain(table.foreign_keys.iter().flat_map(|f| f.columns))
                .chain(table.indexes.iter().flat_map(|i| i.columns));
            for column in named {
                assert!(table.column(column).is_some(), "{}.{column}", table.table);
            }
            for key in table.foreign_keys {
                assert!(
                    ALL.iter().any(|other| other.table == key.table),
                    "{}",
                    key.table
                );
            }
        }
        // A name no table has is none, never another table.
        assert_eq!(by_name("idempotency"), None);
        assert_eq!(by_name(""), None);
        let grouped = IN_EVERY_STORE
            .iter()
            .chain(&SCHEDULE_TABLES)
            .chain(&BATCH_TABLES)
            .chain(&STEP_TABLES);
        for table in grouped {
            assert!(ALL.contains(table), "{}", table.table);
        }
    }

    #[test]
    fn every_column_is_a_plain_type_its_postgresql_column_reads_back_as() {
        for table in ALL {
            for column in table.columns {
                let ty = column.sql_type;
                let text = ty.postgres_column();
                assert_eq!(
                    crate::SqlType::of_column(&text),
                    Some(ty),
                    "{}.{}",
                    table.table,
                    column.name
                );
                assert!(
                    !matches!(
                        ty,
                        crate::SqlType::Array(_)
                            | crate::SqlType::Range(_)
                            | crate::SqlType::Vector
                            | crate::SqlType::Geography
                            | crate::SqlType::Float8
                    ),
                    "{}.{}",
                    table.table,
                    column.name
                );
            }
        }
    }
}
