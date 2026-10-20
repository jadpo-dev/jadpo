//! What an invocation reaches outside itself, through its host (chunk
//! 7.0b): its stores, its passkey ceremonies and its random bytes, with
//! the identities it makes from them and what a recording keeps of them.
//!
//! The capability seam (chunk 7.0c) is built here: each call that leaves
//! the process logged on its invocation, the record recordings, `jadpo
//! calls` and an `outcome_unknown` result read, and the fakes a test
//! gives in the host's place.

use crate::{Database, Host, Passkeys};

/// What happens to the random bytes an invocation is given (area 17).
enum Tape {
    Off,
    /// Kept, in the order they were given.
    Recording(Vec<u8>),
    /// A recorded run's, given back from `at`.
    Replaying {
        bytes: Vec<u8>,
        at: usize,
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
    /// The random bytes a recorded run keeps, or a replayed run gives back
    /// (area 17, "Replaying a production request").
    tape: Tape,
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
        if let Tape::Replaying { bytes, at } = &mut self.tape {
            let Some(next) = at
                .checked_add(out.len())
                .and_then(|end| bytes.get(*at..end))
            else {
                return false;
            };
            out.copy_from_slice(next);
            *at += out.len();
            return true;
        }
        if !self.fresh_random(out) {
            return false;
        }
        if let Tape::Recording(kept) = &mut self.tape {
            kept.extend_from_slice(out);
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

    /// Keeps the random bytes given from now on.
    pub(crate) fn record_random(&mut self) {
        self.tape = Tape::Recording(Vec::new());
    }

    /// The random bytes kept since [`Capabilities::record_random`].
    pub(crate) fn recorded_random(&self) -> Option<&[u8]> {
        match &self.tape {
            Tape::Recording(kept) => Some(kept),
            _ => None,
        }
    }

    /// Gives `bytes`, a recorded run's, as the random bytes, in order.
    pub(crate) fn replay_random(&mut self, bytes: Vec<u8>) {
        self.tape = Tape::Replaying { bytes, at: 0 };
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
