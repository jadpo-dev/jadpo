//! A condition on a related record (area 6, "Filtering by a related
//! record"; chunk 10.11c), as SQL: `EXISTS (SELECT 1 FROM <related> WHERE
//! <link> AND <condition> AND <its visible:> AND <its policy>)`, `NOT
//! EXISTS` under `not`. The related records a caller may not read, or
//! that its lifecycle hides, are not there for the subquery, as for any
//! read. A call without a caller runs without policy, so a line asking for
//! a related record is written twice: with the related entity's policy,
//! and without it.

use jadpo_check::data::EntityModel;
use jadpo_check::data::queries::{Condition, Related};
use jadpo_check::policy::Effect;
use jadpo_sql::policy::ANYONE;
use jadpo_sql::statement as text;

use super::{Planner, Reach, Statement, both, field};
use crate::policy::Row;

/// A read's `where:` lines as SQL, those asking for a related record
/// written with its policy and without.
#[derive(Default)]
pub(crate) struct Filter {
    /// The lines that ask for no related record.
    pub(crate) lines: Vec<String>,
    /// Those that do, with each related entity's policy.
    pub(crate) policed: Vec<String>,
    /// The same, without it.
    pub(crate) open: Vec<String>,
}

impl Planner<'_> {
    /// `filter`, each line a condition on `entity`, as SQL.
    pub(crate) fn filter(
        &mut self,
        statement: &mut Statement,
        entity: &EntityModel,
        filter: &[Condition],
    ) -> Option<Filter> {
        let mut out = Filter::default();
        for line in filter {
            if line.relates() {
                out.policed
                    .push(self.relating(statement, entity, line, false, true)?);
                out.open
                    .push(self.relating(statement, entity, line, false, false)?);
            } else {
                out.lines
                    .push(self.condition(statement, entity, line, false)?);
            }
        }
        Some(out)
    }

    /// A condition holding a related record, `negated` when a `not` above
    /// it was pushed down, under the related entity's policy when
    /// `policed`.
    fn relating(
        &mut self,
        statement: &mut Statement,
        entity: &EntityModel,
        condition: &Condition,
        negated: bool,
        policed: bool,
    ) -> Option<String> {
        Some(match condition {
            Condition::Related(related) => {
                self.exists(statement, entity, related, negated, policed)?
            }
            Condition::And(parts) | Condition::Or(parts) if condition.relates() => {
                let and = matches!(condition, Condition::And(_)) != negated;
                let mut rendered = Vec::new();
                for part in parts {
                    rendered.push(self.relating(statement, entity, part, negated, policed)?);
                }
                text::joined(&rendered, and)
            }
            Condition::Not(inner) if inner.relates() => {
                self.relating(statement, entity, inner, !negated, policed)?
            }
            other => self.condition(statement, entity, other, negated)?,
        })
    }

    /// `EXISTS (SELECT 1 ...)` of the related records `related` asks for,
    /// linked to the row of `entity` the statement reads, `NOT EXISTS`
    /// when `negated`.
    fn exists(
        &mut self,
        statement: &mut Statement,
        entity: &EntityModel,
        related: &Related,
        negated: bool,
        policed: bool,
    ) -> Option<String> {
        let target = self.model.entity(&related.entity)?;
        let (theirs, ours) = if related.parent {
            ("id", related.via.as_str())
        } else {
            (related.via.as_str(), "id")
        };
        let theirs = &field(target, theirs)?.column;
        let ours = &field(entity, ours)?.column;
        let mut tests = Vec::new();
        if let Some(inner) = &related.condition {
            tests.push(self.condition(statement, target, inner, false)?);
        }
        // `include_hidden: true` counts the records its lifecycle hides,
        // under its policy still (chunk 10.11d).
        let reach = if related.hidden {
            Reach::Every
        } else {
            Reach::Read
        };
        tests.extend(self.access(statement, target, reach)?);
        if policed {
            let policy = self.policy(statement, target, Effect::Read, &Row::Table)?;
            if policy != ANYONE {
                tests.push(policy);
            }
        }
        Some(text::related(
            &target.table,
            theirs,
            (&entity.table, ours),
            &tests,
            negated,
        ))
    }
}

/// [`both`], for a read whose `where:` lines `filter` gave: the related
/// records' lines with their policy in the statement with policy, and
/// without in the one a call without a caller runs.
pub(crate) fn both_filtered(
    write: &dyn Fn(&[String]) -> String,
    conditions: Vec<String>,
    policed: Vec<String>,
    open: Vec<String>,
    policy: String,
) -> (String, Option<String>) {
    if policed == open {
        let mut conditions = conditions;
        conditions.extend(policed);
        return both(write, conditions, policy);
    }
    let mut unpoliced = conditions.clone();
    unpoliced.extend(open);
    let mut held = conditions;
    held.extend(policed);
    if policy != ANYONE {
        held.push(policy);
    }
    (write(&held), Some(write(&unpoliced)))
}
