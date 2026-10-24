use super::*;
use std::cell::{Cell, RefCell};

struct Deleting {
    calls: RefCell<Vec<String>>,
    now: Cell<u64>,
    stopped: Cell<bool>,
    cut: bool,
    shutdown: bool,
}
impl Deleting {
    fn new(cut: bool, shutdown: bool) -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            now: Cell::new(0),
            stopped: Cell::new(false),
            cut,
            shutdown,
        }
    }
}
impl Database for Deleting {
    fn begin_at(
        &self,
        _: crate::Isolation,
    ) -> LocalFuture<'_, Result<Box<dyn crate::Transaction>, StoreError>> {
        Box::pin(async { Err(StoreError::Unavailable) })
    }
    fn query_once<'a>(
        &'a self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<crate::Row>, StoreError>> {
        Box::pin(async move {
            self.calls.borrow_mut().push(sql.to_string());
            if sql == EXPIRED_KEYS {
                assert_eq!(params[1], SqlValue::Int8(DELETE_BATCH));
                if self.cut {
                    self.now.set(10_000);
                }
                if self.shutdown {
                    self.stopped.set(true);
                }
                return Ok(vec![vec![SqlValue::Int4(1)]; DELETE_BATCH as usize]);
            }
            if sql == SETTLED {
                self.now.set(10_000);
                return Ok(vec![vec![
                    SqlValue::Uuid([1; 16]),
                    SqlValue::Uuid([2; 16]),
                    SqlValue::Bool(false),
                ]]);
            }
            if sql == housekeeping::RELEASE && (self.cut || self.shutdown) {
                assert_eq!(params[1], SqlValue::Null(SqlType::Timestamp));
            }
            if sql == housekeeping::CLAIM || sql == housekeeping::RELEASE {
                return Ok(vec![vec![SqlValue::Int4(0)]]);
            }
            Ok(vec![])
        })
    }
}
impl Host for Deleting {
    fn database(&self, _: &str) -> Option<&dyn Database> {
        Some(self)
    }
    fn random(&self, bytes: &mut [u8]) -> bool {
        bytes.fill(1);
        true
    }
    fn monotonic_ms(&self) -> Option<u64> {
        Some(self.now.get())
    }
    fn stopping(&self) -> LocalFuture<'_, ()> {
        Box::pin(std::future::poll_fn(|_| {
            if self.stopped.get() {
                std::task::Poll::Ready(())
            } else {
                std::task::Poll::Pending
            }
        }))
    }
}

#[test]
fn every_delete_batch_checks_time_and_shutdown_before_admission() {
    for (time, shutdown) in [(true, false), (false, true)] {
        let host = Deleting::new(time, shutdown);
        let budget = Budget::of(&host, 10_000);
        let (count, complete) = crate::block_on(delete(&host, &budget, "main", EXPIRED_KEYS, 0))
            .unwrap()
            .unwrap();
        assert_eq!(count, 1000);
        assert!(!complete);
        assert_eq!(host.calls.borrow().len(), 1);
    }
    let host = Deleting::new(false, false);
    let budget = Budget::of(&host, 10_000);
    let (count, complete) = crate::block_on(delete(&host, &budget, "main", EXPIRED_KEYS, 0))
        .unwrap()
        .unwrap();
    assert_eq!(count, 10_000);
    assert!(!complete);
    assert_eq!(host.calls.borrow().len(), 10);
}

#[test]
fn expiry_between_event_commit_and_remote_handled_cleanup_keeps_the_safe_orphan() {
    let host = Deleting::new(false, false);
    let budget = Budget::of(&host, 10_000);
    let mut deleted = Deleted::default();
    let complete = crate::block_on(settle(
        &host,
        &budget,
        &["main".into(), "other".into()],
        "main",
        0,
        &mut deleted,
    ))
    .unwrap()
    .unwrap();
    assert!(!complete);
    assert_eq!(deleted.events, 1);
    assert_eq!(deleted.handled, 0);
    assert_eq!(*host.calls.borrow(), vec![SETTLED]);
}

struct OnlyState {
    places: crate::Places,
    plan: SchedulesPlan,
}
impl Executor for OnlyState {
    fn places(&self) -> &crate::Places {
        &self.places
    }
    fn plan(&self) -> crate::Plan<'_> {
        crate::Plan {
            schedules: Some(&self.plan),
            runs: jadpo_std::Runs {
                schedules: false,
                subscribers: false,
            },
            ..crate::Plan::NONE
        }
    }
    fn run<'a, 'p: 'a>(
        &'a self,
        _: &'a mut Ctx<'p>,
        _: u32,
        _: Vec<jadpo_value::Value>,
    ) -> LocalFuture<'a, Outcome> {
        Box::pin(async { Outcome::Value(jadpo_value::Value::Unit) })
    }
}
#[test]
fn cleanup_union_includes_ai_only_and_schedule_only_stores_without_prompt_triggers() {
    let executor = OnlyState {
        places: crate::Places::default(),
        plan: SchedulesPlan {
            cleanup_schedules: vec!["runs".into()],
            cleanup_batches: vec!["batches".into()],
            ai_usage: Some("usage".into()),
            ..SchedulesPlan::default()
        },
    };
    let host = Deleting::new(false, false);
    let ticked = crate::block_on(tick(&executor, &host, 0)).unwrap();
    assert!(ticked.problems.is_empty(), "{ticked:?}");
    let calls = host.calls.borrow();
    assert_eq!(
        calls
            .iter()
            .filter(|sql| sql.as_str() == housekeeping::CLAIM)
            .count(),
        3
    );
    assert!(calls.iter().any(|sql| sql == crate::batches::ENDED));
    assert!(
        calls
            .iter()
            .any(|sql| sql == crate::batches::UNKEPT_CANCELS)
    );
    assert!(calls.iter().any(|sql| sql == OLD_RUNS));
    assert!(calls.iter().any(|sql| sql == OLD_RUN_ACTIONS));
    assert!(calls.iter().any(|sql| sql == crate::ai_budget::OLD_DAYS));
    assert!(!calls.iter().any(|sql| sql == EXPIRED_KEYS));
}

#[test]
fn an_interrupted_pass_releases_without_completing_or_admitting_another_delete() {
    let executor = OnlyState {
        places: crate::Places::default(),
        plan: SchedulesPlan {
            stores: vec!["main".into()],
            ..SchedulesPlan::default()
        },
    };
    for (time, shutdown) in [(true, false), (false, true)] {
        let host = Deleting::new(time, shutdown);
        let budget = Budget::of(&host, 10_000);
        let mut ticked = Ticked::default();
        crate::block_on(housekeeping::run(
            &host,
            &budget,
            &executor.plan,
            0,
            &mut ticked,
        ))
        .unwrap();
        assert_eq!(ticked.deleted.keys, 1000);
        assert_eq!(
            *host.calls.borrow(),
            vec![housekeeping::CLAIM, EXPIRED_KEYS, housekeeping::RELEASE]
        );
    }
}
