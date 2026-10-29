//! Test-only raw-request controller around the generated application in
//! workerd. No controls are linked into the deployable that is shipped.
use jadpo_host_workers::{WorkersHost, wasm_bindgen::prelude::*};
use jadpo_runtime::{
    Database, Executor, Host, Isolation, LocalFuture, Outbound, Row, SqlValue, StoreError,
    Transaction,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
include!(concat!(env!("OUT_DIR"), "/module.rs"));
#[derive(Default)]
struct State {
    outage: Cell<bool>,
    hook: RefCell<Option<(String, String)>>,
    deferred: RefCell<Vec<jadpo_runtime::runner::Deferred>>,
    worker: RefCell<Rc<jadpo_runtime::http::Worker>>,
}
thread_local! {static STATE:Rc<State>=Rc::new(State::default());}
struct Borrowed<'a>(&'a dyn Database);
impl Database for Borrowed<'_> {
    fn begin_at(
        &self,
        isolation: Isolation,
    ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
        self.0.begin_at(isolation)
    }
    fn query_once<'a>(
        &'a self,
        sql: &'a str,
        args: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        self.0.query_once(sql, args)
    }
}
struct Counted<'a> {
    inner: jadpo_runtime::record::Recorded<Borrowed<'a>>,
    state: Rc<State>,
    begins: Rc<Cell<u64>>,
}
struct Hooked {
    inner: Box<dyn Transaction>,
    state: Rc<State>,
}
#[wasm_bindgen(wasm_bindgen=jadpo_host_workers::wasm_bindgen)]
extern "C" {
    #[wasm_bindgen(js_namespace=globalThis,js_name=goldenFixtureMutation)]
    fn fixture_mutation(sql: &str) -> js_sys::Promise;
}
impl Transaction for Hooked {
    fn query<'a>(
        &'a mut self,
        sql: &'a str,
        args: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        Box::pin(async move {
            let rows = self.inner.query(sql, args).await?;
            let mutation = if self
                .state
                .hook
                .borrow()
                .as_ref()
                .is_some_and(|(pattern, _)| sql.starts_with("SELECT") && sql.contains(pattern))
            {
                self.state.hook.borrow_mut().take().map(|(_, sql)| sql)
            } else {
                None
            };
            if let Some(statement) = mutation {
                wasm_bindgen_futures::JsFuture::from(fixture_mutation(&statement))
                    .await
                    .map_err(|_| StoreError::Fault("fixture mutation failed".into()))?;
            }
            Ok(rows)
        })
    }
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
        args: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<u64, StoreError>> {
        self.inner.execute(sql, args)
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
// A fixture mutation runs after a read completes but before its caller resumes.
impl Database for Counted<'_> {
    fn begin_at(
        &self,
        isolation: Isolation,
    ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
        Box::pin(async move {
            if self.state.outage.get() {
                return Err(StoreError::Unavailable);
            }
            let transaction = self.inner.begin_at(isolation).await?;
            self.begins.set(self.begins.get() + 1);
            Ok(Box::new(Hooked {
                inner: transaction,
                state: self.state.clone(),
            }) as Box<dyn Transaction>)
        })
    }
    fn query_once<'a>(
        &'a self,
        sql: &'a str,
        args: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        if self.state.outage.get() {
            return Box::pin(async { Err(StoreError::Unavailable) });
        }
        self.inner.query_once(sql, args)
    }
}
struct Controlled<'a> {
    inner: &'a WorkersHost,
    database: Counted<'a>,
    record: Rc<jadpo_runtime::record::Record>,
}
impl Host for Controlled<'_> {
    fn database(&self, _: &str) -> Option<&dyn Database> {
        Some(&self.database)
    }
    fn capability(&self, name: &str) -> Option<&dyn jadpo_runtime::capability::Capability> {
        self.inner.capability(name)
    }
    fn record(&self) -> Option<&jadpo_runtime::record::Record> {
        Some(&self.record)
    }
    fn random(&self, bytes: &mut [u8]) -> bool {
        self.inner.random(bytes)
    }
    fn sleep(&self, ms: u64) -> LocalFuture<'_, ()> {
        self.inner.sleep(ms)
    }
    fn now_ms(&self) -> Option<i64> {
        self.inner.now_ms()
    }
    fn monotonic_ms(&self) -> Option<u64> {
        self.inner.monotonic_ms()
    }
    fn outbound(&self) -> Option<&dyn Outbound> {
        self.inner.outbound()
    }
    fn rate_limit<'a>(
        &'a self,
        key: &'a str,
        limit: jadpo_std::http::RateLimit,
        now: i64,
    ) -> LocalFuture<'a, jadpo_runtime::Allowed> {
        self.inner.rate_limit(key, limit, now)
    }
    fn budget_ms(&self, invocation: jadpo_runtime::budget::Invocation) -> u64 {
        self.inner.budget_ms(invocation)
    }
}
#[wasm_bindgen(wasm_bindgen=jadpo_host_workers::wasm_bindgen)]
pub fn control(command: String, env: JsValue, context: JsValue) -> JsValue {
    wasm_bindgen_futures::future_to_promise(async move {
  let command:Value=serde_json::from_str(&command).map_err(|_|JsValue::from_str("invalid controller JSON"))?;
  let program=golden::program().map_err(|_|JsValue::from_str("unreadable compiled plan"))?;
  let inner=WorkersHost::new(env,context);
  let values=jadpo_runtime::config::load(&program.runtime().config,program.validators(),&jadpo_runtime::config::HostVariables(&inner),jadpo_runtime::config::Mode::Environment,&|bytes|inner.random(bytes)).map_err(|_|JsValue::from_str("fixture configuration invalid"))?;
  let program=program.with_config_on(&values,jadpo_runtime::http::address::Target::Workers);
  let rates=program.runtime().routes.rate_limits.clone().map(Rc::new);
  let started=program.started().ok_or_else(||JsValue::from_str("no started program"))?;
  let drivers=jadpo_drivers::factory::Drivers::build(&program.runtime().connections,&started,&inner.supplies());
  let inner=inner.with_drivers(drivers).with_rate_limits(rates);
  let database=inner.database("main").ok_or_else(||JsValue::from_str("no fixture database"))?;
  let state=STATE.with(Rc::clone);
  let record=Rc::new(jadpo_runtime::record::Record::new([7;32]));let begins=Rc::new(Cell::new(0));
  let host=Controlled{inner:&inner,database:Counted{inner:jadpo_runtime::record::Recorded::new(Borrowed(database),record.clone()),state:state.clone(),begins:begins.clone()},record:record.clone()};
  let result=match command["op"].as_str().ok_or_else(||JsValue::from_str("missing operation"))?{
   "mutation_hook"=>{*state.hook.borrow_mut()=Some((command["match"].as_str().unwrap().into(),command["sql"].as_str().unwrap().into()));json!({"set":true})}
   "worker_reset"=>{state.deferred.borrow_mut().clear();*state.worker.borrow_mut()=Rc::default();state.outage.set(false);json!({"reset":true})}
   "outage"=>{state.outage.set(command["database"].as_bool().unwrap_or(false));json!({"set":true})}
   "request"=>{
    let mut headers:Vec<(String,String)>=command["headers"].as_array().map(|hs|hs.iter().map(|h|(h[0].as_str().unwrap().into(),h[1].as_str().unwrap().into())).collect()).unwrap_or_default();
    headers.push(("cf-connecting-ip".into(),"127.0.0.1".into()));
    let full=command["path"].as_str().unwrap();let(path,query)=full.split_once('?').unwrap_or((full,""));
    let request=jadpo_runtime::http::Request{method:command["method"].as_str().unwrap().into(),path:path.into(),query:if query.is_empty(){None}else{Some(query.into())},headers,body:command["body"].as_str().unwrap_or("").as_bytes().to_vec(),address:None,too_large:false};
    let worker=state.worker.borrow().clone();
    let response=jadpo_runtime::http::serve_request(&program.app(),&program,&host,&worker,&request).await;
    state.deferred.borrow_mut().extend(response.deferred);
    json!({"status":response.status,"headers":response.headers,"body":String::from_utf8(response.body).unwrap()})
   }
   "drain"=>{let work=std::mem::take(&mut*state.deferred.borrow_mut());let outcomes=jadpo_runtime::runner::run_deferred(&program,&host,work).await;json!({"outcomes":format!("{outcomes:?}")})}
   "deliver_one"=>{
    let mut work=std::mem::take(&mut*state.deferred.borrow_mut());
    let delivery=match work.remove(0){jadpo_runtime::runner::Deferred::Deliveries(mut deliveries)=>{let first=deliveries.remove(0);if!deliveries.is_empty(){state.deferred.borrow_mut().push(jadpo_runtime::runner::Deferred::Deliveries(deliveries));}first},_=>return Err(JsValue::from_str("delivery expected"))};
    state.deferred.borrow_mut().extend(work);
    let mut ctx=jadpo_runtime::Ctx::of(&program,&host,jadpo_runtime::Env{seed:None,clock:jadpo_runtime::Clock::fixed(host.now_ms().unwrap())});
    let outcome=jadpo_runtime::runner::deliver(&program,&mut ctx,delivery).await;state.deferred.borrow_mut().extend(ctx.take_deferred());json!({"outcome":format!("{outcome:?}")})
   }
   "schedule"=>{
    let schedule=program.runtime().schedules.schedules.iter().find(|s|Some(s.name.as_str())==command["name"].as_str()).ok_or_else(||JsValue::from_str("schedule missing"))?;
    let callable=match schedule.run{jadpo_std::schedule::ScheduleRun::Action(n)=>n,_=>return Err(JsValue::from_str("generated purge uses tick"))};
    let mut ctx=jadpo_runtime::Ctx::of(&program,&host,jadpo_runtime::Env{seed:None,clock:jadpo_runtime::Clock::fixed(host.now_ms().unwrap())});
    let outcome=jadpo_runtime::runner::as_system(&program,&mut ctx,jadpo_runtime::runner::Job{callable,args:vec![],system:jadpo_runtime::System::default()}).await;state.deferred.borrow_mut().extend(ctx.take_deferred());json!({"outcome":format!("{outcome:?}")})
   }
   "sweep"=>{let outcomes=jadpo_runtime::runner::sweep(&program,&host,"main",host.now_ms().unwrap()).await.map_err(|_|JsValue::from_str("sweep failed"))?;json!({"outcomes":format!("{outcomes:?}")})}
   "tick"=>{let tick=jadpo_runtime::schedules::tick(&program,&host,host.now_ms().unwrap()).await;json!({"tick":format!("{tick:?}")})}
   "concurrent_tick"=>{
    use std::task::Poll;
    let mut a=Box::pin(jadpo_runtime::schedules::tick(&program,&host,host.now_ms().unwrap()));let mut b=Box::pin(jadpo_runtime::schedules::tick(&program,&host,host.now_ms().unwrap()));let(mut first,mut second)=(None,None);
    std::future::poll_fn(|cx|{if first.is_none(){if let Poll::Ready(v)=a.as_mut().poll(cx){first=Some(v)}}if second.is_none(){if let Poll::Ready(v)=b.as_mut().poll(cx){second=Some(v)}}if first.is_some()&&second.is_some(){Poll::Ready(())}else{Poll::Pending}}).await;
    json!({"first":format!("{first:?}"),"second":format!("{second:?}")})
   }
   other=>return Err(JsValue::from_str(&format!("unsupported workerd control: {other}"))),
  };
  let statements:Vec<String>=record.entries().iter().filter_map(|entry|match entry{jadpo_runtime::record::Entry::Statement{sql,..}=>Some(sql.clone()),jadpo_runtime::record::Entry::Ended{committed,..}=>Some(if *committed{"COMMIT"}else{"ROLLBACK"}.into()),_=>None}).collect();
  Ok(JsValue::from_str(&json!({"result":result,"statements":statements,"transaction_begins":begins.get()}).to_string()))
 }).into()
}
