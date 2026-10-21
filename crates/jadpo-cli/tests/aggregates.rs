//! Aggregate queries on live PostgreSQL (area 6, "Counts, sums and
//! groups"; chunk 5.6c): a usage-per-month query and a counts-by-status
//! query each run as one statement, and give exactly what the interpreter
//! works out in code from the same records, through `list.sum`,
//! `money.sum`, `list.max_by` and `decimal.divide`.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips the
//! live part.

use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jadpo_cli::{Output, run};
use jadpo_interp::Program;
use jadpo_runtime::{
    Clock, Ctx, Database, Env, Executor, Isolation, LocalFuture, Outcome, Row, SqlValue,
    StoreError, Transaction,
};
use jadpo_term::Environment;
use jadpo_test::host::Harness;
use jadpo_value::Value;

use crate::support;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    let mut output = run(&args, &env.with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

/// A metered API's calls, the two aggregate queries the chunk is done
/// with, a plain query of the calls, and actions that call each aggregate
/// query or work the same rows out in code from the plain one.
fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("aggregates-{name}"));
    let _ = fs::remove_dir_all(&root);
    write(&root, "app.jadpo", "application {}\n");
    for (file, text) in [
        (
            "types/call_day.jadpo",
            "/// The day a call was made.\ntype CallDay = CalendarDate {}\n",
        ),
        (
            "types/spent_tokens.jadpo",
            "/// The tokens a call spent.\ntype SpentTokens = Int {\n    min: 0\n}\n",
        ),
        (
            "types/call_charge.jadpo",
            "/// What a call was charged.\ntype CallCharge = Money {\n    currency: Currency.gbp\n}\n",
        ),
        (
            "types/reply_kind.jadpo",
            "/// How a call was answered.\ntype ReplyKind = Enum {\n    answered\n    throttled\n    refused\n}\n",
        ),
        (
            "entities/api_call.jadpo",
            "use metering/types { CallCharge, CallDay, ReplyKind, SpentTokens }\n\n/// One call to the API.\nentity ApiCall {\n    id: Uuid\n    made_on: CallDay\n    tokens_spent: SpentTokens\n    charge: CallCharge\n    reply: ReplyKind\n\n    policy: {\n        read: [Access.public]\n    }\n}\n",
        ),
        (
            "types/month_of_calls.jadpo",
            "use metering/entities { ApiCall }\n\n/// A month of calls.\ntype MonthOfCalls = Output {\n    call_year: Int\n    call_month: Int\n    calls_made: Int\n    tokens_billed: Int\n    amount_billed: Money\n    largest_call: ApiCall.tokens_spent\n    mean_tokens: Decimal\n}\n",
        ),
        (
            "types/reply_tally.jadpo",
            "use metering/types { ReplyKind }\n\n/// How many calls were answered one way.\ntype ReplyTally = Output {\n    reply: ReplyKind\n    calls_made: Int\n}\n",
        ),
        (
            "types/call_row.jadpo",
            "use metering/entities { ApiCall }\n\n/// A call as code reads it.\ntype CallRow = Output {\n    pick: [ApiCall.made_on, ApiCall.tokens_spent, ApiCall.charge, ApiCall.reply]\n}\n",
        ),
        (
            "queries/monthly_calls.jadpo",
            "use metering/entities { ApiCall }\nuse metering/types { CallDay, MonthOfCalls }\n\n/// Calls since `since` by month, the latest first.\nquery monthly_calls(since: CallDay) -> List<MonthOfCalls> {\n    from: ApiCall\n    where: ApiCall.made_on >= since\n    group: {\n        call_year: temporal.year(ApiCall.made_on)\n        call_month: temporal.month(ApiCall.made_on)\n    }\n    aggregate: {\n        calls_made: count()\n        tokens_billed: sum(ApiCall.tokens_spent)\n        amount_billed: sum(ApiCall.charge)\n        largest_call: max(ApiCall.tokens_spent)\n        mean_tokens: average(ApiCall.tokens_spent, scale: 2, rounding: Rounding.half_even)\n    }\n    order: [call_year desc, call_month desc]\n    limit: 100\n}\n",
        ),
        (
            "queries/reply_tallies.jadpo",
            "use metering/entities { ApiCall }\nuse metering/types { CallDay, ReplyTally }\n\n/// Calls since `since` by how they were answered, the most first.\nquery reply_tallies(since: CallDay) -> List<ReplyTally> {\n    from: ApiCall\n    where: ApiCall.made_on >= since\n    group: {\n        reply: ApiCall.reply\n    }\n    aggregate: {\n        calls_made: count()\n    }\n    order: [calls_made desc]\n    limit: 3\n}\n",
        ),
        (
            "queries/calls_since.jadpo",
            "use metering/entities { ApiCall }\nuse metering/types { CallDay, CallRow }\n\n/// Every call since `since`.\nquery calls_since(since: CallDay) -> List<CallRow> {\n    from: ApiCall\n    where: ApiCall.made_on >= since\n    order: [ApiCall.made_on asc]\n    limit: 5000\n}\n",
        ),
        (
            "actions/report_months.jadpo",
            "use metering/queries { monthly_calls }\nuse metering/types { CallDay, MonthOfCalls }\n\n/// The months, as the query works them out.\naction report_months(since: CallDay) -> List<MonthOfCalls> {\n    return monthly_calls(since)\n}\n",
        ),
        (
            "actions/report_tallies.jadpo",
            "use metering/queries { reply_tallies }\nuse metering/types { CallDay, ReplyTally }\n\n/// The tallies, as the query works them out.\naction report_tallies(since: CallDay) -> List<ReplyTally> {\n    return reply_tallies(since)\n}\n",
        ),
        (
            "types/call_count.jadpo",
            "/// How many calls a month holds: at least the one that made it a month.\ntype CallCount = Int {\n    min: 1\n}\n",
        ),
        (
            "failures/no_calls.jadpo",
            "/// A month with no calls, which grouping never makes.\ntype NoCalls = Failure {\n    kind: FailureKind.invalid_value\n}\n",
        ),
        (
            "actions/months_in_code.jadpo",
            "use metering/failures { NoCalls }\nuse metering/queries { calls_since }\nuse metering/types { CallCount, CallDay, CallRow, MonthOfCalls }\n\n/// The months, worked out in code from the calls.\naction months_in_code(since: CallDay) fails NoCalls -> List<MonthOfCalls> {\n    var calls: List<CallRow> = calls_since(since)\n    var keys: List<Int> = list.distinct(list.map(calls, call -> temporal.year(call.made_on) * 100 + temporal.month(call.made_on)))\n    var mut months: List<MonthOfCalls> = []\n    for key in list.sort_by(keys, key -> 0 - key) {\n        var group: List<CallRow> = list.filter(calls, call -> temporal.year(call.made_on) * 100 + temporal.month(call.made_on) == key)\n        var made: CallCount = attempt CallCount(list.count(group)) {\n            min: NoCalls\n        }\n        var tokens: Int = list.sum(list.map(group, call -> call.tokens_spent))\n        match list.max_by(group, call -> call.tokens_spent) {\n            some(top) -> {\n                months = list.append(months, MonthOfCalls {\n                    call_year: integer.divide(key, 100, rounding: Rounding.down)\n                    call_month: key % 100\n                    calls_made: list.count(group)\n                    tokens_billed: tokens\n                    amount_billed: money.sum(list.map(group, call -> call.charge), currency: Currency.gbp)\n                    largest_call: top.tokens_spent\n                    mean_tokens: decimal.divide(tokens * 1.00, made, rounding: Rounding.half_even)\n                })\n            }\n            none -> {}\n        }\n    }\n    return months\n}\n",
        ),
        (
            "actions/tallies_in_code.jadpo",
            "use metering/queries { calls_since }\nuse metering/types { CallDay, CallRow, ReplyKind, ReplyTally }\n\n/// The tallies, worked out in code from the calls.\naction tallies_in_code(since: CallDay) -> List<ReplyTally> {\n    var calls: List<CallRow> = calls_since(since)\n    var mut tallies: List<ReplyTally> = []\n    for reply in [ReplyKind.answered, ReplyKind.throttled, ReplyKind.refused] {\n        var made: Int = list.count(list.filter(calls, call -> call.reply == reply))\n        if (made > 0) {\n            tallies = list.append(tallies, ReplyTally {\n                reply: reply\n                calls_made: made\n            })\n        }\n    }\n    return list.sort_by(tallies, tally -> 0 - tally.calls_made)\n}\n",
        ),
    ] {
        write(&root, &format!("domains/metering/{file}"), text);
    }
    let formatted = jadpo(&root, &["fmt"]);
    assert_eq!(
        formatted.code, 0,
        "{}{}",
        formatted.stdout, formatted.stderr
    );
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    root
}

