//! The system tables (area 5, "System tables"; chunk 6.8): the tables the
//! runtime keeps its own records in, prefixed `jadpo_`, defined once,
//! here. The checker puts each in the stores that need it, so migrations
//! create them and drift in them fails `jadpo check`; the runtime's
//! statements read and write them, and a live test prepares every one of
//! those statements against the schema these give.
//!
//! Each table is a constant, and [`ALL`] lists them: a name no table has
//! is no table ([`by_name`]), never another one.

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

/// A column: its name, its PostgreSQL type and whether it may be `NULL`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemColumn {
    pub name: &'static str,
    pub sql_type: &'static str,
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

const fn column(name: &'static str, sql_type: &'static str, nullable: bool) -> SystemColumn {
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
        column("id", "uuid", false),
        // The event type's identity key, which survives a rename.
        column("event", "text", false),
        column("payload", "jsonb", false),
        column("emitted_at", "timestamptz(3)", false),
        // W3C trace context of the action that emitted it.
        column("traceparent", "text", true),
        // The encoding its payload was written in (area 19, "Changing a
        // standard type's encoding"; chunk 5.9), which a delivery reads
        // it by (chunk 6.8). The runtime always writes it; a migration
        // adds it to a table that may hold rows already, so it may be
        // `NULL`, a payload written before, read as encoding 1.
        column("encoding", "smallint", true),
        // How many events led to it from the request that set the chain
        // off, which stops a loop at 16 (area 12, "Limits"; chunk 6.1b).
        // Added to a table that may hold rows, so it may be `NULL`, read
        // as 0.
        column("depth", "integer", true),
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
        column("id", "uuid", false),
        column("outbox_id", "uuid", false),
        column("subscriber", "text", false),
        column("status", "text", false),
        column("attempts", "integer", false),
        column("next_attempt_at", "timestamptz(3)", true),
        column("last_failure", "jsonb", true),
        // `NULL` until it is first tried (chunk 6.1b).
        column("last_tried_at", "timestamptz(3)", true),
        // When an operator last retried it, or resolved it as not done
        // (`jadpo deliveries`; chunk 6.12): its `within:` counts from
        // then rather than from its event. `NULL` for one never retried.
        column("retried_at", "timestamptz(3)", true),
        // While it is `held` by its connection's circuit breaker (area
        // 12, question 18; chunk 7.2b): the connection, and since when.
        // `NULL` otherwise.
        column("held_for", "text", true),
        column("held_since", "timestamptz(3)", true),
        // How long it has been held before, in milliseconds, which its
        // `within:` doesn't count: held deliveries keep their budget.
        // `NULL` for one never held.
        column("held_ms", "bigint", true),
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
        column("connection", "text", false),
        column("state", "text", false),
        column("failures", "integer", false),
        column("opened_at", "timestamptz(3)", true),
        column("pauses", "integer", false),
        column("probe_at", "timestamptz(3)", true),
        column("probe_id", "uuid", true),
        column("probe_until", "timestamptz(3)", true),
        column("alerted_at", "timestamptz(3)", true),
    ],
    primary_key: &["connection"],
    unique: &[],
    foreign_keys: &[],
    indexes: &[],
};

