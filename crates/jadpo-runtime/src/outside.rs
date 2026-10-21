//! One outside-call path (docs/compiler-runtime.md §2.6, "One outside-call
//! path"; chunk 7.9d, from the Phase 7 architecture review). Every kind of
//! call that leaves the process (a provider's operation, `ai.generate`,
//! `ai.embed`, `ai.stream`, a store's file, an email, a capability's
//! delivery) is decided and sent here, so each kind only builds its
//! request and decodes its answer:
//!
//! - [`comes_from`] says where its answer comes from before anything is
//!   sent: given back by the action's record of its steps (area 12, "The
//!   record"), or by the answers an `ai` call's earlier run kept for area
//!   6's rerun after a concurrent change, or sent, at its place in them;
//!   [`given_back`] logs one given back, and [`keep`] keeps an `ai` call's
//!   answer where it came to be given back from.
//! - [`send`] sends it, and again while that is safe and its [`Retry`]
//!   allows (area 11, question 8; area 21, "Reading an answer"): a call
//!   that never left or that the provider said did nothing, honouring its
//!   `Retry-After`, and one that may have acted only when it changes
//!   nothing or takes its key. A wait never runs past what is left of the
//!   call's window nor of the invocation's time (a route's deadline,
//!   background work's budget): one that wouldn't end within both ends the
//!   sending, the last answer standing.
//! - [`exchange`] is one send through the capability seam, whole or as it
//!   comes: refused inside an open transaction, a replayed run's recorded
//!   answer given back from its tape (area 17), otherwise one of its
//!   connection's `max_concurrent:` slots taken, its step written, and the
//!   driver the host gives asked, the call logged as it goes out and as it
//!   is answered, the slot given back once it is.

use std::rc::Rc;

use jadpo_std::provider::{ConnectionPlan, RetryPlan};

use crate::ai::{Kept, Reused};
use crate::ai_budget::{Resend, Used};
use crate::capability::{Answer, Call, Repeat, Source};
use crate::effects::Unanswered;
use crate::steps::{Answered, Proposed, StepAt, Stepped};
use crate::stream::Streamed;
use crate::{Ctx, FaultKind, Host};

// --- where an answer comes from ---------------------------------------------

/// What a call asks of where its answer comes from, before it is sent.
pub(crate) struct Asking<'a> {
    /// What it proposes to the action's record of its steps, for a call
    /// that may be one of them; `None` for one that never is, `ai.stream`'s,
    /// whose answer the route reads only once the action has ended.
    pub step: Option<Proposed<'a>>,
    /// How its answer is kept for area 6's rerun after a concurrent change
    /// where no record keeps it: an `ai` call's, by its step among the
    /// invocation's calls and a digest of its request (chunk 5.5v3). `None`
    /// for a call a rerun sends again, under its key where it takes one.
    pub rerun: Option<Rerun>,
}

/// How a rerun is given an `ai` call's answer back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Rerun {
    /// Its answer, by the digest of its request.
    Answer([u8; 32]),
    /// The stream its first run opened and nothing has read yet, by the
    /// digest of its request (chunk 5.5v6).
    Stream([u8; 32]),
}

/// Where a call's answer comes from.
#[derive(Debug)]
pub(crate) enum Comes {
    /// It is sent, at its place.
    Send(Place),
    /// The action's record answered this step before (chunk 7.5b): given
    /// back as it was, and logged as the step `at` it is (chunk 7.5f).
    Recorded {
        answered: Answered,
        at: Option<StepAt>,
    },
    /// Its action's earlier run was given this answer.
    Reused(Kept),
    /// Its action's earlier run opened the stream of this number.
    Opened(u32),
}

/// Where a call that is sent stands among the action's outside calls.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Place {
    /// Its step of the action's record, written before it goes out.
    pub step: Option<StepAt>,
    /// Its step among the invocation's calls and its request's digest, by
    /// which a rerun is given its answer back.
    pub rerun: Option<(u32, [u8; 32])>,
}

