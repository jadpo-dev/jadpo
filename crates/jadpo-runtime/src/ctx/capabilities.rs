//! What an invocation reaches outside itself, through its host (chunk
//! 7.0b): its stores, its passkey ceremonies and its random bytes, with
//! the identities it makes from them and what a recording keeps of them.
//!
//! The capability seam's state is here too (chunk 7.0c): the log of each
//! call that left the process ([`Logged`]), which `jadpo calls` lists and
//! from which the runner knows a run may have acted outside, and the
//! calls' answers a recording keeps and a replay gives back.

use crate::capability::{Answer, Call, Ended, Logged, Recorded, Recording, Source};
use crate::{Database, Host, Passkeys};

/// What happens to the effects an invocation is given (area 17).
enum Tape {
    Off,
    /// Kept, in the order they were given.
    Recording(Recording),
    /// A recorded run's, given back in order: its random bytes from
    /// `random`, its calls' answers from `calls`.
    Replaying {
        recording: Recording,
        random: usize,
        calls: usize,
    },
}

/// One invocation's capabilities: its host, and the state of what it draws
/// from the host's randomness.
pub struct Capabilities<'p> {
    host: &'p dyn Host,
    /// The seed a test's randomness comes from (area 13); `None` for the
    /// host's.
    seed: Option<u64>,
    /// How many identities it has made.
    ids: u64,
    /// How many random values it has made from its seed.
    draws: u64,
    /// How many trace and span ids it has worked out without the host's
    /// randomness.
    spans: u64,
    /// The random bytes and calls' answers a recorded run keeps, or a
    /// replayed run gives back (area 17, "Replaying a production request").
    tape: Tape,
    /// Each call that left the process, or was refused, in order.
    calls: Vec<Logged>,
}

impl std::fmt::Debug for Capabilities<'_> {
    /// Never the bytes a recording keeps: they can hold tokens.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let tape = match self.tape {
            Tape::Off => "off",
            Tape::Recording(_) => "recording",
            Tape::Replaying { .. } => "replaying",
        };
        f.debug_struct("Capabilities")
            .field("seeded", &self.seed.is_some())
            .field("ids", &self.ids)
            .field("tape", &tape)
            .field("calls", &self.calls)
            .finish_non_exhaustive()
    }
}

impl<'p> Capabilities<'p> {
    pub(crate) fn new(host: &'p dyn Host, seed: Option<u64>) -> Capabilities<'p> {
        Capabilities {
            host,
            seed,
            ids: 0,
            draws: 0,
            spans: 0,
            tape: Tape::Off,
            calls: Vec::new(),
        }
    }

    pub fn host(&self) -> &'p dyn Host {
        self.host
    }

