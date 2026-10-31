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

use crate::ident;

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
    /// Its role column and the roles that count, each as SQL: a parameter
    /// bound to the role's stored value, or that value written where the
    /// statement takes no parameters (chunk 5.5w3b); `None` when any role
    /// does (tenancy: any membership in the tenant).
    pub roles: Option<(String, Vec<String>)>,
    /// Its lifecycle's `visible:`, written for its own columns, so a
    /// membership the lifecycle hides grants nothing.
    pub visible: Option<String>,
    /// For a membership whose roles need a way in (area 9, "Roles that
    /// need a way in"; chunk A10): each role that counts and, for one that
    /// needs a way in or a strength, the condition on the caller's
    /// sign-in that lets it count. In place of `roles`' test, so a role
    /// whose requirement the caller's session doesn't meet is no
    /// membership at all, in a grant or in its tenant.
    pub counts: Option<(String, Vec<(String, Option<String>)>)>,
}

/// Whether a role counts for a caller (chunk A10): `sign_in`, the
/// caller's way in, is one of `ways`, and when `multi_factor` is given,
/// it holds too.
pub fn requirement(sign_in: Option<(&str, &[String])>, multi_factor: Option<&str>) -> String {
    let mut parts = Vec::new();
    if let Some((caller, ways)) = sign_in {
        parts.push(format!("{caller} IN ({})", ways.join(", ")));
    }
    if let Some(multi_factor) = multi_factor {
        parts.push(format!("{multi_factor} IS TRUE"));
    }
    match parts.as_slice() {
        [] => "TRUE".to_string(),
        [one] => one.clone(),
        _ => format!("({})", parts.join(" AND ")),
    }
}

