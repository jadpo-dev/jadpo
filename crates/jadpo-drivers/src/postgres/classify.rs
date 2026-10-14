//! PostgreSQL's errors in the runtime's fixed set (docs/compiler-runtime.md
//! §2.6), decided in one place by SQLSTATE, so every target classifies the
//! same failure the same way.

use jadpo_runtime::StoreError;

/// What went wrong talking to PostgreSQL, as far as classifying it needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause<'a> {
    /// The server refused, with its SQLSTATE, the constraint it names (if
    /// any) and its message.
    Server {
        code: &'a str,
        constraint: Option<&'a str>,
        message: &'a str,
    },
    /// The connection closed, its I/O failed, or it could not be opened.
    Lost,
    /// Anything else the client reports.
    Other(&'a str),
}

/// The runtime's reading of `cause`.
pub fn classify(cause: Cause<'_>) -> StoreError {
    match cause {
        Cause::Server {
            code,
            constraint,
            message,
        } => match code {
            // unique_violation, foreign_key_violation, exclusion_violation
            "23505" | "23503" | "23P01" => StoreError::Conflict {
                constraint: constraint.unwrap_or_default().to_string(),
            },
            // serialization_failure, deadlock_detected
            "40001" | "40P01" => StoreError::ConcurrentChange,
            // admin_shutdown, crash_shutdown, cannot_connect_now,
            // too_many_connections, and every connection exception
            "57P01" | "57P02" | "57P03" | "53300" => StoreError::Unavailable,
            _ if code.starts_with("08") => StoreError::Unavailable,
            _ => StoreError::Fault(format!("PostgreSQL refused ({code}): {message}")),
        },
        Cause::Lost => StoreError::Unavailable,
        Cause::Other(message) => StoreError::Fault(format!("PostgreSQL client: {message}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(code: &str, constraint: Option<&str>) -> StoreError {
        classify(Cause::Server {
            code,
            constraint,
            message: "the message",
        })
    }

    #[test]
    fn constraint_violations_are_conflicts_named_by_their_constraint() {
        for code in ["23505", "23503", "23P01"] {
            assert_eq!(
                server(code, Some("tasks_title_key")),
                StoreError::Conflict {
                    constraint: "tasks_title_key".into()
                }
            );
        }
        assert_eq!(
            server("23505", None),
            StoreError::Conflict {
                constraint: String::new()
            }
        );
    }

    #[test]
    fn serialisation_failures_and_deadlocks_are_concurrent_changes() {
        assert_eq!(server("40001", None), StoreError::ConcurrentChange);
        assert_eq!(server("40P01", None), StoreError::ConcurrentChange);
    }

    #[test]
    fn a_server_that_cant_serve_now_is_unavailable() {
        for code in [
            "08000", "08003", "08006", "08001", "08P01", "57P01", "57P02", "57P03", "53300",
        ] {
            assert_eq!(server(code, None), StoreError::Unavailable, "{code}");
        }
        assert_eq!(classify(Cause::Lost), StoreError::Unavailable);
    }

    #[test]
    fn anything_else_is_a_fault_with_its_sqlstate_and_message() {
        assert_eq!(
            server("42P01", None),
            StoreError::Fault("PostgreSQL refused (42P01): the message".into())
        );
        // Other integrity errors are not conflicts: a not-null or check
        // violation is a bug in the plan, not a race.
        assert!(matches!(server("23502", Some("c")), StoreError::Fault(_)));
        assert!(matches!(server("23514", Some("c")), StoreError::Fault(_)));
        assert!(matches!(server("40002", None), StoreError::Fault(_)));
        assert_eq!(
            classify(Cause::Other("invalid connection string")),
            StoreError::Fault("PostgreSQL client: invalid connection string".into())
        );
    }
}