/// Where the answer of the call `asking` describes comes from: the
/// action's record when it keeps one and this call is among its steps,
/// else an earlier run's answers for a call a rerun is given back,
/// else a send. Refused as the record refuses a step (`diverged`, a step
/// in doubt that may not go again).
pub(crate) async fn comes_from(ctx: &mut Ctx<'_>, asking: Asking<'_>) -> Result<Comes, Unanswered> {
    if let Some(proposed) = asking.step {
        match crate::steps::step(ctx, proposed).await? {
            Stepped::Untracked => {}
            Stepped::Send(at) => {
                return Ok(Comes::Send(Place {
                    step: Some(at),
                    rerun: None,
                }));
            }
            Stepped::Replay(answered) => {
                return Ok(Comes::Recorded {
                    answered,
                    at: ctx.steps_at(),
                });
            }
        }
    }
    let Some(rerun) = asking.rerun else {
        return Ok(Comes::Send(Place::default()));
    };
    let Some(step) = ctx.next_step() else {
        return Ok(Comes::Send(Place::default()));
    };
    let digest = match rerun {
        Rerun::Answer(digest) => {
            if let Some(kept) = ctx.reused(step, &digest) {
                return Ok(Comes::Reused(kept));
            }
            digest
        }
        Rerun::Stream(digest) => {
            if let Some(number) = ctx.reused_stream(step, &digest) {
                return Ok(Comes::Opened(number));
            }
            digest
        }
    };
    Ok(Comes::Send(Place {
        step: None,
        rerun: Some((step, digest)),
    }))
}

/// Logs `call` as answered `answer` without going out: given back from a
/// replayed run's tape, the action's record or an earlier run's answers.
pub(crate) fn given_back(ctx: &mut Ctx<'_>, call: &Call, answer: &Answer) {
    let now = ctx.now();
    #[cfg(feature = "testing")]
    if let Some(record) = ctx.host().record() {
        record.call(
            call,
            &format!("given back {}", crate::record::answer(answer)),
        );
    }
    let capabilities = ctx.capabilities_mut();
    let place = capabilities.sent(call, Source::Replayed, now);
    capabilities.ended(place, answer, Some(0));
}

/// Keeps what an `ai` call sent at `place` ended with, where it is given
/// back from: sealed in the action's record at its step, or for a rerun by
/// its step and digest.
pub(crate) async fn keep(ctx: &mut Ctx<'_>, place: Place, kept: Kept) -> Result<(), Unanswered> {
    if let Some(at) = place.step {
        crate::steps::answered(ctx, at, kept.answered()).await?;
    }
    if let Some((step, digest)) = place.rerun {
        ctx.keep_reused(Reused { step, digest, kept });
    }
    Ok(())
}

// --- sending ------------------------------------------------------------------

/// The form an answer is asked for in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Form {
    /// Whole, as [`crate::capability::Capability::call`] gives it.
    Whole,
    /// As it comes, `ai.stream`'s, as
    /// [`crate::capability::Capability::stream`] gives it.
    Streamed,
}

/// How often a call may be sent, and for how long.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Retry {
    /// The most sends, the first among them.
    pub sends: u32,
    /// How long its sends and the waits between them may take, from
    /// `began`.
    pub within_ms: u64,
    /// When that began on the host's monotonic clock; `None` on a host
    /// with none, as a test's, which sends again at once and measures
    /// nothing.
    pub began: Option<u64>,
    /// Whether `within_ms` is also each send's timeout, what is left of it:
    /// an `ai` call's `timeout:` (area 21). No send is made once none is
    /// left, which is `timed_out`, and one that timed out had all of it,
    /// so it isn't sent again.
    pub timed: bool,
}

impl Retry {
    /// Sent once: a store's file, an email, a capability's delivery,
    /// which have no `retry:` of their own.
    pub(crate) const ONCE: Retry = Retry {
        sends: 1,
        within_ms: 0,
        began: None,
        timed: false,
    };

    /// An `ai` call's: at most [`jadpo_std::ai::SENDS`] within its
    /// `timeout:`, `timeout_ms`, from `began`.
    pub(crate) fn timed(began: Option<u64>, timeout_ms: u64) -> Retry {
        Retry {
            sends: jadpo_std::ai::SENDS,
            within_ms: timeout_ms,
            began,
            timed: true,
        }
    }

    /// A provider operation's: its connection's `retry:`, from now on
    /// `host`'s monotonic clock.
    pub(crate) fn of(plan: RetryPlan, host: &dyn Host) -> Retry {
        Retry {
            sends: plan.attempts,
            within_ms: plan.within_ms,
            began: host.monotonic_ms(),
            timed: false,
        }
    }
}

/// What a call is sent with besides itself.
pub(crate) struct Sending<'s> {
    pub retry: Retry,
    pub form: Form,
    /// Its connection's `max_concurrent:` (area 11, "Calling"; chunk
    /// 7.6c): each send takes one of the connection's slots.
    pub max_concurrent: Option<u32>,
    /// What each send again first takes from an `ai` call's limits, made
    /// only when they cover it, the send it follows charged what it sent
    /// (chunk 7.9c). The last send is its caller's to charge.
    pub resend: Option<Resend<'s>>,
    /// Hands each send, once answered, to the development call log: an
    /// `ai` call's, which keeps each in full (area 21, "Prompts and
    /// answers in logs").
    pub log: Option<Log<'s>>,
}

