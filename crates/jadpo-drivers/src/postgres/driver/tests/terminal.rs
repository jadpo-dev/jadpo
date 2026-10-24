//! Protocol controls: replies are withheld until the entire terminal read
//! has arrived. A sequential COMMIT cannot pass these controls.
#[cfg(test)]
mod controls {
    use super::super::*;

    fn packet(kind: u8, body: &[u8], out: &mut Vec<u8>) {
        out.push(kind);
        out.extend_from_slice(&(body.len() as u32 + 4).to_be_bytes());
        out.extend_from_slice(body);
    }

    fn command(tag: &str, status: u8) -> Vec<u8> {
        let mut out = Vec::new();
        let mut body = tag.as_bytes().to_vec();
        body.push(0);
        packet(b'C', &body, &mut out);
        packet(b'Z', &[status], &mut out);
        out
    }

    fn rows(value: i64) -> Vec<u8> {
        let mut out = Vec::new();
        packet(b'1', &[], &mut out);
        packet(b'2', &[], &mut out);
        let mut description = 1i16.to_be_bytes().to_vec();
        description.extend_from_slice(b"n\0");
        description.extend_from_slice(&0i32.to_be_bytes());
        description.extend_from_slice(&0i16.to_be_bytes());
        description.extend_from_slice(&20u32.to_be_bytes());
        description.extend_from_slice(&8i16.to_be_bytes());
        description.extend_from_slice(&(-1i32).to_be_bytes());
        description.extend_from_slice(&1i16.to_be_bytes());
        packet(b'T', &description, &mut out);
        let mut row = 1i16.to_be_bytes().to_vec();
        row.extend_from_slice(&8i32.to_be_bytes());
        row.extend_from_slice(&value.to_be_bytes());
        packet(b'D', &row, &mut out);
        out.extend(command("SELECT 1", b'T'));
        out
    }

    fn error(code: &str, status: u8) -> Vec<u8> {
        let mut out = Vec::new();
        let fields = format!("SERROR\0VERROR\0C{code}\0Mscripted refusal\0\0");
        packet(b'E', fields.as_bytes(), &mut out);
        packet(b'Z', &[status], &mut out);
        out
    }

    /// BEGIN, the complete unnamed extended query, then COMMIT, without a
    /// response in between. This asserts protocol ordering, not elapsed speed.
    async fn queued(server: &mut DuplexStream) {
        let (kind, body) = message(server).await.expect("BEGIN");
        assert_eq!(kind, b'Q');
        assert_eq!(body, b"BEGIN ISOLATION LEVEL REPEATABLE READ\0");
        let mut kinds = Vec::new();
        loop {
            let (kind, body) = message(server).await.expect("query group");
            kinds.push(kind);
            if kind == b'P' {
                assert_eq!(body[0], 0, "unnamed statement");
                assert_eq!(&body[1..], b"SELECT $1::int8\0\0\x01\0\0\0\x14");
            }
            if kind == b'B' {
                assert_eq!(&body[..2], &[0, 0], "unnamed portal and statement");
            }
            if kind == b'S' {
                break;
            }
        }
        assert_eq!(kinds, b"PBDES");
        let (kind, body) = message(server).await.expect("COMMIT before replies");
        assert_eq!(kind, b'Q');
        assert_eq!(body, b"COMMIT\0");
    }

