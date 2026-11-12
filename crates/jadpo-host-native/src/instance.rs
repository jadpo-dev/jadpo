//! The process's name as an instance of the app (area 12, "Watching
//! schedules"): its machine's host name and its process id, such as
//! `api-2:4182`, kept with each schedule lease it takes, so `jadpo
//! schedules` and `jadpo why` say where the run holding one is going.

/// Names this process, as a deployable or a `jadpo` command does as it
/// starts; a name given before stands.
pub fn name_instance() {
    jadpo_runtime::instance::name(|| {
        Some(instance_name(host_name().as_deref(), std::process::id()))
    });
}

/// The name of process `pid` on the machine called `host`: `host:pid`, or
/// `process <pid>` where the machine's name can't be read.
fn instance_name(host: Option<&str>, pid: u32) -> String {
    match host.map(str::trim).filter(|host| !host.is_empty()) {
        Some(host) => format!("{host}:{pid}"),
        None => format!("process {pid}"),
    }
}

/// The machine's host name, as `uname` gives it.
#[cfg(unix)]
fn host_name() -> Option<String> {
    let name = rustix::system::uname();
    name.nodename().to_str().ok().map(str::to_string)
}

/// The machine's name, as Windows keeps it.
#[cfg(not(unix))]
fn host_name() -> Option<String> {
    std::env::var("COMPUTERNAME").ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_instance_is_named_by_its_machine_and_process() {
        assert_eq!(instance_name(Some("api-2"), 4182), "api-2:4182");
        assert_eq!(instance_name(Some(" "), 4182), "process 4182");
        assert_eq!(instance_name(None, 7), "process 7");
        assert!(host_name().is_some_and(|host| !host.is_empty()));
    }
}
