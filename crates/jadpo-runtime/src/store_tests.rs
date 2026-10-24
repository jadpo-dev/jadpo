//! Standalone-read routing and the existing boundary's outcome priority.
#[cfg(test)]
mod controls {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use super::super::*;
    use crate::{Database, Env, Host, Places, Row, SqlValue, StoreError, Transaction, block_on};
    use jadpo_std::read::{
        Cardinality, Column, Field, IncludeRead, IncludeShape, MissingPlan, Recheck, Record, Select,
    };
    use jadpo_std::types::FailureId;

    #[derive(Default)]
    struct State {
        lines: RefCell<Vec<String>>,
        rows: RefCell<Vec<Row>>,
        query_error: RefCell<Option<StoreError>>,
        end_error: RefCell<Option<StoreError>>,
        changes: Cell<u32>,
        change_at_end: Cell<bool>,
    }
    struct Store(Rc<State>);
    struct Tx(Rc<State>);
    impl Tx {
        fn rows(&self, kind: &str, sql: &str) -> Result<Vec<Row>, StoreError> {
            self.0.lines.borrow_mut().push(format!("{kind} {sql}"));
            if !self.0.change_at_end.get() && self.0.changes.get() > 0 {
                self.0.changes.set(self.0.changes.get() - 1);
                return Err(StoreError::ConcurrentChange);
            }
            match self.0.query_error.borrow().clone() {
                Some(error) => Err(error),
                None => Ok(self.0.rows.borrow().clone()),
            }
        }
    }
    impl Transaction for Tx {
        fn query<'a>(
            &'a mut self,
            sql: &'a str,
            _: &'a [SqlValue],
        ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
            Box::pin(async move { self.rows("query", sql) })
        }
        fn query_last_read<'a>(
            &'a mut self,
            sql: &'a str,
            _: &'a [SqlValue],
        ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
            Box::pin(async move { self.rows("last", sql) })
        }
        fn execute<'a>(
            &'a mut self,
            sql: &'a str,
            _: &'a [SqlValue],
        ) -> LocalFuture<'a, Result<u64, StoreError>> {
            self.0.lines.borrow_mut().push(format!("execute {sql}"));
            Box::pin(async { Ok(1) })
        }
        fn batch<'a>(&'a mut self, sql: &'a str) -> LocalFuture<'a, Result<(), StoreError>> {
            self.0.lines.borrow_mut().push(format!("batch {sql}"));
            Box::pin(async { Ok(()) })
        }
        fn commit(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
            Box::pin(async move {
                self.0.lines.borrow_mut().push("commit".into());
                if self.0.change_at_end.get() && self.0.changes.get() > 0 {
                    self.0.changes.set(self.0.changes.get() - 1);
                    return Err(StoreError::ConcurrentChange);
                }
                self.0.end_error.borrow().clone().map_or(Ok(()), Err)
            })
        }
        fn rollback(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
            self.0.lines.borrow_mut().push("rollback".into());
            Box::pin(async move { self.0.end_error.borrow().clone().map_or(Ok(()), Err) })
        }
    }
    impl Database for Store {
        fn begin_at(
            &self,
            isolation: Isolation,
        ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
            self.0
                .lines
                .borrow_mut()
                .push(format!("begin {}", isolation.word()));
            let state = Rc::clone(&self.0);
            Box::pin(async move { Ok(Box::new(Tx(state)) as Box<dyn Transaction>) })
        }
    }
    impl Host for Store {
        fn database(&self, _: &str) -> Option<&dyn Database> {
            Some(self)
        }
        fn random(&self, _: &mut [u8]) -> bool {
            false
        }
    }
    fn plan() -> ReadPlan {
        ReadPlan {
            store: "main".into(),
            settings: vec![],
            cardinality: Cardinality::One,
            select: Select {
                sql: "MAIN".into(),
                unpoliced: None,
                permitted: None,
                params: vec![],
                columns: vec![Column::Text],
                held: vec![],
                record: Record {
                    ty: "Item".into(),
                    fields: vec![("name".into(), Field::Column(0))],
                },
                includes: vec![],
                versions: None,
            },
            page: None,
            missing: Some(MissingPlan {
                failure: FailureId(0),
                fields: vec![],
            }),
            recheck: None,
            layer: false,
        }
    }
    fn failures() -> [FailurePlan; 1] {
        [FailurePlan {
            key: "type Missing".into(),
            name: "Missing".into(),
            code: "missing".into(),
            kind: None,
            status: None,
            message: None,
            fields: vec![],
        }]
    }
    fn run_with(plan: &ReadPlan, state: Rc<State>, setup: impl FnOnce(&mut Ctx<'_>)) -> Stored {
        let places = Places::new();
        let host = Store(state);
        let mut ctx = Ctx::new(&places, &host, Env::default());
        setup(&mut ctx);
        block_on(store(
            &mut ctx,
            StoreOp::Read(plan),
            &failures(),
            JsonCodecs::NONE,
            &[],
            0,
        ))
        .unwrap()
    }
    fn value_state() -> Rc<State> {
        let state = Rc::new(State::default());
        *state.rows.borrow_mut() = vec![vec![SqlValue::Text("kept".into())]];
        state
    }

    #[test]
    fn eligible_standalone_read_uses_terminal_capability_at_repeatable_read() {
        let state = value_state();
        assert!(matches!(
            run_with(&plan(), Rc::clone(&state), |_| {}),
            Stored::Value(_)
        ));
        assert_eq!(
            *state.lines.borrow(),
            ["begin repeatable_read", "last MAIN", "commit"]
        );
    }

    #[test]
    fn failed_body_wins_over_terminal_error_and_policy_filters_before_decoding() {
        for hidden in [false, true] {
            let state = Rc::new(State::default());
            *state.end_error.borrow_mut() = Some(StoreError::Unavailable);
            let mut plan = plan();
            if hidden {
                plan.select.permitted = Some(1);
                plan.select.columns.push(Column::Bool);
                // Invalid stored text must be hidden before interpreting it.
                *state.rows.borrow_mut() = vec![vec![SqlValue::Int8(7), SqlValue::Bool(false)]];
            }
            let Stored::Failed(failure) = run_with(&plan, Rc::clone(&state), |_| {}) else {
                panic!("missing failure");
            };
            assert_eq!(failure.code, "missing");
            assert_eq!(
                failure
                    .internal
                    .iter()
                    .any(|(key, _)| key == "concealed_by"),
                hidden
            );
            assert_eq!(
                *state.lines.borrow(),
                ["begin repeatable_read", "last MAIN", "rollback"]
            );
        }
        let state = value_state();
        *state.rows.borrow_mut() = vec![vec![SqlValue::Int8(7)]];
        *state.end_error.borrow_mut() = Some(StoreError::Unavailable);
        assert!(matches!(
            run_with(&plan(), Rc::clone(&state), |_| {}),
            Stored::Fault(_)
        ));
        assert_eq!(
            *state.lines.borrow(),
            ["begin repeatable_read", "last MAIN", "rollback"]
        );
    }

    #[test]
    fn query_and_terminal_concurrent_changes_keep_the_three_attempt_budget() {
        for at_end in [false, true] {
            for changes in [2, 3] {
                let state = value_state();
                state.changes.set(changes);
                state.change_at_end.set(at_end);
                let outcome = run_with(&plan(), Rc::clone(&state), |_| {});
                if changes == 2 {
                    assert!(matches!(outcome, Stored::Value(_)));
                } else {
                    let Stored::Failed(failure) = outcome else {
                        panic!("unavailable");
                    };
                    assert_eq!(failure.code, "unavailable");
                }
                assert_eq!(
                    state
                        .lines
                        .borrow()
                        .iter()
                        .filter(|line| *line == "last MAIN")
                        .count(),
                    3
                );
            }
        }
        for error in [
            StoreError::Unavailable,
            StoreError::Statement("refused".into()),
        ] {
            let state = value_state();
            *state.query_error.borrow_mut() = Some(error);
            run_with(&plan(), Rc::clone(&state), |_| {});
            assert_eq!(
                state
                    .lines
                    .borrow()
                    .iter()
                    .filter(|line| line.starts_with("begin"))
                    .count(),
                1
            );
        }
    }

    #[test]
    fn settings_includes_and_rechecks_keep_the_ordinary_path_and_extra_sql() {
        for kind in 0..3 {
            let state = value_state();
            let mut plan = plan();
            match kind {
                0 => plan.settings.push("SET LOCAL fixture".into()),
                1 => {
                    let mut child = plan.select.clone();
                    child.sql = "CHILD".into();
                    plan.select.includes.push(IncludeRead {
                        shape: IncludeShape::List,
                        key: 0,
                        select: child,
                    });
                }
                _ => {
                    plan.recheck = Some(Recheck {
                        checks: vec![],
                        order: None,
                        limit: None,
                    })
                }
            }
            assert!(matches!(
                run_with(&plan, Rc::clone(&state), |_| {}),
                Stored::Value(_)
            ));
            let lines = state.lines.borrow();
            assert!(lines.iter().any(|line| line == "query MAIN"), "{lines:?}");
            assert!(!lines.iter().any(|line| line.starts_with("last")));
            if kind == 0 {
                assert!(lines.iter().any(|line| line == "batch SET LOCAL fixture"));
            }
            if kind == 1 {
                assert!(lines.iter().any(|line| line == "query CHILD"));
            }
        }
    }

    #[test]
    fn held_and_handled_contexts_leave_later_sql_in_the_same_transaction() {
        let state = value_state();
        let places = Places::new();
        let host = Store(Rc::clone(&state));
        let mut ctx = Ctx::new(&places, &host, Env::default());
        ctx.hold();
        assert!(matches!(
            block_on(store(
                &mut ctx,
                StoreOp::Read(&plan()),
                &failures(),
                JsonCodecs::NONE,
                &[],
                0
            ))
            .unwrap(),
            Stored::Value(_)
        ));
        let held = ctx.take_held().unwrap();
        let mut transaction = held.transaction.unwrap();
        block_on(transaction.execute("AFTER HOLD", &[]))
            .unwrap()
            .unwrap();
        block_on(transaction.commit()).unwrap().unwrap();
        assert_eq!(
            *state.lines.borrow(),
            [
                "begin repeatable_read",
                "query MAIN",
                "execute AFTER HOLD",
                "commit"
            ]
        );
        let state = value_state();
        run_with(&plan(), Rc::clone(&state), |ctx| {
            ctx.background_mut().set_handled(Some(crate::ctx::Handled {
                store: "main".into(),
                id: [7; 16],
            }))
        });
        let lines = state.lines.borrow();
        assert_eq!(lines[1], "query MAIN");
        assert!(
            lines[2].starts_with("execute INSERT INTO \"jadpo_handled_deliveries\""),
            "{lines:?}"
        );
        assert_eq!(lines[3], "commit");
    }

    #[test]
    fn action_background_and_resumable_contexts_use_the_ordinary_path() {
        for kind in 0..3 {
            let state = value_state();
            run_with(&plan(), Rc::clone(&state), |ctx| match kind {
                0 => ctx.open_boundary(Isolation::RepeatableRead),
                1 => ctx.become_system(crate::System::default()),
                _ => ctx.set_resumable(Some(1)),
            });
            assert_eq!(state.lines.borrow()[1], "query MAIN");
        }
    }

    #[test]
    fn extension_compound_and_provider_coded_reads_keep_the_ordinary_path() {
        for column in [
            Column::Vector,
            Column::Geography("Point".into()),
            Column::Compound {
                ty: "Money".into(),
                may_be_none: false,
            },
            Column::Part,
        ] {
            let state = value_state();
            let mut plan = plan();
            plan.select.columns[0] = column;
            run_with(&plan, Rc::clone(&state), |_| {});
            assert_eq!(state.lines.borrow()[1], "query MAIN");
        }
        for ty in [crate::SqlType::Vector, crate::SqlType::Geography] {
            let state = value_state();
            let mut plan = plan();
            plan.select.params.push(jadpo_std::read::Param {
                source: jadpo_std::read::Source::Stored(SqlValue::Null(ty)),
                ty,
            });
            run_with(&plan, Rc::clone(&state), |_| {});
            assert_eq!(state.lines.borrow()[1], "query MAIN");
        }
        #[derive(Debug)]
        struct Coder;
        impl jadpo_value::json::Coder for Coder {
            fn decode(&self, _: &str, _: Value) -> Option<Value> {
                panic!("no provider code in this read")
            }
            fn encode(&self, _: &str, _: Value) -> Option<Value> {
                panic!("no provider code in this read")
            }
            fn unwritten(&self, _: &str) {
                panic!("no provider code in this read")
            }
        }
        let state = value_state();
        let places = Places::new();
        let host = Store(Rc::clone(&state));
        let mut ctx = Ctx::new(&places, &host, Env::default());
        let mut json = JsonCodecs::NONE;
        json.codecs.coder = Some(&Coder);
        block_on(store(
            &mut ctx,
            StoreOp::Read(&plan()),
            &failures(),
            json,
            &[],
            0,
        ))
        .unwrap();
        assert_eq!(state.lines.borrow()[1], "query MAIN");
    }

    #[test]
    fn invalid_cursor_runs_no_sql_and_unavailable_end_does_not_retry() {
        let state = value_state();
        let mut plan = plan();
        plan.cardinality = Cardinality::Page;
        plan.page = Some(jadpo_std::read::PageRead {
            size: jadpo_std::read::Size::Fixed(1),
            after: jadpo_std::read::Source::Arg {
                index: 0,
                path: vec![],
            },
            cursor: vec![],
            tag: 1,
        });
        let places = Places::new();
        let host = Store(Rc::clone(&state));
        let mut ctx = Ctx::new(&places, &host, Env::default());
        let result = block_on(store(
            &mut ctx,
            StoreOp::Read(&plan),
            &failures(),
            JsonCodecs::NONE,
            &[Value::text("wrong")],
            0,
        ))
        .unwrap();
        assert!(matches!(result, Stored::Failed(_)));
        assert!(state.lines.borrow().is_empty());
        let state = value_state();
        *state.end_error.borrow_mut() = Some(StoreError::Unavailable);
        let Stored::Failed(failure) = run_with(&self::plan(), Rc::clone(&state), |_| {}) else {
            panic!("unavailable end");
        };
        assert_eq!(failure.code, "unavailable");
        assert_eq!(
            *state.lines.borrow(),
            ["begin repeatable_read", "last MAIN", "commit"]
        );
    }

    #[test]
    fn recorded_hidden_read_keeps_a_truthful_physical_rollback() {
        struct Recording(crate::record::Recorded<Store>);
        impl Host for Recording {
            fn database(&self, _: &str) -> Option<&dyn Database> {
                Some(&self.0)
            }
            fn random(&self, _: &mut [u8]) -> bool {
                false
            }
        }
        let state = value_state();
        *state.rows.borrow_mut() = vec![vec![SqlValue::Int8(7), SqlValue::Bool(false)]];
        let record = Rc::new(crate::record::Record::new([7; 32]));
        let host = Recording(crate::record::Recorded::new(
            Store(Rc::clone(&state)),
            Rc::clone(&record),
        ));
        let places = Places::new();
        let mut ctx = Ctx::new(&places, &host, Env::default());
        let mut plan = plan();
        plan.select.permitted = Some(1);
        plan.select.columns.push(Column::Bool);
        assert!(matches!(
            block_on(store(
                &mut ctx,
                StoreOp::Read(&plan),
                &failures(),
                JsonCodecs::NONE,
                &[],
                0
            ))
            .unwrap(),
            Stored::Failed(_)
        ));
        assert_eq!(
            *state.lines.borrow(),
            ["begin repeatable_read", "query MAIN", "rollback"]
        );
        assert!(record.entries().iter().any(|entry| matches!(
            entry,
            crate::record::Entry::Ended {
                committed: false,
                ran: crate::record::Ran::Done
            }
        )));
    }
}
