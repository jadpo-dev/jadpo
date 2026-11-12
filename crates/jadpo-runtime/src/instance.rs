//! Names for instances (area 12, "Watching schedules"): which copy of the
//! app holds a schedule's lease, so an operator reading `jadpo schedules`
//! or `jadpo why` sees where the run holding it is going, or went.
//!
//! A process is named once, by its host, as it starts: a native one by its
//! machine's host name and process id (`api-2:4182`), a Worker's isolate
//! by a random name of its own (`worker-3fa9c1d2`). A host that names
//! nothing, as a test's, leaves the lease's holder unknown.

use std::sync::OnceLock;

static NAME: OnceLock<String> = OnceLock::new();

/// Names this process's instance with the name `name` makes, unless it
/// is named already: the first name stands for the process's life. A
/// `name` that makes none leaves it unnamed, for a later call to name.
pub fn name(name: impl FnOnce() -> Option<String>) {
    if NAME.get().is_none()
        && let Some(made) = name()
    {
        let _ = NAME.set(made);
    }
}

/// This process's instance's name, once its host has named it.
pub fn named() -> Option<&'static str> {
    NAME.get().map(String::as_str)
}
