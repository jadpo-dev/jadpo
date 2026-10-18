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
}
