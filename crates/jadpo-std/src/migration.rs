//! One store's share of a migration (area 5, "Migrations"): what
//! `jadpo-check` reads from a project's migration folders and
//! `jadpo-runtime` applies, as `jadpo migrate apply` and the test database
//! run it, so both read the same three facts the same way.

/// One migration of one store, as its folder holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Migration {
    /// The folder's name, `20261020T1400_add_priority`: names sort in the
    /// order migrations apply.
    pub name: String,
    /// The digest a store records for it: of what the migration does to
    /// this store.
    pub digest: String,
    /// The store's SQL, run as one batch.
    pub sql: String,
}

/// The line that opens a backfill filled in batches (area 5, "Computed
/// backfills"; chunk 5.6b), as `jadpo migrate new` writes it: the field
/// and how each batch is worked out, then two statements, one counting
/// the rows left and one filling the next batch.
pub const BACKFILL_MARKER: &str = "-- backfill ";

/// What a store's section of a migration does, step by step: statements
/// run together in one transaction, and backfills filled in batches,
/// each batch in its own transaction (chunk 5.6b).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Statements run as one batch, in one transaction.
    Once(String),
    Batches(Batches),
}

/// A backfill filled in batches until a batch finds no row left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Batches {
    /// The field it fills, `Todo.priority`.
    pub field: String,
    /// How each batch is worked out, in the marker's words: `by UPDATE`.
    pub how: String,
    /// One statement giving how many rows are left to fill.
    pub count: String,
    /// One batch: a statement taking the key the last batch ended at as
    /// `$1` (a `uuid`, the nil UUID before the first) and giving the key
    /// this batch ended at, `NULL` when no row is left past it, and how
    /// many rows it filled.
    pub batch: String,
}

/// How a migration creates an extension, as `jadpo migrate new` writes it.
const CREATE_EXTENSION: &str = "CREATE EXTENSION IF NOT EXISTS ";

impl Migration {
    /// The PostgreSQL extensions it creates, in the order it creates them:
    /// pgvector's `vector` in the first migration with a vector column
    /// (area 21, chunk 5.5v1), which `jadpo migrate apply` checks the
    /// database offers before running anything, and the start-up check
    /// that it has.
    pub fn extensions(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for line in self.sql.lines() {
            let Some(rest) = line.trim().strip_prefix(CREATE_EXTENSION) else {
                continue;
            };
            let name = rest.trim_end_matches(';').trim();
            let name = match name.strip_prefix('"').and_then(|n| n.strip_suffix('"')) {
                Some(quoted) => quoted.replace("\"\"", "\""),
                None => name.to_string(),
            };
            if !name.is_empty() && !out.contains(&name) {
                out.push(name);
            }
        }
        out
    }

    /// Its steps, in order: everything up to a backfill's marker is one
    /// step, the backfill's two statements another, and so on.
    pub fn steps(&self) -> Vec<Step> {
        let mut out = Vec::new();
        let mut once = String::new();
        let mut lines = self.sql.lines();
        let close = |once: &mut String, out: &mut Vec<Step>| {
            let sql = once.trim();
            if !sql.is_empty() {
                out.push(Step::Once(sql.to_string()));
            }
            once.clear();
        };
        while let Some(line) = lines.next() {
            let Some(rest) = line.strip_prefix(BACKFILL_MARKER) else {
                once.push_str(line);
                once.push('\n');
                continue;
            };
            close(&mut once, &mut out);
            let (field, how) = rest.split_once(": ").unwrap_or((rest, ""));
            let how = how.split(',').next().unwrap_or_default().trim();
            let mut statement = || {
                let mut text = String::new();
                for line in lines.by_ref() {
                    text.push_str(line);
                    if let Some(done) = text.strip_suffix(';') {
                        return done.to_string();
                    }
                    text.push('\n');
                }
                text
            };
            let count = statement();
            let batch = statement();
            out.push(Step::Batches(Batches {
                field: field.to_string(),
                how: how.to_string(),
                count,
                batch,
            }));
        }
        close(&mut once, &mut out);
        out
    }

    /// Whether it fills a backfill in batches.
    pub fn has_batches(&self) -> bool {
        self.sql
            .lines()
            .any(|line| line.starts_with(BACKFILL_MARKER))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extensions_a_migration_creates_are_read_from_its_sql() {
        let migration = Migration {
            name: "20261020T1400_initial".into(),
            digest: "0".into(),
            sql: "CREATE EXTENSION IF NOT EXISTS \"vector\";\nCREATE TABLE \"article\" (\n    \"id\" uuid NOT NULL\n);\n".into(),
        };
        assert_eq!(migration.extensions(), ["vector"]);
        let none = Migration {
            sql: "CREATE TABLE \"article\" ();".into(),
            ..migration
        };
        assert!(none.extensions().is_empty());
    }

    #[test]
    fn a_backfill_in_batches_is_a_step_of_its_own() {
        let migration = Migration {
            name: "20261101T0900_priority".into(),
            digest: "0".into(),
            sql: "ALTER TABLE \"todo\" ADD COLUMN \"priority\" text;\n-- backfill Todo.priority: by UPDATE, 1000 rows a batch\nSELECT count(*) FROM \"todo\";\nWITH batch AS (\nSELECT 1\n) SELECT 2;\nALTER TABLE \"todo\" ALTER COLUMN \"priority\" SET NOT NULL;".into(),
        };
        assert!(migration.has_batches());
        assert_eq!(
            migration.steps(),
            vec![
                Step::Once("ALTER TABLE \"todo\" ADD COLUMN \"priority\" text;".into()),
                Step::Batches(Batches {
                    field: "Todo.priority".into(),
                    how: "by UPDATE".into(),
                    count: "SELECT count(*) FROM \"todo\"".into(),
                    batch: "WITH batch AS (\nSELECT 1\n) SELECT 2".into(),
                }),
                Step::Once("ALTER TABLE \"todo\" ALTER COLUMN \"priority\" SET NOT NULL;".into()),
            ]
        );
    }
}