/// A store that counts the statements sent through it.
struct Counted {
    store: jadpo_host_native::Postgres,
    sent: Rc<Cell<usize>>,
}

/// A transaction of a [`Counted`] store.
struct CountedTransaction {
    inner: Box<dyn Transaction>,
    sent: Rc<Cell<usize>>,
}

impl Transaction for CountedTransaction {
    fn query<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        self.sent.set(self.sent.get() + 1);
        self.inner.query(sql, params)
    }

    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<u64, StoreError>> {
        self.sent.set(self.sent.get() + 1);
        self.inner.execute(sql, params)
    }

    fn batch<'a>(&'a mut self, sql: &'a str) -> LocalFuture<'a, Result<(), StoreError>> {
        self.sent.set(self.sent.get() + 1);
        self.inner.batch(sql)
    }

    fn commit(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
        self.inner.commit()
    }

    fn rollback(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
        self.inner.rollback()
    }
}

impl Database for Counted {
    fn begin_at(
        &self,
        isolation: Isolation,
    ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
        let sent = self.sent.clone();
        Box::pin(async move {
            let inner = self.store.begin_at(isolation).await?;
            Ok(Box::new(CountedTransaction { inner, sent }) as Box<dyn Transaction>)
        })
    }
}

/// The days from 1970-01-01 to 2024-01-01.
const FIRST_DAY: i32 = 19_723;

