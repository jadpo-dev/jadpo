//! Policy as SQL (area 9, "When policy says no"): the conditions grants
//! and tenancy add to a statement's `WHERE`, and the create that writes a
//! row only when its grants hold. The plan decides which grants apply and
//! where each value comes from; this writes them as PostgreSQL does.
//!
//! A grant is a test of the row a statement reaches, qualified with its
//! table so a membership's subquery can name it, or of a new record's
//! parameters; a membership is an `EXISTS` of the caller's current
//! membership, its table under an alias of its own so a membership's
//! grants can test its own table.

use crate::{ident, text_literal};

/// One way a grant lets a caller in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Who {
    /// `<value> = <caller>`: a field of the record (a column or a new
    /// record's parameter) against the caller's `id`.
    Equals { value: String, caller: String },
    /// A current membership of the caller's.
    Member(Membership),
    /// A condition already written, such as `$3`, whether anyone is
    /// signed in.
    Condition(String),
    /// Anyone at all.
    Anyone,
}

/// A caller's membership, as an `EXISTS`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Membership {
    pub table: String,
    /// The alias its table takes in the subquery.
    pub alias: String,
    /// Its column naming who holds it.
    pub member: String,
    /// The caller's `id`.
    pub caller: String,
    /// Its scope column and what it must equal: the record's reference,
    /// qualified, or a new record's parameter. `None` for a membership
    /// across the whole application.
    pub scope: Option<(String, String)>,
    /// Its role column and the roles that count; `None` when any role
    /// does (tenancy: any membership in the tenant).
    pub roles: Option<(String, Vec<String>)>,
    /// Its lifecycle's `visible:`, written for its own columns, so a
    /// membership the lifecycle hides grants nothing.
    pub visible: Option<String>,
}

/// `"table"."column"`: a column of the row a statement reaches, named so a
/// subquery can read it.
pub fn qualified(table: &str, column: &str) -> String {
    format!("{}.{}", ident(table), ident(column))
}

/// The membership as `EXISTS (SELECT 1 FROM ... WHERE ...)`.
pub fn exists(membership: &Membership) -> String {
    let alias = ident(&membership.alias);
    let mut tests = vec![format!(
        "{alias}.{} = {}",
        ident(&membership.member),
        membership.caller
    )];
    if let Some((column, value)) = &membership.scope {
        tests.push(format!("{alias}.{} = {value}", ident(column)));
    }
    if let Some((column, roles)) = &membership.roles {
        let listed: Vec<String> = roles.iter().map(|role| text_literal(role)).collect();
        tests.push(format!(
            "{alias}.{} IN ({})",
            ident(column),
            listed.join(", ")
        ));
    }
    if let Some(visible) = &membership.visible {
        tests.push(visible.clone());
    }
    format!(
        "EXISTS (SELECT 1 FROM {} AS {alias} WHERE {})",
        ident(&membership.table),
        tests.join(" AND ")
    )
}

/// The condition that lets anyone in, which a statement leaves out.
pub const ANYONE: &str = "TRUE";

/// Whether any of `whos` lets the caller in: `FALSE` for none, [`ANYONE`]
/// when anyone may.
pub fn any_of(whos: &[Who]) -> String {
    if whos.contains(&Who::Anyone) {
        return ANYONE.to_string();
    }
    let parts: Vec<String> = whos
        .iter()
        .map(|who| match who {
            Who::Equals { value, caller } => format!("{value} = {caller}"),
            Who::Member(membership) => exists(membership),
            Who::Condition(condition) => condition.clone(),
            Who::Anyone => "TRUE".to_string(),
        })
        .collect();
    match parts.as_slice() {
        [] => "FALSE".to_string(),
        [one] => one.clone(),
        _ => format!("({})", parts.join(" OR ")),
    }
}

/// `INSERT` of `values` into `columns` of `table` only when `condition`
/// holds of them, returning `returning`: no row when policy refuses.
pub fn insert_when(
    table: &str,
    columns: &[&str],
    values: &[String],
    condition: &str,
    returning: &[&str],
) -> String {
    format!(
        "INSERT INTO {} ({})\nSELECT {}\nWHERE {condition}\nRETURNING {}",
        ident(table),
        crate::idents(columns),
        values.join(", "),
        crate::idents(returning)
    )
}

/// `<holder> IN (SELECT "id" FROM <table> WHERE <conditions>)`: a
/// credential whose holder (a column or a parameter) has a record the
/// conditions let in (area 9, question 8).
pub fn held_by(holder: &str, table: &str, conditions: &[String]) -> String {
    let mut inner = format!("SELECT {} FROM {}", ident("id"), ident(table));
    for (i, condition) in conditions.iter().enumerate() {
        inner.push_str(if i == 0 { " WHERE " } else { " AND " });
        inner.push_str(condition);
    }
    format!("{holder} IN ({inner})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_membership_is_an_exists_of_the_callers_current_role() {
        let membership = Membership {
            table: "company_membership".into(),
            alias: "jadpo_m".into(),
            member: "user_id".into(),
            caller: "$2".into(),
            scope: Some(("company_id".into(), qualified("project", "company_id"))),
            roles: Some(("role".into(), vec!["editor".into(), "owner".into()])),
            visible: None,
        };
        assert_eq!(
            exists(&membership),
            "EXISTS (SELECT 1 FROM \"company_membership\" AS \"jadpo_m\" WHERE \"jadpo_m\".\"user_id\" = $2 AND \"jadpo_m\".\"company_id\" = \"project\".\"company_id\" AND \"jadpo_m\".\"role\" IN ('editor', 'owner'))"
        );
    }

    #[test]
    fn grants_are_any_of_their_entries() {
        assert_eq!(any_of(&[]), "FALSE");
        assert_eq!(any_of(&[Who::Anyone, Who::Condition("$1".into())]), "TRUE");
        assert_eq!(
            any_of(&[
                Who::Equals {
                    value: qualified("todo", "owner_id"),
                    caller: "$1".into()
                },
                Who::Condition("$2".into())
            ]),
            "(\"todo\".\"owner_id\" = $1 OR $2)"
        );
    }
}
