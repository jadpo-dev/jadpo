//! Every statement the runtime runs on its system tables (area 5,
//! "System tables"; chunk 6.8), by where it is declared: what a live test
//! prepares against the schema [`jadpo_std::system`] gives, so a
//! statement and the table it names can't drift apart unseen. A test
//! here finds every statement in the runtime's source that names a system
//! table, so one left out of the list fails it.
//!
//! What a plan builds on the system tables with an application's own
//! (a session read with its user's record) is the plan's, and its tests'.

/// Each statement, by `module::NAME`, with its SQL.
pub fn system_statements() -> Vec<(&'static str, String)> {
    vec![
        (
            "schedules::housekeeping::CLAIM",
            crate::schedules::housekeeping::CLAIM.to_string(),
        ),
        (
            "schedules::housekeeping::RELEASE",
            crate::schedules::housekeeping::RELEASE.to_string(),
        ),
        ("ai_budget::DEBIT", crate::ai_budget::DEBIT.to_string()),
        (
            "ai_budget::GIVE_BACK",
            crate::ai_budget::GIVE_BACK.to_string(),
        ),
        (
            "ai_budget::OLD_DAYS",
            crate::ai_budget::OLD_DAYS.to_string(),
        ),
        ("ai_budget::READ", crate::ai_budget::READ.to_string()),
        ("ai_budget::TAKE", crate::ai_budget::TAKE.to_string()),
        ("batches::ADVANCE", crate::batches::ADVANCE.to_string()),
        ("batches::CANCEL", crate::batches::CANCEL.to_string()),
        ("batches::CLAIM", crate::batches::CLAIM.to_string()),
        ("batches::END", crate::batches::END.to_string()),
        ("batches::ENDED", crate::batches::ENDED.to_string()),
        ("batches::LOCK", crate::batches::LOCK.to_string()),
        (
            "batches::calling::PEEK",
            crate::batches::calling::PEEK.to_string(),
        ),
        (
            "batches::file::PLACE",
            crate::batches::file::PLACE.to_string(),
        ),
        ("batches::PROBLEM", crate::batches::PROBLEM.to_string()),
        ("batches::PROBLEMS", crate::batches::PROBLEMS.to_string()),
        ("batches::RELEASE", crate::batches::RELEASE.to_string()),
        ("batches::START", crate::batches::START.to_string()),
        ("batches::STATUS", crate::batches::STATUS.to_string()),
        (
            "batches::UNKEPT_CANCELS",
            crate::batches::UNKEPT_CANCELS.to_string(),
        ),
        (
            "batches::UNFINISHED",
            crate::batches::UNFINISHED.to_string(),
        ),
        ("batches::WAITING", crate::batches::WAITING.to_string()),
        ("breaker::FAILED", crate::breaker::FAILED.to_string()),
        ("breaker::HOLD", crate::breaker::HOLD.to_string()),
        (
            "breaker::PROBE_UNUSED",
            crate::breaker::PROBE_UNUSED.to_string(),
        ),
        ("breaker::RAISE", crate::breaker::RAISE.to_string()),
        ("breaker::READ", crate::breaker::READ.to_string()),
        (
            "breaker::TAKE_PROBE",
            crate::breaker::TAKE_PROBE.to_string(),
        ),
        (
            "breaker::UNAVAILABLE",
            crate::breaker::UNAVAILABLE.to_string(),
        ),
        ("breaker::succeeded_sql", crate::breaker::succeeded_sql()),
        ("breaker::wake_sql", crate::breaker::wake_sql()),
        (
            "deliveries::ACT_DELIVERY",
            crate::deliveries::ACT_DELIVERY.to_string(),
        ),
        (
            "deliveries::ABANDON_WITH_RECORD",
            crate::deliveries::ABANDON_WITH_RECORD.to_string(),
        ),
        (
            "deliveries::CANCEL_REQUEST",
            crate::deliveries::CANCEL_REQUEST.to_string(),
        ),
        (
            "deliveries::CANCELLED_RUNS",
            crate::deliveries::CANCELLED_RUNS.to_string(),
        ),
        (
            "deliveries::DELIVERY_COUNTS",
            crate::deliveries::DELIVERY_COUNTS.to_string(),
        ),
        (
            "deliveries::FIND_REQUESTED",
            crate::deliveries::FIND_REQUESTED.to_string(),
        ),
        (
            "deliveries::LOG_RUN",
            crate::deliveries::LOG_RUN.to_string(),
        ),
        (
            "deliveries::REQUESTED_RUNS",
            crate::deliveries::REQUESTED_RUNS.to_string(),
        ),
        (
            "deliveries::RETRY_WITH_RECORD",
            crate::deliveries::RETRY_WITH_RECORD.to_string(),
        ),
        (
            "deliveries::SETTLE_RUN",
            crate::deliveries::SETTLE_RUN.to_string(),
        ),
        (
            "deliveries::deliveries_sql",
            crate::deliveries::deliveries_sql(),
        ),
        (
            "deliveries::dismissed_runs_sql",
            crate::deliveries::dismissed_runs_sql(),
        ),
        (
            "deliveries::find_delivery_sql",
            crate::deliveries::find_delivery_sql(),
        ),
        (
            "deliveries::find_run_sql",
            crate::deliveries::find_run_sql(),
        ),
        (
            "deliveries::find_request_sql",
            crate::deliveries::find_request_sql(),
        ),
        (
            "deliveries::request_counts_sql",
            crate::deliveries::request_counts_sql(),
        ),
        (
            "deliveries::requests_sql",
            crate::deliveries::requests_sql(),
        ),
        ("deliveries::runs_sql", crate::deliveries::runs_sql()),
        (
            "http::idempotency::CLAIM",
            crate::http::idempotency::CLAIM.to_string(),
        ),
        (
            "http::idempotency::RELEASE",
            crate::http::idempotency::RELEASE.to_string(),
        ),
        (
            "http::idempotency::SETTLED",
            crate::http::idempotency::SETTLED.to_string(),
        ),
        (
            "http::idempotency::STORE",
            crate::http::idempotency::STORE.to_string(),
        ),
        (
            "http::identity::END",
            crate::http::identity::END.to_string(),
        ),
        (
            "http::identity::ISSUE_KEY",
            crate::http::identity::ISSUE_KEY.to_string(),
        ),
        (
            "http::identity::KEYS",
            crate::http::identity::KEYS.to_string(),
        ),
        (
            "http::identity::REVOKE_KEY",
            crate::http::identity::REVOKE_KEY.to_string(),
        ),
        (
            "http::identity::REVOKE_TOKEN",
            crate::http::identity::REVOKE_TOKEN.to_string(),
        ),
        (
            "http::identity::START",
            crate::http::identity::START.to_string(),
        ),
        (
            "http::identity::TOKENS",
            crate::http::identity::TOKENS.to_string(),
        ),
        (
            "http::magic_links::ASKED",
            crate::http::magic_links::ASKED.to_string(),
        ),
        (
            "http::magic_links::BY_ADDRESS",
            crate::http::magic_links::BY_ADDRESS.to_string(),
        ),
        (
            "http::magic_links::BY_TOKEN",
            crate::http::magic_links::BY_TOKEN.to_string(),
        ),
        (
            "http::magic_links::ENSURE",
            crate::http::magic_links::ENSURE.to_string(),
        ),
        (
            "http::magic_links::LOCK",
            crate::http::magic_links::LOCK.to_string(),
        ),
        (
            "http::magic_links::MINT",
            crate::http::magic_links::MINT.to_string(),
        ),
        (
            "http::magic_links::SWEEP",
            crate::http::magic_links::SWEEP.to_string(),
        ),
        (
            "http::magic_links::USED",
            crate::http::magic_links::USED.to_string(),
        ),
        (
            "http::magic_links::WRONG",
            crate::http::magic_links::WRONG.to_string(),
        ),
        (
            "http::passkeys::ADD",
            crate::http::passkeys::ADD.to_string(),
        ),
        (
            "http::passkeys::LIST",
            crate::http::passkeys::LIST.to_string(),
        ),
        (
            "http::passkeys::PUT_CHALLENGE",
            crate::http::passkeys::PUT_CHALLENGE.to_string(),
        ),
        (
            "http::passkeys::REMOVE",
            crate::http::passkeys::REMOVE.to_string(),
        ),
        (
            "http::passkeys::RENAME",
            crate::http::passkeys::RENAME.to_string(),
        ),
        (
            "http::passkeys::SWEEP",
            crate::http::passkeys::SWEEP.to_string(),
        ),
        (
            "http::passkeys::TAKE",
            crate::http::passkeys::TAKE.to_string(),
        ),
        (
            "http::passkeys::USED",
            crate::http::passkeys::USED.to_string(),
        ),
        ("outbox::CLAIM", crate::outbox::CLAIM.to_string()),
        ("outbox::DUE", crate::outbox::DUE.to_string()),
        (
            "outbox::FIND_HANDLED",
            crate::outbox::FIND_HANDLED.to_string(),
        ),
        ("outbox::GIVE_UP", crate::outbox::GIVE_UP.to_string()),
        ("outbox::HANDED_ON", crate::outbox::HANDED_ON.to_string()),
        (
            "outbox::MARK_SENDING",
            crate::outbox::MARK_SENDING.to_string(),
        ),
        (
            "outbox::INSERT_DELIVERIES",
            crate::outbox::INSERT_DELIVERIES.to_string(),
        ),
        (
            "outbox::INSERT_HANDLED",
            crate::outbox::INSERT_HANDLED.to_string(),
        ),
        (
            "outbox::INSERT_OUTBOX",
            crate::outbox::INSERT_OUTBOX.to_string(),
        ),
        ("outbox::PARK", crate::outbox::PARK.to_string()),
        ("outbox::PARK_LOST", crate::outbox::PARK_LOST.to_string()),
        ("outbox::RELEASE", crate::outbox::RELEASE.to_string()),
        ("outbox::SETTLE", crate::outbox::SETTLE.to_string()),
        ("schedules::CLAIM", crate::schedules::CLAIM.to_string()),
        (
            "schedules::CLAIM_REQUESTED",
            crate::schedules::CLAIM_REQUESTED.to_string(),
        ),
        ("schedules::ENDED", crate::schedules::ENDED.to_string()),
        (
            "schedules::ENDED_SERVICE_KEYS",
            crate::schedules::ENDED_SERVICE_KEYS.to_string(),
        ),
        (
            "schedules::ENDED_SESSIONS",
            crate::schedules::ENDED_SESSIONS.to_string(),
        ),
        ("schedules::ENSURE", crate::schedules::ENSURE.to_string()),
        (
            "schedules::EXPIRED_KEYS",
            crate::schedules::EXPIRED_KEYS.to_string(),
        ),
        ("schedules::HANDLED", crate::schedules::HANDLED.to_string()),
        (
            "schedules::OLD_DELIVERY_ACTIONS",
            crate::schedules::OLD_DELIVERY_ACTIONS.to_string(),
        ),
        (
            "schedules::OLD_RUN_ACTIONS",
            crate::schedules::OLD_RUN_ACTIONS.to_string(),
        ),
        (
            "schedules::OLD_RUNS",
            crate::schedules::OLD_RUNS.to_string(),
        ),
        ("schedules::RECORD", crate::schedules::RECORD.to_string()),
        ("schedules::REGRID", crate::schedules::REGRID.to_string()),
        ("schedules::RELEASE", crate::schedules::RELEASE.to_string()),
        ("schedules::SETTLED", crate::schedules::SETTLED.to_string()),
        (
            "schedules::SETTLED_BY_HAND",
            crate::schedules::SETTLED_BY_HAND.to_string(),
        ),
        ("schedules::SKIP", crate::schedules::SKIP.to_string()),
        ("schedules::TICKED", crate::schedules::TICKED.to_string()),
        (
            "schedules::watch::CLAIM_NOW",
            crate::schedules::watch::CLAIM_NOW.to_string(),
        ),
        (
            "schedules::watch::METRICS",
            crate::schedules::watch::METRICS.to_string(),
        ),
        (
            "schedules::watch::RAISE",
            crate::schedules::watch::RAISE.to_string(),
        ),
        (
            "schedules::watch::REQUEST",
            crate::schedules::watch::REQUEST.to_string(),
        ),
        (
            "schedules::watch::SET_PAUSED",
            crate::schedules::watch::SET_PAUSED.to_string(),
        ),
        (
            "schedules::watch::STATES",
            crate::schedules::watch::STATES.to_string(),
        ),
        (
            "schedules::watch::STREAKS",
            crate::schedules::watch::STREAKS.to_string(),
        ),
        (
            "schedules::watch::WITHDRAW",
            crate::schedules::watch::WITHDRAW.to_string(),
        ),
        (
            "schedules::watch::latest_runs_sql",
            crate::schedules::watch::latest_runs_sql(),
        ),
        (
            "schedules::watch::run_sql",
            crate::schedules::watch::run_sql(),
        ),
        (
            "schedules::watch::history_sql",
            crate::schedules::watch::history_sql(),
        ),
        ("steps::ANSWER", crate::steps::ANSWER.to_string()),
        ("steps::CREATE", crate::steps::CREATE.to_string()),
        ("steps::DELETE", crate::steps::DELETE.to_string()),
        ("steps::DIVERGE", crate::steps::DIVERGE.to_string()),
        ("steps::NEW_KEY", crate::steps::NEW_KEY.to_string()),
        ("steps::OPEN", crate::steps::OPEN.to_string()),
        ("steps::SEND", crate::steps::SEND.to_string()),
        ("steps::STEPS", crate::steps::STEPS.to_string()),
        (
            "unfinished::ABANDON",
            crate::unfinished::ABANDON.to_string(),
        ),
        (
            "unfinished::ABANDON_REQUEST",
            crate::unfinished::ABANDON_REQUEST.to_string(),
        ),
        (
            "unfinished::ABANDONED",
            crate::unfinished::ABANDONED.to_string(),
        ),
        (
            "unfinished::OLD_ACTIONS",
            crate::unfinished::OLD_ACTIONS.to_string(),
        ),
        (
            "unfinished::OLD_ANSWERS",
            crate::unfinished::OLD_ANSWERS.to_string(),
        ),
        ("unfinished::METRIC", crate::unfinished::METRIC.to_string()),
        ("unfinished::RAISE", crate::unfinished::RAISE.to_string()),
        ("unfinished::RETRY", crate::unfinished::RETRY.to_string()),
        ("unfinished::STEPS", crate::unfinished::STEPS.to_string()),
        ("unfinished::by_id_sql", crate::unfinished::by_id_sql()),
        (
            "unfinished::of_delivery_sql",
            crate::unfinished::of_delivery_sql(),
        ),
        (
            "unfinished::unfinished_sql",
            crate::unfinished::unfinished_sql(),
        ),
        (
            "webhooks::OLD_RECEIVED",
            crate::webhooks::OLD_RECEIVED.to_string(),
        ),
        ("webhooks::RECEIVE", crate::webhooks::RECEIVE.to_string()),
        ("receiving::PARK", crate::receiving::PARK.to_string()),
        ("polls::CLAIM", crate::polls::CLAIM.to_string()),
        ("polls::AGAIN", crate::polls::AGAIN.to_string()),
        ("polls::CHANGED", crate::polls::CHANGED.to_string()),
        ("receiving::RETRIED", crate::receiving::RETRIED.to_string()),
        ("receiving::DONE", crate::receiving::DONE.to_string()),
        (
            "receiving::OLD_DISMISSED",
            crate::receiving::OLD_DISMISSED.to_string(),
        ),
        (
            "receiving::OLD_MESSAGE_ACTIONS",
            crate::receiving::OLD_MESSAGE_ACTIONS.to_string(),
        ),
        (
            "deliveries::ACT_MESSAGE",
            crate::deliveries::ACT_MESSAGE.to_string(),
        ),
        (
            "deliveries::MESSAGE_COUNTS",
            crate::deliveries::MESSAGE_COUNTS.to_string(),
        ),
        (
            "deliveries::messages_sql",
            crate::deliveries::messages_sql(),
        ),
        (
            "deliveries::find_message_sql",
            crate::deliveries::find_message_sql(),
        ),
        (
            "webhooks::replies::CLAIM",
            crate::webhooks::replies::CLAIM.to_string(),
        ),
        (
            "webhooks::replies::FIND",
            crate::webhooks::replies::FIND.to_string(),
        ),
        (
            "webhooks::replies::KEEP",
            crate::webhooks::replies::KEEP.to_string(),
        ),
        (
            "webhooks::replies::RELEASE",
            crate::webhooks::replies::RELEASE.to_string(),
        ),
        (
            "webhooks::replies::RETAKE",
            crate::webhooks::replies::RETAKE.to_string(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The system tables `sql` names, without their prefix.
    fn tables(sql: &str) -> Vec<&str> {
        sql.split("\"jadpo_")
            .skip(1)
            .filter_map(|rest| rest.split_once('"').map(|(name, _)| name))
            .collect()
    }

    #[test]
    fn every_statement_names_only_system_tables_and_their_columns() {
        for (name, sql) in system_statements() {
            let named = tables(&sql);
            assert!(!named.is_empty(), "{name} names no system table");
            for table in named {
                assert!(
                    jadpo_std::system::by_name(table).is_some(),
                    "{name} names `jadpo_{table}`, which no system table is"
                );
            }
        }
    }

    /// Every `const` in the runtime's source whose statement names a
    /// system table is listed, by its module and name.
    #[test]
    fn every_statement_on_a_system_table_is_listed() {
        let listed: Vec<&str> = system_statements()
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut found = Vec::new();
        let mut folders = vec![src.clone()];
        while let Some(folder) = folders.pop() {
            for entry in std::fs::read_dir(&folder).expect("the source").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    folders.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let module = path
                    .strip_prefix(&src)
                    .expect("under src")
                    .with_extension("")
                    .to_string_lossy()
                    .replace('/', "::")
                    .trim_end_matches("::mod")
                    .to_string();
                let text = std::fs::read_to_string(&path).expect("a source file");
                let text = text.split("#[cfg(test)]").next().unwrap_or_default();
                for part in text.split("const ").skip(1) {
                    let Some((name, rest)) = part.split_once(": &str =") else {
                        continue;
                    };
                    let statement = rest.split("\";").next().unwrap_or_default();
                    // A savepoint's name isn't a table.
                    if tables(statement).is_empty() || statement.contains("SAVEPOINT") {
                        continue;
                    }
                    found.push(format!("{module}::{}", name.trim()));
                }
            }
        }
        assert!(found.len() > 50, "{found:?}");
        for name in found {
            assert!(listed.contains(&name.as_str()), "{name} isn't listed");
        }
    }
}
