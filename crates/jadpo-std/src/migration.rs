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
}