/// `counts`' test of the role column `column` under `alias`: any role that
/// needs nothing, or one whose requirement holds.
fn counted(alias: &str, column: &str, roles: &[(String, Option<String>)]) -> String {
    let column = format!("{alias}.{}", ident(column));
    let plain: Vec<&str> = roles
        .iter()
        .filter(|(_, needs)| needs.is_none())
        .map(|(role, _)| role.as_str())
        .collect();
    let mut parts = Vec::new();
    if !plain.is_empty() {
        parts.push(format!("{column} IN ({})", plain.join(", ")));
    }
    for (role, needs) in roles {
        if let Some(needs) = needs {
            parts.push(format!("({column} = {role} AND {needs})"));
        }
    }
    match parts.as_slice() {
        [] => "FALSE".to_string(),
        [one] => one.clone(),
        _ => format!("({})", parts.join(" OR ")),
    }
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
    if let Some((column, roles)) = &membership.counts {
        tests.push(counted(&alias, column, roles));
    } else if let Some((column, roles)) = &membership.roles {
        tests.push(format!(
            "{alias}.{} IN ({})",
            ident(column),
            roles.join(", ")
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
pub fn insert_when<S: AsRef<str>>(
    table: &str,
    columns: &[&str],
    values: &[String],
    condition: &str,
    returning: &[S],
) -> String {
    format!(
        "INSERT INTO {} ({})\nSELECT {}\nWHERE {condition}\nRETURNING {}",
        ident(table),
        crate::idents(columns),
        values.join(", "),
        crate::idents(returning)
    )
}

/// `NOT EXISTS (SELECT 1 FROM <table> AS <alias> WHERE <alias>.<column> =
/// <value> AND <alias>.<tenant> IS DISTINCT FROM <tenancy>)`: no record of
/// `table` whose `column` is `value` belongs to another tenant than
/// `tenancy` (area 9, "References stay inside the tenant"). A record that
/// isn't there passes, so its foreign key still reports it missing, and so
/// does a `none` reference.
pub fn outside_tenant(
    table: &str,
    alias: &str,
    column: &str,
    value: &str,
    tenant: &str,
    tenancy: &str,
) -> String {
    let alias = ident(alias);
    format!(
        "NOT EXISTS (SELECT 1 FROM {} AS {alias} WHERE {alias}.{} = {value} AND {alias}.{} IS DISTINCT FROM {tenancy})",
        ident(table),
        ident(column),
        ident(tenant)
    )
}

/// `(<system> AND (<tenant> IS NULL OR <at> = <tenant>))`: the system,
/// the caller of a schedule's run or a subscriber's delivery (area 12),
/// on a record of tenant `at`: in the tenant it is limited to, or any
/// tenant when it is limited to none (chunk 6.1a). `system` and `tenant`
/// are the caller's parameters, `false` and `NULL` for every request.
pub fn system_in_tenant(system: &str, tenant: &str, at: &str) -> String {
    format!("({system} AND ({tenant} IS NULL OR {at} = {tenant}))")
}

/// `NOT EXISTS (SELECT 1 FROM <table> AS <alias> WHERE <alias>.<scope> =
/// <value>)`: no membership of `table` is held in the record `value`
/// names, hidden or not (area 9, "Tenancy": founding a tenant).
pub fn unheld(table: &str, alias: &str, scope: &str, value: &str) -> String {
    let alias = ident(alias);
    format!(
        "NOT EXISTS (SELECT 1 FROM {} AS {alias} WHERE {alias}.{} = {value})",
        ident(table),
        ident(scope)
    )
}

/// `EXISTS (SELECT 1 FROM <table> AS <alias> WHERE <alias>.<scope> =
/// <value>)`: some membership of `table` is held in the record `value`
/// names, hidden or not (area 9, "Background work": the system adds a
/// membership only where someone belongs).
pub fn held_in(table: &str, alias: &str, scope: &str, value: &str) -> String {
    let alias = ident(alias);
    format!(
        "EXISTS (SELECT 1 FROM {} AS {alias} WHERE {alias}.{} = {value})",
        ident(table),
        ident(scope)
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

/// Every one of `parts` holding: [`ANYONE`] for none, the one itself, or
/// `a AND b` of several.
pub fn all_of(parts: &[String]) -> String {
    match parts {
        [] => ANYONE.to_string(),
        [one] => one.clone(),
        _ => parts.join(" AND "),
    }
}

/// `(a AND b ...)`: several conditions as one, inside another's `OR`.
pub fn grouped(parts: &[String]) -> String {
    format!("({})", parts.join(" AND "))
}

/// `<value> = <caller>`: a field of the record (a column or a new record's
/// parameter) naming the caller.
pub fn names_caller(value: &str, caller: &str) -> String {
    format!("{value} = {caller}")
}

/// A field's own grant `who` on a new record's field that may be `none`:
/// met by a field left `none`, which reaches no one.
pub fn unless_none(value: &str, who: &str) -> String {
    format!("({value} IS NULL OR {who})")
}

/// A field's own grant `who` on an update's field, met when the patch
/// doesn't supply it (`supplied` the parameter saying whether it does).
pub fn unless_unsupplied(supplied: &str, who: &str) -> String {
    format!("(NOT {supplied} OR {who})")
}

/// A field's value once a patch is written: `value` when `supplied`, its
/// stored one in `table`'s `column` otherwise.
pub fn supplied_value(table: &str, column: &str, supplied: &str, value: &str) -> String {
    format!(
        "CASE WHEN {supplied} THEN {value} ELSE {} END",
        qualified(table, column)
    )
}

/// `SELECT 1 WHERE <any of whos>`: whether a caller holds one of the roles
/// an action's gate names (area 9, `policy { actions: }`).
pub fn gate(whos: &[Who]) -> String {
    format!("SELECT 1 WHERE {}", any_of(whos))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn several_conditions_join_as_the_plan_wrote_them() {
        assert_eq!(all_of(&[]), ANYONE);
        assert_eq!(all_of(&["a".into(), "b".into()]), "a AND b");
        assert_eq!(grouped(&["a".into(), "b".into()]), "(a AND b)");
        assert_eq!(unless_none("$1", "w"), "($1 IS NULL OR w)");
        assert_eq!(unless_unsupplied("$2", "w"), "(NOT $2 OR w)");
        assert_eq!(gate(&[Who::Condition("$1".into())]), "SELECT 1 WHERE $1");
    }

    #[test]
    fn a_membership_is_an_exists_of_the_callers_current_role() {
        let membership = Membership {
            table: "company_membership".into(),
            alias: "jadpo_m".into(),
            member: "user_id".into(),
            caller: "$2".into(),
            scope: Some(("company_id".into(), qualified("project", "company_id"))),
            roles: Some(("role".into(), vec!["$3".into(), "$4".into()])),
            visible: None,
            counts: None,
        };
        assert_eq!(
            exists(&membership),
            "EXISTS (SELECT 1 FROM \"company_membership\" AS \"jadpo_m\" WHERE \"jadpo_m\".\"user_id\" = $2 AND \"jadpo_m\".\"company_id\" = \"project\".\"company_id\" AND \"jadpo_m\".\"role\" IN ($3, $4))"
        );
    }

    /// A role that needs a way in counts only when the caller's sign-in
    /// meets it; one that needs nothing counts as before (chunk A10).
    #[test]
    fn a_role_that_needs_a_way_in_counts_only_with_it() {
        let needs = requirement(Some(("$5", &["$6".to_string()])), Some("$7"));
        assert_eq!(needs, "($5 IN ($6) AND $7 IS TRUE)");
        let membership = Membership {
            table: "staff_member".into(),
            alias: "jadpo_m0".into(),
            member: "patron_id".into(),
            caller: "$2".into(),
            scope: None,
            roles: None,
            visible: None,
            counts: Some((
                "role".into(),
                vec![("$3".into(), Some(needs)), ("$4".into(), None)],
            )),
        };
        assert_eq!(
            exists(&membership),
            "EXISTS (SELECT 1 FROM \"staff_member\" AS \"jadpo_m0\" WHERE \"jadpo_m0\".\"patron_id\" = $2 AND (\"jadpo_m0\".\"role\" IN ($4) OR (\"jadpo_m0\".\"role\" = $3 AND ($5 IN ($6) AND $7 IS TRUE))))"
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

    #[test]
    fn the_system_is_in_its_own_tenant_or_any_when_it_has_none() {
        assert_eq!(
            system_in_tenant("$3", "$4", &qualified("chore", "company_id")),
            "($3 AND ($4 IS NULL OR \"chore\".\"company_id\" = $4))"
        );
    }
}
