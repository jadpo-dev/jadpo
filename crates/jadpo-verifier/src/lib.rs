//! The registry's verifier (area 11, "The verifier"; chunk R6): what
//! `jadpo package verify --serve` runs.
//!
//! A package's tests and examples are code from anyone, so everything here
//! is arranged around where that code runs and what comes back from it:
//!
//! - [`request`] holds what a request may name: a repository reached over
//!   HTTPS at a public host, a commit, a tag, a digest.
//! - [`git`] fetches one commit, with network to that host's checked
//!   address and nothing else, and reads it without checking anything out.
//! - [`files`] decides which of a commit's files a release may hold, before
//!   any of them is written anywhere.
//! - [`sandbox`] runs a program with no network, no host files, no
//!   variables of this process and limits on what it can use.
//! - [`verification`] runs `jadpo package verify` twice in that sandbox:
//!   once running none of the package's code, for everything worked out
//!   from its source, and once running it, for what running it shows. A
//!   process running the package's code can't choose its own facts.
//! - [`service`] answers the registry, one verification at a time.

pub mod files;
pub mod git;
pub mod request;
pub mod sandbox;
pub mod service;
pub mod verification;