/// What hands a send, once answered, to the development call log.
pub(crate) type Log<'s> = &'s dyn Fn(&Ctx<'_>, &Call, &Answer);

impl Sending<'static> {
    /// Sent once, whole, with no slot: a store's file, an email, a
    /// capability's delivery.
    pub(crate) fn once() -> Sending<'static> {
        Sending {
            retry: Retry::ONCE,
            form: Form::Whole,
            max_concurrent: None,
            resend: None,
            log: None,
        }
    }

    /// A provider operation's call through `plan`: its `retry:` from now,
    /// under its `max_concurrent:`.
    pub(crate) fn provider(plan: &ConnectionPlan, host: &dyn Host) -> Sending<'static> {
        Sending {
            retry: Retry::of(plan.retry, host),
            form: Form::Whole,
            max_concurrent: plan.max_concurrent,
            resend: None,
            log: None,
        }
    }
}

/// What a call's last send was answered, and where the answer came from.
#[derive(Debug)]
pub(crate) struct Sent {
    pub answer: Streamed,
    pub source: Source,
}

impl Sent {
    /// Its answer, asked for whole.
    pub(crate) fn whole(self) -> Answer {
        match self.answer {
            Streamed::Whole(answer) => answer,
            // Only a call asked for as it comes is answered so.
            Streamed::Coming(_) => Answer::Unknown,
        }
    }
}

/// Sends `call` through the capability seam ([`exchange`]), and again
/// while sending it again is safe and `sending`'s [`Retry`] allows: a call
/// that never left or that the provider said did nothing, whatever it
/// changes, after the wait a `Retry-After` asks for; and one that may have
/// acted only when it changes nothing or takes its key, sent again
/// unchanged. Each wait is 200 milliseconds doubled before each send after
/// the second, drawn evenly from zero ([`crate::provider::retry_wait`]),
/// or the provider's `Retry-After`; one that wouldn't end before the
/// call's window does, or before the invocation's time is up (a route's
/// deadline, background work's budget: [`Ctx::deadline_at`]), isn't
/// waited, and the last answer stands. Each send is logged as the call it
/// is. Its last answer, or why it couldn't be sent.
pub(crate) async fn send(
    ctx: &mut Ctx<'_>,
    call: &Call,
    mut sending: Sending<'_>,
) -> Result<Sent, Unanswered> {
    let host = ctx.host();
    let retry = sending.retry;
    let mut sends = 1;
    let mut unanswered: Option<Sent> = None;
    loop {
        // An `ai` call's send has what is left of its `timeout:`, and none
        // goes once none is.
        let mut timed = None;
        if retry.timed {
            let spent = match (retry.began, host.monotonic_ms()) {
                (Some(began), Some(now)) => now.saturating_sub(began),
                _ => 0,
            };
            let left = retry.within_ms.saturating_sub(spent);
            if left == 0 {
                return Ok(Sent {
                    answer: Streamed::Whole(Answer::TimedOut),
                    source: Source::Live,
                });
            }
            let mut call = call.clone();
            if let Some(request) = call.request.as_mut() {
                request.timeout_ms = left;
            }
            timed = Some(call);
        }
        let call = timed.as_ref().unwrap_or(call);
        if let Some(last) = unanswered.take()
            && let Some(resend) = sending.resend.as_mut()
        {
            if !crate::ai_budget::top_up(ctx, resend.taken, resend.input).await {
                return Ok(last);
            }
            resend.used.add(Used {
                input: resend.input,
                ..Used::default()
            });
        }
        let answer = exchange(ctx, call, sending.form, sending.max_concurrent).await?;
        let source = ctx
            .capabilities()
            .calls()
            .last()
            .map_or(Source::Live, |logged| logged.source);
        if let Some(log) = sending.log {
            match &answer {
                Streamed::Whole(answer) => log(ctx, call, answer),
                Streamed::Coming(_) => log(ctx, call, &Answer::Unknown),
            }
        }
        let asked = match &answer {
            Streamed::Whole(Answer::Unavailable) => None,
            Streamed::Whole(Answer::RetryAfter(millis)) => Some(*millis),
            Streamed::Whole(Answer::Unknown) if call.repeat != Repeat::Unkeyed => None,
            Streamed::Whole(Answer::TimedOut) if call.repeat != Repeat::Unkeyed && !retry.timed => {
                None
            }
            _ => return Ok(Sent { answer, source }),
        };
        if sends >= retry.sends {
            return Ok(Sent { answer, source });
        }
        sends += 1;
        if let Some(began) = retry.began {
            let now = host.monotonic_ms().unwrap_or(began);
            let mut left = retry.within_ms.saturating_sub(now.saturating_sub(began));
            // Never past the invocation's time (chunk 7.9d).
            if let Some(at) = ctx.deadline_at() {
                left = left.min(at.saturating_sub(now));
            }
            let mut random = [0u8; 8];
            let random = host.random(&mut random).then_some(random);
            let Some(wait) = crate::provider::retry_wait(sends - 1, asked, left, random) else {
                return Ok(Sent { answer, source });
            };
            if wait > 0 {
                host.sleep(wait).await;
            }
        }
        unanswered = Some(Sent { answer, source });
    }
}

