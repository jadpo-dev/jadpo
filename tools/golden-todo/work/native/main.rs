//! Test-only controller around generated native code. Requests use the
//! production HTTP runtime and codecs. Controls never become app routes.
use jadpo_host_native::{NativeHost, Postgres, Runtime};
use jadpo_runtime::{
    Database, Executor, Host, Isolation, LocalFuture, Outbound, Row, SqlValue, StoreError,
    Transaction,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    io::{self, BufRead, Write},
    rc::Rc,
};
include!(concat!(env!("OUT_DIR"), "/module.rs"));
struct Observed {
    record: Rc<jadpo_runtime::record::Record>,
    outage: Cell<bool>,
    begins: Cell<u64>,
    hook: RefCell<Option<(String, String)>>,
}
impl Default for Observed {
    fn default() -> Self {
        Self {
            record: Rc::new(jadpo_runtime::record::Record::new([7; 32])),
            outage: Cell::new(false),
            begins: Cell::new(0),
            hook: RefCell::new(None),
        }
    }
}
struct Counted {
    inner: jadpo_runtime::record::Recorded<Postgres>,
    observed: Rc<Observed>,
    fixture: Rc<Postgres>,
}
impl Database for Counted {
    fn begin_at(
        &self,
        isolation: Isolation,
    ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
        if self.observed.outage.get() {
            return Box::pin(async { Err(StoreError::Unavailable) });
        }
        Box::pin(async move {
            let inner = self.inner.begin_at(isolation).await?;
            self.observed.begins.set(self.observed.begins.get() + 1);
            Ok(Box::new(Hooked {
                inner,
                observed: self.observed.clone(),
                fixture: self.fixture.clone(),
            }) as Box<dyn Transaction>)
        })
    }
    fn query_once<'a>(
        &'a self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        if self.observed.outage.get() {
            return Box::pin(async { Err(StoreError::Unavailable) });
        }
        self.inner.query_once(sql, params)
    }
}

