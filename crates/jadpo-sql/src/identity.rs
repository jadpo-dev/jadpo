//! The statements area 8's callers are looked up and their credentials
//! kept with (area 8; area 9, question 8): a session's, an API token's or a
//! service key's lookup by its verifier, with its caller's record in the
//! same statement, a passkey's by its credential ID, and the statements
//! listing, issuing and revoking credentials, each limited to holders
//! whose record the caller's policy lets it reach (`held`, which the plan
//! writes from the holder's entity's grants).

use crate::ident;

/// The column of a credential's table naming its holder.
pub fn holder() -> String {
    ident("principal_id")
}

/// A user's API tokens that haven't ended, oldest first, whose user the
/// caller may read: `$1` the user, `$2` the time now.
pub fn tokens(held: &str) -> String {
    format!(
        "SELECT \"id\", \"started_at\", \"ends_at\" FROM \"jadpo_sessions\" WHERE \"principal_id\" = $1 AND \"csrf_verifier\" IS NULL AND \"ends_at\" > $2 AND {held} ORDER BY \"started_at\", \"id\""
    )
}

/// Ends one API token, never a browser session, whose user the caller may
/// update: `$1` the token.
pub fn revoke_token(held: &str) -> String {
    format!(
        "DELETE FROM \"jadpo_sessions\" WHERE \"id\" = $1 AND \"csrf_verifier\" IS NULL AND {held}"
    )
}

/// Keeps a new service key, for a service the caller may update: `$1` to
/// `$5` its identity, its service, its verifier, when it starts and when
/// it ends.
pub fn issue_key(held: &str) -> String {
    format!(
        "INSERT INTO \"jadpo_service_keys\" (\"id\", \"principal_id\", \"verifier\", \"started_at\", \"ends_at\") SELECT $1, $2, $3, $4, $5 WHERE {held}"
    )
}

/// A service's keys that haven't expired, oldest first, whose service the
/// caller may read: `$1` the service, `$2` the time now.
pub fn keys(held: &str) -> String {
    format!(
        "SELECT \"id\", \"started_at\", \"ends_at\" FROM \"jadpo_service_keys\" WHERE \"principal_id\" = $1 AND \"ends_at\" > $2 AND {held} ORDER BY \"started_at\", \"id\""
    )
}

/// Deletes one service key, whose service the caller may update: `$1` the
/// key.
pub fn revoke_key(held: &str) -> String {
    format!("DELETE FROM \"jadpo_service_keys\" WHERE \"id\" = $1 AND {held}")
}

/// Whether a caller's record is active: its `active:` condition, written
/// for the record's own columns, holding, or `TRUE` when it has none.
pub fn active(condition: Option<&str>) -> String {
    match condition {
        Some(condition) => format!("({condition}) IS TRUE"),
        None => "TRUE".to_string(),
    }
}

/// The holder's record a credential `s` names.
pub fn held_by_credential() -> String {
    format!("{} = s.{}", ident("id"), ident("principal_id"))
}

/// How a passkey is found by its credential ID with its user's record
/// (area 8, "Passkeys"): `$1` the credential ID, `$2` the time now, read
/// from `table` beside `entity_table`'s record where `wheres` hold, its
/// `active` worked out as given.
pub fn passkey_lookup(table: &str, entity_table: &str, active: &str, wheres: &[String]) -> String {
    format!(
        "SELECT s.{id}, s.{credential}, s.{encoding}, s.{handle}, u.{id}, u.{active_name} FROM {table} AS s CROSS JOIN LATERAL (SELECT {id}, {active} AS {active_name} FROM {entity_table} WHERE {wheres}) AS u WHERE s.{credential_id} = $1 AND $2::timestamptz IS NOT NULL",
        id = ident("id"),
        credential = ident("credential"),
        encoding = ident("encoding"),
        handle = ident("user_handle"),
        active_name = ident("active"),
        table = ident(table),
        entity_table = ident(entity_table),
        wheres = wheres.join(" AND "),
        credential_id = ident("credential_id"),
    )
}

/// How a caller is looked up by a credential's verifier (area 8, question
/// 12): `$1` the verifier, `$2` the time now, read from `table` beside
/// `entity_table`'s record where `wheres` hold, its subject `subject`'s
/// column and its `active` worked out as given; for a session or an API
/// token (`session`), its `csrf_verifier` too, and after the rest how it
/// was started, its `sign_in_method` and `strength` (chunk A10).
pub fn caller_lookup(
    table: &str,
    session: bool,
    entity_table: &str,
    subject: &str,
    active: &str,
    wheres: &[String],
) -> String {
    let (csrf, started) = if session {
        (
            format!("s.{}, ", ident("csrf_verifier")),
            format!(", s.{}, s.{}", ident("sign_in_method"), ident("strength")),
        )
    } else {
        (String::new(), String::new())
    };
    format!(
        "SELECT s.{id}, {csrf}u.{id}, u.{subject_name}, u.{active_name}, s.{ends}{started} FROM {table} AS s CROSS JOIN LATERAL (SELECT {id}, {subject} AS {subject_name}, {active} AS {active_name} FROM {entity_table} WHERE {wheres}) AS u WHERE s.{verifier} = $1 AND s.{ends} > $2",
        id = ident("id"),
        subject_name = ident("subject"),
        active_name = ident("active"),
        table = ident(table),
        subject = ident(subject),
        entity_table = ident(entity_table),
        wheres = wheres.join(" AND "),
        verifier = ident("verifier"),
        ends = ident("ends_at"),
    )
}