/// Sends `call` once through the capability seam (chunk 7.0c), its answer
/// asked for in `form`: refused while a transaction that has written is open,
/// having closed one that has only read (area 6, "Outside calls hold no
/// transaction"; chunk 7.4); in a replayed run, answered from its tape
/// without going out (area 17); otherwise sent through the driver the host
/// gives for its connection, `misconfigured` with none. Under its
/// connection's `max_concurrent:` it first waits for one of the
/// connection's slots, and gives it back once answered, a stream once its
/// answer starts to come (chunk 7.6c). A step of the action's record is
/// written before it goes out (chunk 7.5b), and `jadpo test` may crash
/// once it is answered, its answer not kept yet (chunk 7.5d). It is logged
/// on the invocation as it goes out and as it is answered, never with its
/// input or its answer ([`crate::capability::Logged`]).
pub(crate) async fn exchange(
    ctx: &mut Ctx<'_>,
    call: &Call,
    form: Form,
    max_concurrent: Option<u32>,
) -> Result<Streamed, Unanswered> {
    let now = ctx.now();
    if ctx.close_snapshot().await.is_err() {
        ctx.capabilities_mut().refused(call, now);
        return Err(Unanswered::Failure(Box::new(crate::unavailable())));
    }
    if ctx.transaction_open() {
        ctx.capabilities_mut().refused(call, now);
        return Err(Unanswered::Fault(FaultKind::CallInTransaction(call.name())));
    }
    if let Some(replayed) = ctx.capabilities_mut().replayed(call) {
        return match replayed {
            Ok(answer) => {
                given_back(ctx, call, &answer);
                Ok(Streamed::Whole(answer))
            }
            Err(recorded) => {
                ctx.capabilities_mut().refused(call, now);
                Err(Unanswered::Fault(FaultKind::Defect(format!(
                    "a replayed run calling `{}` where its recording has {recorded}",
                    call.name()
                ))))
            }
        };
    }
    let host = ctx.host();
    let Some(driver) = host.capability(&call.connection) else {
        ctx.capabilities_mut().refused(call, now);
        return Err(Unanswered::Failure(Box::new(crate::misconfigured())));
    };
    let slot = match max_concurrent {
        Some(limit) => Some(host.slots().take(&call.connection, limit).await),
        None => None,
    };
    if let Some(at) = call.step
        && let Err(unsent) = crate::steps::sending(ctx, at).await
    {
        ctx.capabilities_mut().refused(call, now);
        return Err(unsent);
    }
    let began = host.monotonic_ms();
    let place = ctx
        .capabilities_mut()
        .sent(call, driver.source_of(call), now);
    let answer = match form {
        Form::Whole => Streamed::Whole(driver.call(call).await),
        Form::Streamed => driver.stream(call).await,
    };
    let took = began.and_then(|began| Some(host.monotonic_ms()?.saturating_sub(began)));
    #[cfg(feature = "testing")]
    if let Some(record) = host.record() {
        match &answer {
            Streamed::Whole(answer) => record.call(call, &crate::record::answer(answer)),
            Streamed::Coming(_) => record.call(call, "streamed"),
        }
    }
    match &answer {
        Streamed::Whole(answer) => ctx.capabilities_mut().ended(place, answer, took),
        // A stream that has started is logged as answered, never with its
        // text.
        Streamed::Coming(_) => {
            ctx.capabilities_mut()
                .ended(place, &Answer::Wire(Rc::from("")), took);
        }
    }
    drop(slot);
    // A step answered, its answer not kept yet, which `jadpo test` may
    // crash after (chunk 7.5d).
    #[cfg(feature = "testing")]
    if call.step.is_some() && crate::crashes::due(ctx, crate::crashes::Boundary::AfterSend) {
        return Err(Unanswered::Fault(FaultKind::Crashed));
    }
    Ok(answer)
}
