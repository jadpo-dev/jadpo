//! What a deployable's generated code hands its host (roadmap chunks 8.1o
//! and 8.2b; docs/compiler-runtime.md §2.6, "A native deployable" and "A
//! Workers deployable"): the compiled program, the build stamp and each
//! store's migrations, as constants. The native host's `main` and the
//! Workers host's handlers take the same [`Deployable`], so both start a
//! program the same way and stamp every event alike.

use jadpo_std::Unreadable;

use crate::compiled::Program;

/// What a generated deployable hands its host.
#[derive(Clone, Copy, Debug)]
pub struct Deployable {
    /// The compiled program on the calling thread: the generated crate's
    /// `program`, which decodes its plan once for the process.
    pub program: fn() -> Result<Program, Unreadable>,
    pub stamp: Stamp,
    /// Each database the project keeps migrations in, with its migrations
    /// in the order they apply, as the project held them when it was built.
    pub stores: &'static [Store],
}

/// The build stamp (area 18, "Production builds"; docs/compiler-runtime.md
/// §2.8): a constant in the deployable, and on every event it logs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    /// The build id, `b_` and twelve hex digits of a digest of the source
    /// the deployable reaches, the compiler, the target and the lock.
    pub build: &'static str,
    /// The commit it was built from, when the project is in git.
    pub commit: Option<&'static str>,
    /// Whether the tree held changes the commit doesn't, or had no commit.
    pub modified: bool,
    /// The Jadpo version that built it.
    pub compiler: &'static str,
    pub deployable: &'static str,
    /// `native` or `wasm`, as `target:` names it.
    pub target: &'static str,
}

/// A database the project keeps migrations in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Store {
    /// The store's connection.
    pub name: &'static str,
    pub migrations: &'static [Migration],
}

/// One migration of a store: its folder's name and the digest the store
/// records for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Migration {
    pub name: &'static str,
    pub digest: &'static str,
}