/// A subscriber's handled record (area 12, "Outbox placement, settled";
/// chunk 6.1b): in the store its action writes, committed with its
/// writes, so a repeated delivery finds it and does nothing. Keyed by the
/// delivery's `id`, which the outbox's store holds: no reference across
/// stores.
pub const HANDLED_DELIVERIES: SystemTable = SystemTable {
    table: "jadpo_handled_deliveries",
    columns: &[
        column("id", "uuid", false),
        column("handled_at", "timestamptz(3)", false),
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
        column("source", "text", false),
        column("event_id", "text", false),
        column("received_at", "timestamptz(3)", false),
        column("reply", "text", true),
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
        column("id", "uuid", false),
        column("queue", "text", false),
        column("digest", "text", false),
        column("body", "text", false),
        column("status", "text", false),
        column("reason", "text", false),
        column("event", "text", true),
        column("message_id", "text", true),
        column("problems", "jsonb", true),
        column("parked_at", "timestamptz(3)", false),
        column("retried_at", "timestamptz(3)", true),
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
        column("poll", "text", false),
        column("next_at", "timestamptz(3)", false),
        column("digest", "text", true),
        column("last_change", "timestamptz(3)", true),
        // The interval it was last claimed under, so only one claimed
        // under another counts as shortened (chunk 7.9c). Added to a
        // table that may hold rows, so it may be `NULL`: a row claimed
        // before.
        column("every_ms", "bigint", true),
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
        column("scope", "text", false),
        column("key", "text", false),
        column("request_digest", "text", false),
        column("status", "smallint", true),
        column("response", "jsonb", true),
        column("stored_at", "timestamptz(3)", false),
        column("kept_until", "timestamptz(3)", false),
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
        column("schedule", "text", false),
        column("every_ms", "bigint", false),
        column("next_run_at", "timestamptz(3)", false),
        column("paused", "boolean", false),
        column("lease_until", "timestamptz(3)", true),
        column("run_id", "uuid", true),
        column("last_tick_at", "timestamptz(3)", true),
        column("last_succeeded_at", "timestamptz(3)", true),
        // The run an operator asked for with `jadpo schedules run`
        // against a deployment (chunk 6.2d): the id its record will have,
        // until the application's own tick takes it.
        column("run_requested", "uuid", true),
        // The alerts a tick raised that still hold, each with when it was
        // raised (chunk 6.9), so of several instances' ticks one raises
        // each.
        column("alerts", "jsonb", true),
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
        column("id", "uuid", false),
        column("schedule", "text", false),
        column("due_slot", "timestamptz(3)", false),
        column("started_at", "timestamptz(3)", false),
        column("ended_at", "timestamptz(3)", true),
        column("outcome", "text", false),
        column("reason", "text", true),
        column("failure", "jsonb", true),
        column("events", "integer", true),
        column("traceparent", "text", true),
        // The items a run processed, for a run that counts them: the
        // records a purge deleted (area 5; chunk 6.2c).
        column("items", "integer", true),
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
        column("id", "uuid", false),
        column("principal_id", "uuid", false),
        column("verifier", "text", false),
        column("csrf_verifier", "text", true),
        column("started_at", "timestamptz(3)", false),
        column("ends_at", "timestamptz(3)", false),
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
        column("id", "uuid", false),
        column("principal_id", "uuid", false),
        column("verifier", "text", false),
        column("started_at", "timestamptz(3)", false),
        column("ends_at", "timestamptz(3)", false),
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
        column("id", "uuid", false),
        column("principal_id", "uuid", false),
        column("credential_id", "bytea", false),
        column("user_handle", "bytea", false),
        column("credential", "jsonb", false),
        column("registered_at", "timestamptz(3)", false),
        // The encoding `credential` was written in (chunk 5.9); `NULL`
        // for a passkey kept before encodings were recorded.
        column("encoding", "smallint", true),
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
/// that asked, with the state `webauthn-rs` checks the answer against.
pub const PASSKEY_CHALLENGES: SystemTable = SystemTable {
    table: "jadpo_passkey_challenges",
    columns: &[
        column("id", "uuid", false),
        column("challenge", "text", false),
        column("purpose", "text", false),
        column("binding", "text", false),
        column("principal_id", "uuid", true),
        column("user_handle", "bytea", true),
        column("state", "text", false),
        column("expires_at", "timestamptz(3)", false),
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
        column("id", "uuid", false),
        // The address's digest in lower case, so the limits count every
        // way of writing it as one; and as it was written, which the
        // address a code is confirmed with must match.
        column("address", "text", false),
        column("exact", "text", true),
        // The latest request, which its email's delivery names; `NULL`
        // once its link or code is used.
        column("request", "uuid", true),
        column("token", "text", true),
        column("code", "text", true),
        column("sealed", "bytea", true),
        column("return_to", "text", true),
        column("binding", "text", true),
        column("wrong_codes", "integer", false),
        column("requested_at", "timestamptz(3)", true),
        column("expires_at", "timestamptz(3)", true),
        // The hour the sends are counted in, and how many.
        column("sends_since", "timestamptz(3)", true),
        column("sends", "integer", false),
        // The UTC day the wrong codes are counted in, and how many.
        column("failed_since", "timestamptz(3)", true),
        column("failed", "integer", false),
        // When nothing in the row counts any more.
        column("kept_until", "timestamptz(3)", false),
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

/// Batch runs (area 12, "How a run goes"; chunk 6.4): which `batches.each`
/// call each is and the shape it saved under, its `key:` and saved values
/// as their types' JSON, how far it got (the cursor of the next page, or
/// that it read them all) and its counts, its state, the lease its runner
/// holds it by, and the depth in a chain of events it started at.
pub const BATCH_RUNS: SystemTable = SystemTable {
    table: "jadpo_batch_runs",
    columns: &[
        column("id", "uuid", false),
        column("batch", "text", false),
        column("shape", "text", false),
        column("key", "text", true),
        column("captured", "jsonb", false),
        column("state", "text", false),
        column("position", "text", true),
        column("read_all", "boolean", false),
        column("items_done", "bigint", false),
        column("problems", "bigint", false),
        column("failure", "text", true),
        column("lease_until", "timestamptz(3)", true),
        column("started_at", "timestamptz(3)", false),
        column("stepped_at", "timestamptz(3)", false),
        column("finished_at", "timestamptz(3)", true),
        // How deep in a chain of events the action that started it was,
        // which its items run at (chunk 6.6a); `NULL` in a run kept
        // before, which counts from the start.
        column("depth", "integer", true),
        // The tenant the run is limited to, all zeros for none, which its
        // `key:` is unique within (chunk 6.6c); `NULL` in a run kept
        // before, which no later start finds.
        column("tenant", "uuid", true),
        // The encoding its saved values and its `key:` were written in
        // (area 19, "Changing a standard type's encoding"; chunk 6.8);
        // `NULL` in a run kept before, whose values are encoding 1's.
        column("encoding", "smallint", true),
        // How many items the run is expected to hold, which
        // `batches.status` gives (chunk 6.4b3): its query's count, taken
        // as its first page is read, or worked out from its file's size
        // and how far into it the run is; `NULL` until then, and in a run
        // kept before.
        column("estimated_total", "bigint", true),
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
        column("run_id", "uuid", false),
        column("row", "bigint", false),
        column("column", "text", false),
        column("rule", "text", false),
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
        column("run_id", "uuid", false),
        column("requested_at", "timestamptz(3)", false),
    ],
    primary_key: &["run_id"],
    unique: &[],
    foreign_keys: &[],
    indexes: &[],
};

/// What an operator did to a delivery or a schedule's run with `jadpo
/// deliveries` (area 12, "What an operator sees"; area 14, question 13;
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
        column("id", "uuid", false),
        column("acted_at", "timestamptz(3)", false),
        column("action", "text", false),
        // `delivery` or `schedule_run`.
        column("kind", "text", false),
        column("item", "uuid", false),
        // The subscriber's or the schedule's identity key, and a
        // delivery's event type's.
        column("target", "text", false),
        column("event", "text", true),
        column("was", "text", false),
        column("operator", "text", false),
        column("role", "text", false),
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
        column("id", "uuid", false),
        // Whose key it is (`delivery`, a route's caller's scope) and the
        // action's key (a delivery's id, a client's `Idempotency-Key`).
        column("scope", "text", false),
        column("key", "text", false),
        // The identity key of the action that is run.
        column("action", "text", false),
        // The build it started under, and a digest of its action's step
        // shape (area 11, "Deploys").
        column("build", "text", false),
        column("shape", "text", false),
        // When its first run started: `clock.now` for every rerun.
        column("started_at", "timestamptz(3)", false),
        // The generation of the runner that holds it, which every write
        // checks: a delivery's attempt, a route's claim.
        column("generation", "bigint", false),
        // `running`, or `diverged` with what differed (`problem`: the
        // step, its operation and the fields, never a value).
        column("state", "text", false),
        column("problem", "jsonb", true),
        column("taken_at", "timestamptz(3)", false),
        // When a tick raised the alert on unfinished records for it, once
        // it had been unfinished a day (chunk 7.5f).
        column("alerted_at", "timestamptz(3)", true),
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
        column("record_id", "uuid", false),
        column("position", "integer", false),
        // `pay.payments.create`, `sheets.put`.
        column("operation", "text", false),
        column("fingerprint", "jsonb", false),
        // How many times its key was replaced (a provider's
        // `new_key_on:`).
        column("key_generation", "integer", false),
        column("dispatched_at", "timestamptz(3)", false),
        // `answered` or `failed` once answered; `NULL` while in doubt.
        column("outcome", "text", true),
        column("answer", "bytea", true),
        column("answered_at", "timestamptz(3)", true),
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
        column("day", "date", false),
        column("scope", "text", false),
        column("key", "text", false),
        column("calls", "integer", false),
        column("input_tokens", "bigint", false),
        column("output_tokens", "bigint", false),
        column("cached_tokens", "bigint", false),
        column("reserved_tokens", "bigint", false),
        // The estimated cost in millionths of `currency`'s major unit:
        // `NULL` currency while no call has had a price.
        column("cost_micros", "bigint", false),
        column("reserved_micros", "bigint", false),
        column("currency", "text", true),
        column("touched_at", "timestamptz(3)", false),
    ],
    primary_key: &["day", "scope", "key"],
    unique: &[],
    foreign_keys: &[],
    indexes: &[],
};

/// One hourly cleanup lease per store. Completion is kept separately from the
/// lease: a failed, capped or interrupted pass is eligible on the next tick.
pub const HOUSEKEEPING: SystemTable = SystemTable {
    table: "jadpo_housekeeping",
    columns: &[
        column("slot", "integer", false),
        column("lease_id", "uuid", true),
        column("lease_until", "timestamptz(3)", true),
        column("last_cleaned_at", "timestamptz(3)", true),
    ],
    primary_key: &["slot"],
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
pub const ALL: [&SystemTable; 23] = [
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
    &BATCH_RUNS,
    &BATCH_PROBLEMS,
    &BATCH_CANCELS,
    &STEP_RECORDS,
    &STEPS,
    &RECEIVED_EVENTS,
    &PARKED_MESSAGES,
    &POLLS,
    &AI_USAGE,
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
}
