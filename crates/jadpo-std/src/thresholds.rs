//! The per-request monitoring thresholds (area 16, "Monitoring
//! thresholds"; chunk B13): plain data the plan carries into every
//! deployable, so both executors judge a request by the same limits once
//! the `telemetry:` section sends what crosses one. Thresholds judged
//! across requests are the monitoring tool's, written to
//! `build/monitoring/`, and never reach the runtime.

use serde::{Deserialize, Serialize};

/// One threshold with a limit judged per request.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ThresholdPlan {
    /// As an acceptance names it: `hello.greeting`.
    pub name: String,
    /// The identity key of what it watches: `route POST /hello`, `action
    /// greet`.
    pub watches: String,
    /// The longest one request or run may take.
    pub duration_ms: Option<u64>,
    /// The most queries one may run.
    pub queries: Option<u64>,
}

impl ThresholdPlan {
    /// Whether a request or run that took `duration_ms` and ran `queries`
    /// crossed it.
    pub fn crossed(&self, duration_ms: u64, queries: u64) -> bool {
        self.duration_ms.is_some_and(|limit| duration_ms > limit)
            || self.queries.is_some_and(|limit| queries > limit)
    }
}

/// The names of the thresholds on `watches` that a request which took
/// `duration_ms` and ran `queries` crossed, in plan order.
pub fn crossed<'a>(
    plans: &'a [ThresholdPlan],
    watches: &'a str,
    duration_ms: u64,
    queries: u64,
) -> impl Iterator<Item = &'a str> + 'a {
    plans
        .iter()
        .filter(move |plan| plan.watches == watches && plan.crossed(duration_ms, queries))
        .map(|plan| plan.name.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_crosses_a_limit_only_past_it() {
        let plans = [
            ThresholdPlan {
                name: "hello.greeting_route".into(),
                watches: "route POST /hello".into(),
                duration_ms: Some(800),
                queries: Some(2),
            },
            ThresholdPlan {
                name: "hello.greeting".into(),
                watches: "action greet".into(),
                duration_ms: Some(50),
                queries: None,
            },
        ];
        let route = "route POST /hello";
        assert_eq!(crossed(&plans, route, 800, 2).count(), 0);
        assert_eq!(
            crossed(&plans, route, 801, 0).collect::<Vec<_>>(),
            ["hello.greeting_route"]
        );
        assert_eq!(
            crossed(&plans, route, 10, 3).collect::<Vec<_>>(),
            ["hello.greeting_route"]
        );
        assert_eq!(
            crossed(&plans, "action greet", 51, 100).collect::<Vec<_>>(),
            ["hello.greeting"]
        );
        assert_eq!(crossed(&plans, "action other", 10_000, 100).count(), 0);
    }
}