/// How a magic link finds its user (area 8, "Magic links"): `$1` the
/// address it was sent to, exactly as the field holds it, or the user's
/// `id`, `$2` the time now, read from `entity_table` where `column` is
/// `$1` and `wheres` hold: the user's `id`, and its `active` worked out as
/// given.
pub fn user_by(entity_table: &str, column: &str, active: &str, wheres: &[String]) -> String {
    let mut conditions = vec![format!("{} = $1", ident(column))];
    conditions.extend(wheres.iter().cloned());
    conditions.push("$2::timestamptz IS NOT NULL".to_string());
    format!(
        "SELECT {id}, {active} AS {active_name} FROM {entity_table} WHERE {conditions} LIMIT 1",
        id = ident("id"),
        active_name = ident("active"),
        entity_table = ident(entity_table),
        conditions = conditions.join(" AND "),
    )
}

/// How an external token finds its user once `on_first_sign_in:` may
/// have linked one (area 8, "A person the app has not seen"; chunk A7), in
/// one read: the user whose `subject` column is `$1`, else the one
/// `links` links way in `$3` and subject `$4` to; `$2` the time now. Each
/// read from `entity_table` where `wheres` hold, giving its `id`, its
/// `active` worked out as given and its subject.
pub fn linked_user(
    links: &str,
    entity_table: &str,
    subject: &str,
    active: &str,
    wheres: &[String],
) -> String {
    let user = |extra: &str| {
        let mut conditions = vec![extra.to_string()];
        conditions.extend(wheres.iter().cloned());
        format!(
            "SELECT {id}, {active} AS {active_name}, {subject} AS {subject_name} FROM {entity_table} WHERE {conditions}",
            id = ident("id"),
            active_name = ident("active"),
            subject = ident(subject),
            subject_name = ident("subject"),
            entity_table = ident(entity_table),
            conditions = conditions.join(" AND "),
        )
    };
    format!(
        "SELECT u.{id}, u.{active_name}, u.{subject_name} FROM (SELECT 0 AS {found}, f.* FROM ({direct}) AS f UNION ALL SELECT 1, l.* FROM {links} AS s CROSS JOIN LATERAL ({linked}) AS l WHERE s.{method} = $3 AND s.{link_subject} = $4) AS u WHERE $2::timestamptz IS NOT NULL ORDER BY u.{found} LIMIT 1",
        id = ident("id"),
        active_name = ident("active"),
        subject_name = ident("subject"),
        found = ident("found"),
        direct = user(&format!("{} = $1", ident(subject))),
        linked = user(&format!("{} = s.{}", ident("id"), ident("user_id"))),
        links = ident(links),
        method = ident("method"),
        link_subject = ident("subject"),
    )
}

/// A user by `$1` its `id`, `$2` the time now, read from `entity_table`
/// where `wheres` hold: its `id`, its `active` worked out as given and its
/// `subject` column. The user `on_first_sign_in:`'s action gave (chunk A7).
pub fn user_with_subject(
    entity_table: &str,
    subject: &str,
    active: &str,
    wheres: &[String],
) -> String {
    let mut conditions = vec![format!("{} = $1", ident("id"))];
    conditions.extend(wheres.iter().cloned());
    conditions.push("$2::timestamptz IS NOT NULL".to_string());
    format!(
        "SELECT {id}, {active} AS {active_name}, {subject} AS {subject_name} FROM {entity_table} WHERE {conditions} LIMIT 1",
        id = ident("id"),
        active_name = ident("active"),
        subject = ident(subject),
        subject_name = ident("subject"),
        entity_table = ident(entity_table),
        conditions = conditions.join(" AND "),
    )
}

/// A user's address by its `id` (area 8, "Passkeys"; chunk 5.5z2): `$1`
/// the user's `id`, `$2` the time now, read from `entity_table`'s
/// `column` where `wheres` hold. What a person's device lists a new
/// passkey under.
pub fn address_of(entity_table: &str, column: &str, wheres: &[String]) -> String {
    let mut conditions = vec![format!("{} = $1", ident("id"))];
    conditions.extend(wheres.iter().cloned());
    conditions.push("$2::timestamptz IS NOT NULL".to_string());
    format!(
        "SELECT {column} FROM {entity_table} WHERE {conditions} LIMIT 1",
        column = ident(column),
        entity_table = ident(entity_table),
        conditions = conditions.join(" AND "),
    )
}
