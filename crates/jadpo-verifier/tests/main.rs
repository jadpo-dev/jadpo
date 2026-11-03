//! The integration tests of `jadpo-verifier`, one binary for the crate:
//! each file beside this one is a module. The sandbox's are run against the
//! sandbox this machine has: bubblewrap on Linux, `sandbox-exec` on a Mac.
//! `JADPO_REQUIRE_SANDBOX=1`, which CI sets, makes a machine with neither a
//! failure rather than a skipped test, so isolation can't silently go
//! untested.

mod files;
mod requests;
mod sandbox;
mod service;
mod support;
mod verification;
