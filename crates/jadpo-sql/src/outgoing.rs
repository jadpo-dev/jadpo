//! The statements the addresses customers register for outgoing webhooks
//! are kept and read with (area 16, "Outgoing webhooks"; chunk C1b): each
//! of the generated operations limited to records the caller's grants
//! reach (`held`, which the plan writes from the entity's grants, as for
//! a service's keys), and what a delivery reads as the system.

use crate::ident;

const ADDRESSES: &str = "\"jadpo_webhook_addresses\"";

/// `register_webhook`'s hold on the record `$1`, in `table`, when `held`
/// lets the caller update it: one row, how many addresses it has, the
/// record held until the action commits so two registrations for it count
/// in turn. No row when it may not, or there is no such record.
pub fn lock(table: &str, held: &str) -> String {
    format!(
        "SELECT (SELECT count(*) FROM {ADDRESSES} WHERE \"holder_id\" = $1) FROM {table} WHERE {id} = $1 AND {held} FOR NO KEY UPDATE",
        table = ident(table),
        id = ident("id"),
    )
}

/// A record's addresses, oldest first, never their secrets, when `held`
/// lets the caller read it: `$1` the record.
pub fn list(held: &str) -> String {
    format!(
        "SELECT \"id\", \"url\", \"registered_at\" FROM {ADDRESSES} WHERE \"holder_id\" = $1 AND {held} ORDER BY \"registered_at\", \"id\""
    )
}

/// Deletes one address, when `held` lets the caller update its record:
/// `$1` the address.
pub fn remove(held: &str) -> String {
    format!("DELETE FROM {ADDRESSES} WHERE \"id\" = $1 AND {held}")
}

/// The column of the addresses' table naming the record.
pub fn holder() -> String {
    ident("holder_id")
}

/// What a delivery reads, as the system: `$1` the record, each address's
/// `id`, `url` and sealed secret, oldest first, and none once the record
/// is gone from `table`.
pub fn addresses(table: &str) -> String {
    format!(
        "SELECT a.\"id\", a.\"url\", a.\"secret\" FROM {ADDRESSES} AS a WHERE a.\"holder_id\" = $1 AND EXISTS (SELECT 1 FROM {table} AS h WHERE h.{id} = a.\"holder_id\") ORDER BY a.\"registered_at\", a.\"id\"",
        table = ident(table),
        id = ident("id"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_statement_names_the_addresses_and_the_record() {
        assert_eq!(
            lock("shops", "TRUE"),
            "SELECT (SELECT count(*) FROM \"jadpo_webhook_addresses\" WHERE \"holder_id\" = $1) FROM \"shops\" WHERE \"id\" = $1 AND TRUE FOR NO KEY UPDATE"
        );
        assert!(list("TRUE").ends_with("ORDER BY \"registered_at\", \"id\""));
        assert!(addresses("shops").contains("EXISTS (SELECT 1 FROM \"shops\" AS h"));
    }
}