struct Hooked {
    inner: Box<dyn Transaction>,
    observed: Rc<Observed>,
    fixture: Rc<Postgres>,
}
impl Transaction for Hooked {
    fn query<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        Box::pin(async move {
            let rows = self.inner.query(sql, params).await?;
            let mutation = if self
                .observed
                .hook
                .borrow()
                .as_ref()
                .is_some_and(|(pattern, _)| sql.starts_with("SELECT") && sql.contains(pattern))
            {
                self.observed.hook.borrow_mut().take().map(|(_, sql)| sql)
            } else {
                None
            };
            if let Some(statement) = mutation {
                self.fixture
                    .query_once(&statement, &[])
                    .await
                    .expect("concurrent fixture mutation");
            }
            Ok(rows)
        })
    }
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<u64, StoreError>> {
        self.inner.execute(sql, params)
    }
    fn batch<'a>(&'a mut self, sql: &'a str) -> LocalFuture<'a, Result<(), StoreError>> {
        self.inner.batch(sql)
    }
    fn commit(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
        self.inner.commit()
    }
    fn rollback(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
        self.inner.rollback()
    }
}
#[derive(Default)]
struct ProviderState {
    calls: RefCell<Vec<Value>>,
    answers: RefCell<Vec<String>>,
    receipts: RefCell<BTreeMap<String, (String, Vec<u8>)>>,
    mutate: RefCell<Option<String>>,
    now: Cell<i64>,
}
struct Mail {
    state: Rc<ProviderState>,
    database: Postgres,
}
impl jadpo_drivers::package::Fetch for Mail {
    fn fetch<'a>(
        &'a self,
        request: &'a jadpo_runtime::provider::Request,
    ) -> LocalFuture<
        'a,
        Result<jadpo_runtime::provider::Response, jadpo_drivers::package::FetchError>,
    > {
        Box::pin(async move {
            let key = request
                .headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("Idempotency-Key"))
                .map(|(_, v)| v.clone())
                .expect("runtime provider key");
            let body: Value = serde_json::from_str(request.body.as_deref().unwrap_or_default())
                .expect("typed request JSON");
            self.state
                .calls
                .borrow_mut()
                .push(json!({"key":key,"body":body,"url":request.url}));
            if let Some(sql) = self.state.mutate.borrow_mut().take() {
                self.database
                    .query_once(&sql, &[])
                    .await
                    .expect("concurrent fixture mutation");
            }
            let answer = if self.state.answers.borrow().is_empty() {
                "accepted".into()
            } else {
                self.state.answers.borrow_mut().remove(0)
            };
            let fingerprint = body.to_string();
            let stored = self.state.receipts.borrow().get(&key).cloned();
            if stored
                .as_ref()
                .is_some_and(|(original, _)| original != &fingerprint)
            {
                return Ok(jadpo_runtime::provider::Response {
                    status: 422,
                    headers: vec![],
                    body: br#"{"error":{"code":"idempotency_key_reused"}}"#.to_vec(),
                });
            }
            let receipt = stored.map(|(_,receipt)|receipt).unwrap_or_else(|| json!({"message_id":format!("mail_{}", self.state.receipts.borrow().len()+1),"accepted_at":instant(self.state.now.get())}).to_string().into_bytes());
            if answer == "accepted" || answer == "lost" {
                self.state
                    .receipts
                    .borrow_mut()
                    .insert(key, (fingerprint, receipt.clone()));
            }
            if answer == "lost" {
                return Err(jadpo_drivers::package::FetchError::NoAnswer(
                    "fixture lost acknowledgement".into(),
                ));
            }
            if answer == "not_sent" {
                return Err(jadpo_drivers::package::FetchError::NotSent(
                    "fixture outage".into(),
                ));
            }
            Ok(jadpo_runtime::provider::Response {
                status: if answer == "unauthorized" {
                    401
                } else if answer == "server_error" || answer == "unavailable" {
                    503
                } else {
                    200
                },
                headers: Vec::new(),
                body: if answer == "unauthorized" {
                    b"private-provider-secret".to_vec()
                } else if answer == "unavailable" {
                    br#"{"error":{"code":"unavailable"}}"#.to_vec()
                } else {
                    receipt
                },
            })
        })
    }
}
struct FixtureOutbound {
    jwks: String,
    unavailable: Cell<bool>,
}
impl Outbound for FixtureOutbound {
    fn get<'a>(
        &'a self,
        url: &'a str,
        _: usize,
        _: u64,
    ) -> LocalFuture<'a, Result<(u16, Vec<u8>), String>> {
        Box::pin(async move {
            if self.unavailable.get() {
                return Err("fixture issuer unavailable".into());
            }
            let body = if url == "https://identity.example/tenant/.well-known/openid-configuration"
            {
                json!({"issuer":"https://identity.example/tenant/","jwks_uri":"https://keys.example/published"}).to_string()
            } else if url == "https://keys.example/published" {
                self.jwks.clone()
            } else {
                return Err("unrecognized fixture host".into());
            };
            Ok((200, body.into_bytes()))
        })
    }
}
struct Controlled {
    inner: NativeHost,
    database: Counted,
    now: Cell<i64>,
    outbound: FixtureOutbound,
}
impl Host for Controlled {
    fn database(&self, _: &str) -> Option<&dyn Database> {
        Some(&self.database)
    }
    fn capability(&self, name: &str) -> Option<&dyn jadpo_runtime::capability::Capability> {
        self.inner.capability(name)
    }
    fn random(&self, bytes: &mut [u8]) -> bool {
        self.inner.random(bytes)
    }
    fn sleep(&self, ms: u64) -> LocalFuture<'_, ()> {
        self.inner.sleep(ms)
    }
    fn now_ms(&self) -> Option<i64> {
        Some(self.now.get())
    }
    fn monotonic_ms(&self) -> Option<u64> {
        self.inner.monotonic_ms().map(|now| {
            now.saturating_add(self.now.get().saturating_sub(1768478400000).max(0) as u64)
        })
    }
    fn outbound(&self) -> Option<&dyn Outbound> {
        Some(&self.outbound)
    }
}
fn instant(ms: i64) -> String {
    jadpo_value::Value::Instant(ms)
        .to_string()
        .trim_matches('"')
        .into()
}
fn scalar(value: &SqlValue) -> Value {
    match value {
        SqlValue::Null(_) => Value::Null,
        SqlValue::Bool(v) => json!(v),
        SqlValue::Int4(v) => json!(v),
        SqlValue::Int8(v) => json!(v),
        SqlValue::Text(v) => json!(v),
        SqlValue::Uuid(v) => json!(uuid(v)),
        SqlValue::Timestamp(v) => json!(instant(*v / 1000)),
        other => json!(format!("{other:?}")),
    }
}
fn uuid(bytes: &[u8; 16]) -> String {
    let h: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
}
fn main() {
    let runtime = Runtime::new().expect("native runtime");
    let program = golden::program().expect("compiled plan");
    let variables: BTreeMap<String, String> = std::env::vars().collect();
    let values = jadpo_runtime::config::load(
        &program.runtime().config,
        program.plan().json.codecs.validators,
        &variables,
        jadpo_runtime::config::Mode::Environment,
        &|bytes| NativeHost::new().random(bytes),
    )
    .expect("fixture configuration");
    let program = program.with_config(&values);
    let url = variables.get("DATABASE_URL").expect("DATABASE_URL");
    let observed = Rc::new(Observed::default());
    let state = Rc::new(ProviderState::default());
    let mail = Rc::new(Mail {
        state: state.clone(),
        database: jadpo_host_native::postgres(url).expect("mail fixture store"),
    });
    let supplies = jadpo_host_native::supplies().with_fetch(mail);
    let drivers = jadpo_host_native::Drivers::build(
        &program.runtime().connections,
        &program.started().expect("started"),
        &supplies,
    );
    let signing = jadpo_value::crypto::KeyRing::from_signing_text(
        variables
            .get("JWT_SIGNING_KEY")
            .expect("generated fixture JWT key"),
        None,
    );
    let jwks=json!({"keys":[{"kty":"OKP","crv":"Ed25519","alg":"EdDSA","kid":"fixture","x":jadpo_value::crypto::public_key(&signing).expect("public key") }]}).to_string();
    let host = Controlled {
        inner: NativeHost::new().with_drivers(drivers),
        database: Counted {
            inner: jadpo_runtime::record::Recorded::new(
                jadpo_host_native::postgres(url).expect("store"),
                observed.record.clone(),
            ),
            observed: observed.clone(),
            fixture: Rc::new(jadpo_host_native::postgres(url).expect("mutation fixture")),
        },
        now: Cell::new(1768478400000),
        outbound: FixtureOutbound {
            jwks,
            unavailable: Cell::new(false),
        },
    };
    let mut worker = jadpo_runtime::http::Worker::default();
    let mut deferred = Vec::new();
    for line in io::stdin().lock().lines() {
        let line = line.expect("controller input");
        let command: Value = serde_json::from_str(&line).expect("controller JSON");
        let record_start = observed.record.entries().len();
        let begins_start = observed.begins.get();
        let result=runtime.block_on(async {
            match command["op"].as_str().expect("op") {
                "clock"=>{ let now=command["at"].as_i64().expect("clock milliseconds");host.now.set(now);state.now.set(now);json!({"at":now}) }
                "worker_reset"=>{worker=jadpo_runtime::http::Worker::default(); deferred.clear(); *observed.hook.borrow_mut()=None; json!({"reset":true})}
                "outage"=>{observed.outage.set(command["database"].as_bool().unwrap_or(false));host.outbound.unavailable.set(command["issuer"].as_bool().unwrap_or(false));json!({"set":true})}
                "mail"=>{ if let Some(answers)=command["answers"].as_array(){*state.answers.borrow_mut()=answers.iter().map(|v|v.as_str().expect("answer").into()).collect();} *state.mutate.borrow_mut()=command["mutate"].as_str().map(str::to_string); if command["clear"]==true {state.calls.borrow_mut().clear();state.receipts.borrow_mut().clear();} json!({"calls":*state.calls.borrow(),"accepted":state.receipts.borrow().len(),"receipts":state.receipts.borrow().values().map(|(_,bytes)|serde_json::from_slice::<Value>(bytes).expect("receipt")).collect::<Vec<_>>()}) }
                "sql"=>{match host.database.inner.inner().query_once(command["sql"].as_str().expect("SQL"),&[]).await {Ok(rows)=>json!({"rows":rows.iter().map(|r|r.iter().map(scalar).collect::<Vec<_>>()).collect::<Vec<_>>()}),Err(error)=>json!({"error":format!("{error:?}")})}}
                "request"=>{
                    let headers=command["headers"].as_array().map(|hs|hs.iter().map(|h|(h[0].as_str().unwrap().into(),h[1].as_str().unwrap().into())).collect()).unwrap_or_default();
                    let (path, query)=command["path"].as_str().unwrap().split_once('?').unwrap_or((command["path"].as_str().unwrap(),""));
                    let request=jadpo_runtime::http::Request {query:if query.is_empty(){None}else{Some(query.into())},method:command["method"].as_str().unwrap().into(),path:path.into(),headers,body:command["body"].as_str().unwrap_or("").as_bytes().to_vec(),address:Some("127.0.0.1".into()),too_large:false};
                    let response=jadpo_runtime::http::serve_request(&program.app(),&program,&host,&worker,&request).await;
                    deferred.extend(response.deferred);
                    json!({"status":response.status,"headers":response.headers,"body":String::from_utf8(response.body).unwrap()})
                }
                "drain"=>{let outcomes=jadpo_runtime::runner::run_deferred(&program,&host,std::mem::take(&mut deferred)).await; json!({"outcomes":outcomes.iter().map(|o|format!("{o:?}")).collect::<Vec<_>>()})}
                "deliver_one"=>{
                    let mut work=std::mem::take(&mut deferred);
                    let delivery=match work.remove(0) {jadpo_runtime::runner::Deferred::Deliveries(mut deliveries)=>{let first=deliveries.remove(0);if !deliveries.is_empty(){deferred.push(jadpo_runtime::runner::Deferred::Deliveries(deliveries));}first},_=>panic!("delivery expected")};
                    deferred.extend(work);
                    let mut ctx=jadpo_runtime::Ctx::of(&program,&host,jadpo_runtime::Env{seed:None,clock:jadpo_runtime::Clock::fixed(host.now.get())});
                    let outcome=jadpo_runtime::runner::deliver(&program,&mut ctx,delivery).await;
                    deferred.extend(ctx.take_deferred());
                    json!({"outcome":format!("{outcome:?}")})
                }
                "sweep"=>{let outcomes=jadpo_runtime::runner::sweep(&program,&host,"main",host.now.get()).await.expect("sweep");json!({"outcomes":outcomes.iter().map(|o|format!("{o:?}")).collect::<Vec<_>>()})}
                "schedule"=>{
                    let schedule=program.runtime().schedules.schedules.iter().find(|s|Some(s.name.as_str())==command["name"].as_str()).expect("schedule");
                    let callable=match schedule.run {jadpo_std::schedule::ScheduleRun::Action(n)=>n,_=>panic!("generated purge uses purge op")};
                    let mut ctx=jadpo_runtime::Ctx::of(&program,&host,jadpo_runtime::Env{seed:None,clock:jadpo_runtime::Clock::fixed(host.now.get())});
                    let outcome=jadpo_runtime::runner::as_system(&program,&mut ctx,jadpo_runtime::runner::Job{callable,args:vec![],system:jadpo_runtime::System::default()}).await;
                    deferred.extend(ctx.take_deferred());
                    json!({"outcome":format!("{outcome:?}")})
                }
                "mutation_hook"=>{*observed.hook.borrow_mut()=Some((command["match"].as_str().expect("SQL match").into(),command["sql"].as_str().expect("fixture mutation").into()));json!({"set":true})}
                "concurrent_tick"=>{
                    use std::task::Poll;
                    let mut a=Box::pin(jadpo_runtime::schedules::tick(&program,&host,host.now.get()));
                    let mut b=Box::pin(jadpo_runtime::schedules::tick(&program,&host,host.now.get()));
                    let(mut first,mut second)=(None,None);
                    std::future::poll_fn(|cx|{if first.is_none(){if let Poll::Ready(value)=a.as_mut().poll(cx){first=Some(value);}}if second.is_none(){if let Poll::Ready(value)=b.as_mut().poll(cx){second=Some(value);}}if first.is_some()&&second.is_some(){Poll::Ready(())}else{Poll::Pending}}).await;
                    json!({"first":format!("{first:?}"),"second":format!("{second:?}")})
                }
                "tick"=>{let tick=jadpo_runtime::schedules::tick(&program,&host,host.now.get()).await; json!({"tick":format!("{tick:?}")})}
                "jwks"=>json!({"jwks":serde_json::from_str::<Value>(&host.outbound.jwks).expect("fixture JWKS")}),
                "jwt"=>{let claims=json!({"iss":"https://identity.example/tenant/","aud":"golden-todo","sub":command["subject"],"exp":host.now.get()/1000+3600,"email":"attacker@example.test","role":"administrator"});let body=format!("{}.{}",jadpo_value::encoding::to_base64url(br#"{"alg":"EdDSA","kid":"fixture"}"#),jadpo_value::encoding::to_base64url(claims.to_string().as_bytes()));let sig=jadpo_value::crypto::sign(body.as_bytes(),&signing).expect("fixture JWT");json!({"token":format!("{body}.{}",jadpo_value::encoding::to_base64url(&sig))})}
                "digest"=>{json!({"digest":blake3::hash(command["text"].as_str().unwrap().as_bytes()).to_hex().to_string()})}
                "open"=>{json!({"payload":jadpo_runtime::http::signed::SigningKey::new(variables.get("SESSION_SIGNING_KEY").unwrap(),None).open(command["signed"].as_str().unwrap()).expect("verified fixture signed claims")})}
                "sign"=>{json!({"signed":jadpo_runtime::http::signed::SigningKey::new(variables.get("SESSION_SIGNING_KEY").unwrap(),None).sign(command["payload"].as_str().unwrap())})}
                other=>panic!("unsupported controller operation: {other}"),
            }
        });
        println!(
            "{}",
            json!({"result":result,"transaction_begins":observed.begins.get()-begins_start,"statements":observed.record.entries()[record_start..].iter().filter_map(|entry|match entry {jadpo_runtime::record::Entry::Statement{sql,..}=>Some(sql.clone()),jadpo_runtime::record::Entry::Ended{committed,..}=>Some(if *committed {"COMMIT"} else {"ROLLBACK"}.into()),_=>None}).collect::<Vec<_>>()})
        );
        io::stdout().flush().expect("flushed response");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jadpo_drivers::package::Fetch;
    #[test]
    fn fixture_transport_not_sent_is_distinct_from_http_unavailable() {
        let state = Rc::new(ProviderState::default());
        *state.answers.borrow_mut() = vec!["not_sent".into(), "unavailable".into()];
        let provider = Mail {
            state: state.clone(),
            database: jadpo_host_native::postgres("postgresql://fixture@127.0.0.1:1/local")
                .expect("unopened fixture store"),
        };
        let request = jadpo_runtime::provider::Request {
            method: jadpo_std::HttpMethod::Post,
            url: "https://mail.example.invalid/v1/reminders".into(),
            headers: vec![("Idempotency-Key".into(), "transport-control".into())],
            body: Some("{}".into()),
            timeout_ms: 1000,
            reading: Default::default(),
            repeatable: true,
        };
        Runtime::new().expect("runtime").block_on(async {
            assert!(matches!(
                provider.fetch(&request).await,
                Err(jadpo_drivers::package::FetchError::NotSent(_))
            ));
            assert_eq!(
                provider
                    .fetch(&request)
                    .await
                    .expect("HTTP failure response")
                    .status,
                503
            );
            assert!(state.receipts.borrow().is_empty());
        });
    }
    #[test]
    fn fixture_rejects_same_key_with_different_payload() {
        let state = Rc::new(ProviderState::default());
        let provider = Mail {
            state: state.clone(),
            database: jadpo_host_native::postgres("postgresql://fixture@127.0.0.1:1/local")
                .expect("unopened fixture store"),
        };
        let request = |body: &str| jadpo_runtime::provider::Request {
            method: jadpo_std::HttpMethod::Post,
            url: "https://mail.example.invalid/v1/reminders".into(),
            headers: vec![("Idempotency-Key".into(), "negative-control".into())],
            body: Some(body.into()),
            timeout_ms: 1000,
            reading: Default::default(),
            repeatable: true,
        };
        let runtime = Runtime::new().expect("runtime");
        runtime.block_on(async {
            let first = provider
                .fetch(&request(r#"{"title":"Original"}"#))
                .await
                .expect("first response");
            let repeated = provider
                .fetch(&request(r#"{"title":"Original"}"#))
                .await
                .expect("same response");
            let changed = provider
                .fetch(&request(r#"{"title":"Changed"}"#))
                .await
                .expect("reuse response");
            assert_eq!(first.status, 200);
            assert_eq!(first.body, repeated.body);
            assert_eq!(changed.status, 422);
            assert_eq!(
                state.receipts.borrow().len(),
                1,
                "no second accepted message"
            );
        });
    }
}