    fn played<T>(
        serve: impl AsyncFnOnce(DuplexStream) + 'static,
        work: impl AsyncFnOnce(&Postgres) -> T,
    ) -> T {
        let host = Rc::new(Played::default());
        let store = Postgres::new(
            "postgres://app@localhost/app",
            Rc::clone(&host) as Rc<dyn Transport>,
        )
        .unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&runtime, async {
            let mut server = tokio::task::spawn_local(async move {
                let mut socket = loop {
                    let next = host.servers.borrow_mut().pop();
                    if let Some(next) = next {
                        break next;
                    }
                    tokio::task::yield_now().await;
                };
                started(&mut socket).await.expect("startup");
                serve(socket).await;
            });
            let result =
                tokio::time::timeout(std::time::Duration::from_secs(2), work(&store)).await;
            if result.is_err() {
                server.abort();
            }
            let served = tokio::time::timeout(std::time::Duration::from_secs(2), &mut server).await;
            if served.is_err() {
                server.abort();
                let _ = server.await;
            }
            let done =
                result.expect("terminal read did not queue BEGIN/query/COMMIT before replies");
            assert!(matches!(served, Ok(Ok(()))), "scripted server: {served:?}");
            done
        })
    }

    #[test]
    fn terminal_read_queues_all_groups_and_reuses_a_clean_connection() {
        played(
            async |mut server| {
                queued(&mut server).await;
                server.write_all(&command("BEGIN", b'T')).await.unwrap();
                server.write_all(&rows(7)).await.unwrap();
                server.write_all(&command("COMMIT", b'I')).await.unwrap();
                statement(&mut server).await.expect("ordinary reuse");
                server.write_all(ANSWERED).await.unwrap();
            },
            async |store: &Postgres| {
                let mut transaction = store.begin().await.unwrap();
                assert_eq!(
                    transaction
                        .query_last_read("SELECT $1::int8", &[SqlValue::Int8(7)])
                        .await,
                    Ok(vec![vec![SqlValue::Int8(7)]])
                );
                transaction.commit().await.unwrap();
                assert_eq!(store.pool.idle.borrow().len(), 1);
                assert_eq!(store.query_once("SELECT 0", &[]).await, Ok(Vec::new()));
            },
        );
    }

    #[test]
    fn terminal_query_error_survives_the_commits_rollback_tag() {
        played(
            async |mut server| {
                queued(&mut server).await;
                server.write_all(&command("BEGIN", b'T')).await.unwrap();
                server.write_all(&error("40001", b'E')).await.unwrap();
                server.write_all(&command("ROLLBACK", b'I')).await.unwrap();
                statement(&mut server).await.expect("clean reuse");
                server.write_all(ANSWERED).await.unwrap();
            },
            async |store: &Postgres| {
                let mut transaction = store.begin().await.unwrap();
                assert_eq!(
                    transaction
                        .query_last_read("SELECT $1::int8", &[SqlValue::Int8(7)])
                        .await,
                    Err(StoreError::ConcurrentChange)
                );
                assert_eq!(
                    transaction.commit().await,
                    Err(StoreError::ConcurrentChange)
                );
                assert_eq!(store.pool.idle.borrow().len(), 1);
                assert_eq!(store.query_once("SELECT 0", &[]).await, Ok(Vec::new()));
            },
        );
    }

    #[test]
    fn terminal_end_error_is_deferred_and_discards_the_connection() {
        for disconnect in [false, true] {
            played(
                async move |mut server| {
                    queued(&mut server).await;
                    server.write_all(&command("BEGIN", b'T')).await.unwrap();
                    server.write_all(&rows(7)).await.unwrap();
                    if !disconnect {
                        server.write_all(&error("40001", b'I')).await.unwrap();
                    }
                },
                async move |store: &Postgres| {
                    let mut transaction = store.begin().await.unwrap();
                    assert_eq!(
                        transaction
                            .query_last_read("SELECT $1::int8", &[SqlValue::Int8(7)])
                            .await,
                        Ok(vec![vec![SqlValue::Int8(7)]])
                    );
                    assert_eq!(
                        transaction.commit().await,
                        Err(if disconnect {
                            StoreError::Unavailable
                        } else {
                            StoreError::ConcurrentChange
                        })
                    );
                    assert!(store.pool.idle.borrow().is_empty());
                },
            );
        }
    }

    #[test]
    fn terminal_transaction_rejects_later_statements_without_sql() {
        let finished = Rc::new(std::cell::Cell::new(false));
        let server_finished = Rc::clone(&finished);
        played(
            async move |mut server| {
                queued(&mut server).await;
                server.write_all(&command("BEGIN", b'T')).await.unwrap();
                server.write_all(&rows(7)).await.unwrap();
                server.write_all(&command("COMMIT", b'I')).await.unwrap();
                while !server_finished.get() {
                    tokio::task::yield_now().await;
                }
                assert!(
                    tokio::time::timeout(
                        std::time::Duration::from_millis(20),
                        message(&mut server)
                    )
                    .await
                    .is_err(),
                    "no later SQL or transaction command"
                );
            },
            async move |store: &Postgres| {
                let mut transaction = store.begin().await.unwrap();
                transaction
                    .query_last_read("SELECT $1::int8", &[SqlValue::Int8(7)])
                    .await
                    .unwrap();
                assert!(transaction.query("SELECT 2", &[]).await.is_err());
                assert!(transaction.query_last_read("SELECT 2", &[]).await.is_err());
                assert!(transaction.execute("SELECT 2", &[]).await.is_err());
                assert!(transaction.batch("SELECT 2").await.is_err());
                assert!(transaction.savepoint().await.is_err());
                assert!(transaction.release_savepoint().await.is_err());
                assert!(transaction.rollback_to_savepoint().await.is_err());
                transaction.rollback().await.unwrap();
                finished.set(true);
            },
        );
    }

    #[test]
    fn cancelled_terminal_read_discards_its_connection_and_forbids_further_sql() {
        // Cancel immediately after the first poll, after enqueueing reaches the
        // server, and during response delivery. All must drop the owned connection.
        for phase in 0..3 {
            let responses = phase == 2;
            let cancelled = Rc::new(std::cell::Cell::new(false));
            let server_cancelled = Rc::clone(&cancelled);
            let queued_phase = Rc::new(std::cell::Cell::new(false));
            let server_phase = Rc::clone(&queued_phase);
            played(
                async move |mut server| {
                    queued(&mut server).await;
                    if responses {
                        server.write_all(&command("BEGIN", b'T')).await.unwrap();
                        // Parse/bind and row metadata arrive, but no terminal response.
                        let partial = rows(7);
                        server
                            .write_all(&partial[..partial.len() - command("SELECT 1", b'T').len()])
                            .await
                            .unwrap();
                    }
                    server_phase.set(true);
                    while !server_cancelled.get() {
                        tokio::task::yield_now().await;
                    }
                    if responses {
                        server.write_all(&command("SELECT 1", b'T')).await.unwrap();
                    } else {
                        server.write_all(&command("BEGIN", b'T')).await.unwrap();
                        server.write_all(&rows(7)).await.unwrap();
                    }
                    server.write_all(&command("COMMIT", b'I')).await.unwrap();
                    if let Some((kind, _)) = message(&mut server).await {
                        assert_eq!(kind, b'X', "only Terminate may follow cancellation");
                    }
                },
                async move |store: &Postgres| {
                    let mut transaction = store.begin().await.unwrap();
                    let params = [SqlValue::Int8(7)];
                    let mut reading = transaction.query_last_read("SELECT $1::int8", &params);
                    std::future::poll_fn(|cx| {
                        assert!(reading.as_mut().poll(cx).is_pending());
                        Poll::Ready(())
                    })
                    .await;
                    if phase != 0 {
                        while !queued_phase.get() {
                            tokio::task::yield_now().await;
                        }
                    }
                    drop(reading);
                    assert_eq!(
                        store.pool.slots.open.get(),
                        0,
                        "future owned and dropped its slot"
                    );
                    assert!(store.pool.idle.borrow().is_empty());
                    assert!(transaction.query("SELECT 2", &[]).await.is_err());
                    assert!(transaction.execute("SELECT 2", &[]).await.is_err());
                    assert!(transaction.savepoint().await.is_err());
                    assert_eq!(transaction.rollback().await, Err(StoreError::Unavailable));
                    cancelled.set(true);
                },
            );
        }
    }

    #[test]
    fn terminal_commit_keeps_end_failure_precedence_over_query_failure() {
        played(
            async |mut server| {
                queued(&mut server).await;
                server.write_all(&command("BEGIN", b'T')).await.unwrap();
                server.write_all(&error("40001", b'E')).await.unwrap();
                // Dropping the server makes the saved terminal failure unavailable.
            },
            async |store: &Postgres| {
                let mut transaction = store.begin().await.unwrap();
                assert_eq!(
                    transaction
                        .query_last_read("SELECT $1::int8", &[SqlValue::Int8(7)])
                        .await,
                    Err(StoreError::ConcurrentChange)
                );
                assert_eq!(transaction.commit().await, Err(StoreError::Unavailable));
            },
        );
    }

    #[test]
    fn extension_parameter_keeps_the_ordinary_transaction_open() {
        played(
            async |mut server| {
                let (kind, begin) = message(&mut server).await.unwrap();
                assert_eq!(kind, b'Q');
                assert_eq!(begin, b"BEGIN ISOLATION LEVEL REPEATABLE READ\0");
                let (kind, parse) = message(&mut server).await.unwrap();
                assert_eq!(kind, b'P');
                assert_eq!(
                    &parse[parse.len() - 4..],
                    &0u32.to_be_bytes(),
                    "unspecified extension OID"
                );
                statement(&mut server).await.unwrap();
                server.write_all(&command("BEGIN", b'T')).await.unwrap();
                let mut answered = ANSWERED.to_vec();
                answered[ANSWERED.len() - 1] = b'T';
                server.write_all(&answered).await.unwrap();
                let (kind, ending) = message(&mut server).await.unwrap();
                assert_eq!(kind, b'Q');
                assert_eq!(ending, b"ROLLBACK\0");
                server.write_all(&command("ROLLBACK", b'I')).await.unwrap();
            },
            async |store: &Postgres| {
                let mut transaction = store.begin().await.unwrap();
                transaction
                    .query_last_read(
                        "SELECT $1::vector",
                        &[SqlValue::Null(jadpo_runtime::SqlType::Vector)],
                    )
                    .await
                    .unwrap();
                transaction.rollback().await.unwrap();
            },
        );
    }
}
