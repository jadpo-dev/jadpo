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