/// A small, fixed generator, so the calls are the same every run.
struct Numbers(u64);

impl Numbers {
    fn below(&mut self, n: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % n
    }
}

/// Three thousand calls over three years: most answered, fewer throttled,
/// fewest refused, so the counts never tie.
fn calls(url: &str) {
    let store = jadpo_host_native::postgres(url).expect("store");
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    let mut numbers = Numbers(56);
    runtime.block_on(async {
        let mut transaction = store.begin().await.expect("begin");
        for i in 0..3000u64 {
            let mut id = [0u8; 16];
            id[..8].copy_from_slice(&(i + 1).to_be_bytes());
            id[8..].copy_from_slice(&i.wrapping_mul(2_654_435_761).to_be_bytes());
            id[6] = (id[6] & 0x0f) | 0x40;
            id[8] = (id[8] & 0x3f) | 0x80;
            let day = FIRST_DAY + i32::try_from(numbers.below(1096)).expect("a day");
            let tokens = i64::try_from(numbers.below(5001)).expect("tokens");
            let pence = i64::try_from(numbers.below(1000)).expect("pence");
            let reply = match numbers.below(10) {
                0..=6 => "answered",
                7 | 8 => "throttled",
                _ => "refused",
            };
            transaction
                .execute(
                    "INSERT INTO api_call (id, made_on, tokens_spent, charge_amount, charge_currency, reply) VALUES ($1, $2, $3, $4, $5, $6)",
                    &[
                        SqlValue::Uuid(id),
                        SqlValue::Date(day),
                        SqlValue::Int8(tokens),
                        SqlValue::Int8(pence),
                        SqlValue::Text("GBP".to_string()),
                        SqlValue::Text(reply.to_string()),
                    ],
                )
                .await
                .expect("insert");
        }
        transaction.commit().await.expect("commit");
    });
}

/// What the action called `name` gives for calls since `since`, and how
/// many statements it sent.
fn call(program: &Program, url: &str, name: &str, since: i32) -> (Value, usize) {
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    let sent = Rc::new(Cell::new(0));
    let host = Harness::new(Counted {
        store: jadpo_host_native::postgres(url).expect("store"),
        sent: sent.clone(),
    });
    let id = program.plan().find(name).expect("the action");
    let found = runtime.block_on(async {
        let env = Env {
            seed: Some(1),
            clock: Clock::TEST,
        };
        let mut ctx = Ctx::of(program, &host, env);
        program.run(&mut ctx, id, vec![Value::Date(since)]).await
    });
    let Outcome::Value(value) = found else {
        panic!("`{name}` gave {found:?}");
    };
    (value, sent.get())
}

#[test]
fn usage_per_month_and_counts_by_status_are_one_statement_each_and_what_code_gives() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("live");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server.create_database("aggregates").expect("database");
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    calls(&url);
    let loaded = jadpo_db::load(&root);
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    // Each aggregate query's plan is one grouped statement, with nothing
    // run before it and no include after it.
    for name in ["monthly_calls", "reply_tallies"] {
        let plan = program.plan().query_named(name).expect("the query's plan");
        assert!(plan.read.settings.is_empty(), "{name}");
        assert!(plan.read.select.includes.is_empty(), "{name}");
        assert!(
            plan.read.select.sql.contains("GROUP BY"),
            "{}",
            plan.read.select.sql
        );
    }
    // From the middle of March 2024: the first month is a part month.
    let since = FIRST_DAY + 74;
    let (by_query, sent) = call(&program, &url, "report_months", since);
    assert_eq!(sent, 1, "the usage-per-month query sends one statement");
    let (in_code, _) = call(&program, &url, "months_in_code", since);
    let Value::List(months) = &by_query else {
        panic!("not a list: {by_query:?}");
    };
    assert_eq!(months.len(), 34, "March 2024 to December 2026");
    assert_eq!(
        by_query, in_code,
        "the months the query gives are what code gives"
    );
    // Down to the text of each value, places and all.
    assert_eq!(format!("{by_query:?}"), format!("{in_code:?}"));
    let (by_query, sent) = call(&program, &url, "report_tallies", since);
    assert_eq!(sent, 1, "the counts-by-status query sends one statement");
    let (in_code, _) = call(&program, &url, "tallies_in_code", since);
    let Value::List(tallies) = &by_query else {
        panic!("not a list: {by_query:?}");
    };
    assert_eq!(tallies.len(), 3);
    assert_eq!(
        by_query, in_code,
        "the tallies the query gives are what code gives"
    );
    assert_eq!(format!("{by_query:?}"), format!("{in_code:?}"));
}
