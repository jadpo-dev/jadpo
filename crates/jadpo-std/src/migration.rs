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

/// How a backfill with an operation that has no SQL form is worked out,
/// in its marker's words (area 5, "Computed backfills"; chunk 5.6b2):
/// `jadpo migrate apply` reads each batch, works the values out in the
/// runtime and writes them back. Its marker is followed by three
/// statements: the count, the read and the write.
pub const IN_JADPO: &str = "in Jadpo";

/// The line that opens the last step's fill of a backfill worked out in
/// Jadpo (chunk 5.6b2): the rows servers still on the old build wrote
/// meanwhile, read and written back with the table locked against
/// writes, in the transaction that makes the column required.
pub const FILL_MARKER: &str = "-- fill ";

/// What a store's section of a migration does, step by step: statements
/// run together in one transaction, and backfills filled in batches,
/// each batch in its own transaction (chunk 5.6b).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Statements run as one batch, in one transaction.
    Once(String),
    Batches(Batches),
    /// Statements and fills worked out in Jadpo, run in order in one
    /// transaction: a backfill's last step when Jadpo works it out
    /// (chunk 5.6b2).
    Together(Vec<Part>),
}

/// One part of a [`Step::Together`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    /// Statements run as one batch.
    Sql(String),
    Fill(Fill),
}

/// The rows still without a value, worked out in Jadpo and written back
/// (chunk 5.6b2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fill {
    /// The field it fills, `Todo.priority`.
    pub field: String,
    /// A statement giving each row still without a value: its key, then
    /// the columns the backfill reads.
    pub read: String,
    /// A statement taking the keys as `$1` and their values as `$2`, two
    /// arrays in the same order, and filling the rows still without one.
    pub write: String,
}

/// A backfill filled in batches until a batch finds no row left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Batches {
    /// The field it fills, `Todo.priority`.
    pub field: String,
    /// How each batch is worked out, in the marker's words: `by UPDATE`,
    /// or [`IN_JADPO`].
    pub how: String,
    /// One statement giving how many rows are left to fill.
    pub count: String,
    /// One batch: a statement taking the key the last batch ended at as
    /// `$1` (a `uuid`, the nil UUID before the first). By `UPDATE`, it
    /// gives the key this batch ended at, `NULL` when no row is left past
    /// it, and how many rows it filled. In Jadpo, it gives the batch's
    /// rows still without a value, locked, each its key and then the
    /// columns the backfill reads, and [`Batches::write`] writes their
    /// values back.
    pub batch: String,
    /// In Jadpo: the statement writing a batch's values back, as
    /// [`Fill::write`] does.
    pub write: Option<String>,
}

/// How a migration creates an extension, as `jadpo migrate new` writes it.
const CREATE_EXTENSION: &str = "CREATE EXTENSION IF NOT EXISTS ";

/// How a migration builds an HNSW index, as `jadpo migrate new` writes it.
const USING_HNSW: &str = " USING hnsw ";

/// The pgvector a store needs once it has an HNSW index: 0.8.0, whose
/// iterative scans a `nearest` query turns on (area 21, chunk 5.5v1b).
pub const ITERATIVE_SCANS: (&str, [u32; 3]) = ("vector", [0, 8, 0]);

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
    /// step, the backfill's statements another, and so on. A fill worked
    /// out in Jadpo joins the statements around it in one step.
    pub fn steps(&self) -> Vec<Step> {
        let mut out = Vec::new();
        let mut once = String::new();
        let mut parts: Vec<Part> = Vec::new();
        let mut lines = self.sql.lines();
        let close = |once: &mut String, parts: &mut Vec<Part>, out: &mut Vec<Step>| {
            let sql = once.trim();
            if !sql.is_empty() {
                parts.push(Part::Sql(sql.to_string()));
            }
            once.clear();
            match parts.len() {
                0 => {}
                1 if matches!(parts[0], Part::Sql(_)) => {
                    if let Some(Part::Sql(sql)) = parts.pop() {
                        out.push(Step::Once(sql));
                    }
                }
                _ => out.push(Step::Together(std::mem::take(parts))),
            }
        };
        fn statement<'l>(lines: &mut impl Iterator<Item = &'l str>) -> String {
            let mut text = String::new();
            for line in lines.by_ref() {
                text.push_str(line);
                if let Some(done) = text.strip_suffix(';') {
                    return done.to_string();
                }
                text.push('\n');
            }
            text
        }
        while let Some(line) = lines.next() {
            if let Some(rest) = line.strip_prefix(FILL_MARKER) {
                let sql = once.trim();
                if !sql.is_empty() {
                    parts.push(Part::Sql(sql.to_string()));
                }
                once.clear();
                let field = rest.split(' ').next().unwrap_or_default();
                let read = statement(&mut lines);
                let write = statement(&mut lines);
                parts.push(Part::Fill(Fill {
                    field: field.to_string(),
                    read,
                    write,
                }));
                continue;
            }
            let Some(rest) = line.strip_prefix(BACKFILL_MARKER) else {
                once.push_str(line);
                once.push('\n');
                continue;
            };
            close(&mut once, &mut parts, &mut out);
            let (field, how) = rest.split_once(": ").unwrap_or((rest, ""));
            let how = how.split(',').next().unwrap_or_default().trim();
            let count = statement(&mut lines);
            let batch = statement(&mut lines);
            let write = (how == IN_JADPO).then(|| statement(&mut lines));
            out.push(Step::Batches(Batches {
                field: field.to_string(),
                how: how.to_string(),
                count,
                batch,
                write,
            }));
        }
        close(&mut once, &mut parts, &mut out);
        out
    }

    /// Whether it fills a backfill in batches.
    pub fn has_batches(&self) -> bool {
        self.sql
            .lines()
            .any(|line| line.starts_with(BACKFILL_MARKER))
    }
    /// The least version of each extension it needs, beyond having it:
    /// pgvector 0.8.0 for a migration that builds an HNSW index, which only
    /// a `nearest` query asks for, since its queries run with pgvector's
    /// iterative scans (area 21, chunk 5.5v1b).
    pub fn least_versions(&self) -> Vec<(String, [u32; 3])> {
        if self
            .sql
            .lines()
            .any(|line| line.starts_with("CREATE INDEX ") && line.contains(USING_HNSW))
        {
            let (extension, least) = ITERATIVE_SCANS;
            return vec![(extension.to_string(), least)];
        }
        Vec::new()
    }
}