    /// The store called `store`, when the host has it.
    pub fn database(&self, store: &str) -> Option<&'p dyn Database> {
        self.host.database(store)
    }

    /// The host's passkey ceremonies, when it has them (area 8).
    pub fn passkeys(&self) -> Option<&'p dyn Passkeys> {
        self.host.passkeys()
    }

    /// Bytes for a new trace or span id at `now`, `chain` events deep: the
    /// host's, or worked out from the seed and the clock, without drawing
    /// on what `random` gives, so tracing never changes a test's values
    /// (area 13).
    pub(crate) fn trace_bytes(&mut self, now: i64, chain: u32, out: &mut [u8]) {
        if self.host.random(out) {
            return;
        }
        let mut state = self
            .seed
            .unwrap_or(0)
            .wrapping_add(now as u64)
            .wrapping_add(u64::from(chain).wrapping_mul(0x2545_F491_4F6C_DD1D))
            .wrapping_add(self.spans.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        self.spans += 1;
        for chunk in out.chunks_mut(8) {
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^= z >> 31;
            for (to, from) in chunk.iter_mut().zip(z.to_be_bytes()) {
                *to = from;
            }
        }
    }

    /// Fills `out` with random bytes, as [`crate::Ctx::random_bytes`]
    /// gives them.
    pub(crate) fn random_bytes(&mut self, out: &mut [u8]) -> bool {
        if let Tape::Replaying {
            recording, random, ..
        } = &mut self.tape
        {
            let Some(next) = random
                .checked_add(out.len())
                .and_then(|end| recording.random.get(*random..end))
            else {
                return false;
            };
            out.copy_from_slice(next);
            *random += out.len();
            return true;
        }
        if !self.fresh_random(out) {
            return false;
        }
        if let Tape::Recording(kept) = &mut self.tape {
            kept.random.extend_from_slice(out);
        }
        true
    }

    /// Fills `out` with random bytes a credential or a nonce is made of,
    /// as [`crate::Ctx::secret_random_bytes`] gives them: never kept on a
    /// recording's tape nor taken from a replayed one.
    pub(crate) fn secret_random_bytes(&mut self, out: &mut [u8]) -> bool {
        self.fresh_random(out)
    }

    /// Random bytes no recording gave: the host's, or the seed's.
    fn fresh_random(&mut self, out: &mut [u8]) -> bool {
        if self.host.random(out) {
            return true;
        }
        let Some(seed) = self.seed else {
            return false;
        };
        for chunk in out.chunks_mut(8) {
            self.draws += 1;
            let mut z = seed
                .wrapping_add(self.draws.wrapping_mul(0xD1B5_4A32_D192_ED03))
                .wrapping_add(0x9E37_79B9_7F4A_7C15);
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^= z >> 31;
            let bytes = z.to_be_bytes();
            for (to, from) in chunk.iter_mut().zip(bytes) {
                *to = from;
            }
        }
        true
    }

    /// Keeps the random bytes given and the calls' answers from now on.
    pub(crate) fn record(&mut self) {
        self.tape = Tape::Recording(Recording::default());
    }

    /// What was kept since [`Capabilities::record`].
    pub(crate) fn recorded(&self) -> Option<&Recording> {
        match &self.tape {
            Tape::Recording(kept) => Some(kept),
            _ => None,
        }
    }

    /// Gives `recording`'s random bytes and calls' answers, in order.
    pub(crate) fn replay(&mut self, recording: Recording) {
        self.tape = Tape::Replaying {
            recording,
            random: 0,
            calls: 0,
        };
    }

    // --- the capability seam's log (chunk 7.0c) ---------------------------

    /// The calls that left the process, or were refused, in order.
    pub fn calls(&self) -> &[Logged] {
        &self.calls
    }

    /// Takes the log of calls, leaving it empty.
    pub(crate) fn take_calls(&mut self) -> Vec<Logged> {
        std::mem::take(&mut self.calls)
    }

    /// Whether a call logged from `since` on may have acted outside in a
    /// way sending it again would repeat ([`Logged::may_have_acted`]).
    pub fn may_have_acted(&self, since: usize) -> bool {
        self.calls
            .get(since..)
            .is_some_and(|calls| calls.iter().any(Logged::may_have_acted))
    }

    /// Logs `call` at `at` as going out with an answer from `source`,
    /// giving its place in the log, to say how it ended with
    /// [`Capabilities::ended`].
    pub(crate) fn sent(&mut self, call: &Call, source: Source, at: i64) -> usize {
        self.calls.push(Logged {
            connection: call.connection.clone(),
            operation: call.operation.clone(),
            repeat: call.repeat,
            source,
            at,
            took_ms: None,
            ended: Ended::InFlight,
        });
        self.calls.len() - 1
    }

    /// Logs that the call at `place` was answered `answer` after `took_ms`,
    /// and keeps the answer while recording.
    pub(crate) fn ended(&mut self, place: usize, answer: &Answer, took_ms: Option<u64>) {
        let Some(logged) = self.calls.get_mut(place) else {
            return;
        };
        logged.ended = Ended::of(answer);
        logged.took_ms = took_ms;
        if let Tape::Recording(kept) = &mut self.tape {
            kept.calls.push(Recorded {
                connection: logged.connection.clone(),
                operation: logged.operation.clone(),
                answer: answer.clone(),
            });
        }
    }

    /// Logs `call` at `at` as refused before it went out.
    pub(crate) fn refused(&mut self, call: &Call, at: i64) {
        let place = self.sent(call, Source::Live, at);
        if let Some(logged) = self.calls.get_mut(place) {
            logged.ended = Ended::Refused;
        }
    }

    /// In a replayed run, the answer the recording kept for its next call,
    /// once it is `call`'s; `Err` with what the recording has there when
    /// it isn't (`nothing` once it has run out). `None` when not
    /// replaying.
    pub(crate) fn replayed(&mut self, call: &Call) -> Option<Result<Answer, String>> {
        let Tape::Replaying {
            recording, calls, ..
        } = &mut self.tape
        else {
            return None;
        };
        let Some(next) = recording.calls.get(*calls) else {
            return Some(Err("nothing".to_string()));
        };
        if next.connection != call.connection || next.operation != call.operation {
            return Some(Err(format!("`{}.{}`", next.connection, next.operation)));
        }
        *calls += 1;
        Some(Ok(next.answer.clone()))
    }

    /// A new identity made at `now`, as [`crate::Ctx::new_id`] makes it.
    pub(crate) fn new_id(&mut self, now: i64) -> Option<[u8; 16]> {
        self.ids += 1;
        let (a, b) = match self.seed {
            Some(seed) => {
                let mut state = seed ^ self.ids.wrapping_mul(0x9E37_79B9_7F4A_7C15);
                let mut next = || {
                    state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
                    let mut z = state;
                    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
                    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
                    z ^ (z >> 31)
                };
                (next(), next())
            }
            None => return random_id(self.host, now),
        };
        Some(uuid_v7(now, a, b))
    }
}

/// A new identity made at `now` from `host`'s randomness, outside any
/// context: the run an operator asks a schedule's tick for (chunk 6.2d).
/// None when the host can't give random bytes.
pub(crate) fn random_id(host: &dyn Host, now: i64) -> Option<[u8; 16]> {
    let mut random = [0u8; 16];
    if !host.random(&mut random) {
        return None;
    }
    let [a @ .., _, _, _, _, _, _, _, _] = random;
    let [_, _, _, _, _, _, _, _, b @ ..] = random;
    Some(uuid_v7(now, u64::from_be_bytes(a), u64::from_be_bytes(b)))
}

/// A version 7 UUID made at `millis` (milliseconds since the epoch), its
/// 12 bits of `rand_a` from `a` and 62 of `rand_b` from `b` (RFC 9562).
fn uuid_v7(millis: i64, a: u64, b: u64) -> [u8; 16] {
    let millis = u64::try_from(millis).unwrap_or(0) & 0xFFFF_FFFF_FFFF;
    let [_, _, m0, m1, m2, m3, m4, m5] = millis.to_be_bytes();
    let [a0, a1] = ((a & 0x0FFF) as u16 | 0x7000).to_be_bytes();
    let [b0, b1, b2, b3, b4, b5, b6, b7] = b.to_be_bytes();
    [
        m0,
        m1,
        m2,
        m3,
        m4,
        m5,
        a0,
        a1,
        (b0 & 0x3F) | 0x80,
        b1,
        b2,
        b3,
        b4,
        b5,
        b6,
        b7,
    ]
}