/// An extension's version as PostgreSQL gives it (`0.8.7`, `0.7.0`), as
/// three numbers, a missing part 0; `None` when it doesn't start with a
/// number.
pub fn version(text: &str) -> Option<[u32; 3]> {
    let mut out = [0u32; 3];
    let mut parts = text.split('.');
    for (at, slot) in out.iter_mut().enumerate() {
        let Some(part) = parts.next() else {
            break;
        };
        let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() {
            return if at == 0 { None } else { Some(out) };
        }
        *slot = digits.parse().ok()?;
        if digits.len() < part.len() {
            break;
        }
    }
    Some(out)
}

/// A version as PostgreSQL writes it: `0.8.0`.
pub fn version_text(version: [u32; 3]) -> String {
    format!("{}.{}.{}", version[0], version[1], version[2])
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
        assert!(none.least_versions().is_empty());
    }

    #[test]
    fn an_hnsw_index_needs_pgvector_0_8() {
        let migration = Migration {
            name: "20261020T1500_closest_articles".into(),
            digest: "0".into(),
            sql: "CREATE INDEX \"help_article_body_meaning_idx\" ON \"help_article\" USING hnsw (\"body_meaning\" vector_cosine_ops);\n".into(),
        };
        assert_eq!(
            migration.least_versions(),
            [("vector".to_string(), [0, 8, 0])]
        );
        assert_eq!(version("0.8.7"), Some([0, 8, 7]));
        assert_eq!(version("0.7.0"), Some([0, 7, 0]));
        assert_eq!(version("0.10"), Some([0, 10, 0]));
        assert_eq!(version("1.0.0-dev"), Some([1, 0, 0]));
        assert_eq!(version("unknown"), None);
        assert!(version("0.7.4").unwrap() < [0, 8, 0]);
        assert!(version("0.10.0").unwrap() > [0, 8, 0]);
        assert_eq!(version_text([0, 8, 0]), "0.8.0");
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
                    write: None,
                }),
                Step::Once("ALTER TABLE \"todo\" ALTER COLUMN \"priority\" SET NOT NULL;".into()),
            ]
        );
    }

    #[test]
    fn a_backfill_in_jadpo_reads_and_writes_each_batch_and_fills_the_rest_in_its_last_step() {
        let migration = Migration {
            name: "20261101T0900_priority".into(),
            digest: "0".into(),
            sql: "ALTER TABLE \"todo\" ADD COLUMN \"priority\" text;\n-- backfill Todo.priority: in Jadpo, 1000 rows a batch\nSELECT count(*) FROM \"todo\";\nSELECT \"id\" FROM \"todo\";\nUPDATE \"todo\" SET x;\nLOCK TABLE \"todo\" IN SHARE ROW EXCLUSIVE MODE;\n-- fill Todo.priority in Jadpo: the rows written meanwhile\nSELECT \"id\" FROM \"todo\" WHERE y;\nUPDATE \"todo\" SET z;\nALTER TABLE \"todo\" ALTER COLUMN \"priority\" SET NOT NULL;".into(),
        };
        assert_eq!(
            migration.steps(),
            vec![
                Step::Once("ALTER TABLE \"todo\" ADD COLUMN \"priority\" text;".into()),
                Step::Batches(Batches {
                    field: "Todo.priority".into(),
                    how: IN_JADPO.into(),
                    count: "SELECT count(*) FROM \"todo\"".into(),
                    batch: "SELECT \"id\" FROM \"todo\"".into(),
                    write: Some("UPDATE \"todo\" SET x".into()),
                }),
                Step::Together(vec![
                    Part::Sql("LOCK TABLE \"todo\" IN SHARE ROW EXCLUSIVE MODE;".into()),
                    Part::Fill(Fill {
                        field: "Todo.priority".into(),
                        read: "SELECT \"id\" FROM \"todo\" WHERE y".into(),
                        write: "UPDATE \"todo\" SET z".into(),
                    }),
                    Part::Sql(
                        "ALTER TABLE \"todo\" ALTER COLUMN \"priority\" SET NOT NULL;".into()
                    ),
                ]),
            ]
        );
    }
}
