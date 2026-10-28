//! The `random` namespace (area 19, "Randomness"): tokens,
//! codes, whole numbers, shuffles and samples, each made from the random
//! bytes it is given.
//!
//! These are the only operations that aren't pure. Each takes the
//! invocation's [`Source`] of random bytes and nothing else random: in a
//! deployable the host's cryptographically secure generator (the operating
//! system's, or the Web Crypto API on Workers), in a test a generator its
//! seed starts, so a rerun gives the same values, and in a replayed run the
//! bytes the recorded run was given (area 17). Nothing here reads a clock,
//! a counter or the platform, so the same bytes give the same values on
//! every target.
//!
//! Every value is uniform: a code's character is five bits of a byte, and a
//! whole number below a bound is drawn by rejection, never by a remainder
//! that would favour the low numbers.

use crate::{Fault, Op, Value, encoding};

/// Where random bytes come from: it fills the slice and gives `true`, or
/// gives `false` when it has none to give, which is a fault, never a
/// weaker source.
pub type Source<'a> = dyn FnMut(&mut [u8]) -> bool + 'a;

/// How many random bytes a token holds: 256 bits, as area 8's session
/// tokens.
pub const TOKEN_BYTES: usize = 32;

/// Crockford's Base32 alphabet: the digits and the capitals without `I`,
/// `L`, `O` and `U`, so a code read out over the phone isn't misheard.
pub const CODE_ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// The shortest and the longest code, which the checker proves a call's
/// `length` is within.
pub const CODE_LENGTHS: (i64, i64) = (4, 32);

fn fill(source: &mut Source<'_>, bytes: &mut [u8]) -> Result<(), Fault> {
    if source(bytes) {
        Ok(())
    } else {
        Err(Fault::NoRandomness)
    }
}

/// 64 random bits.
fn draw(source: &mut Source<'_>) -> Result<u64, Fault> {
    let mut bytes = [0u8; 8];
    fill(source, &mut bytes)?;
    Ok(u64::from_be_bytes(bytes))
}

/// A whole number from 0 to `bound - 1`, each as likely: a draw at or past
/// the largest multiple of `bound` that 64 bits hold is drawn again, so the
/// remainder favours none. `bound` is at least 1.
fn below(bound: u64, source: &mut Source<'_>) -> Result<u64, Fault> {
    if bound == 0 {
        return Err(Fault::Shape);
    }
    // 2^64 mod bound, worked out in 64 bits: the draws below it are the
    // ones a remainder would count once too often.
    let skip = bound.wrapping_neg() % bound;
    loop {
        let value = draw(source)?;
        if value >= skip {
            return Ok(value % bound);
        }
    }
}

/// `random.token()`: 32 random bytes as Base64url without padding, 43
/// characters.
pub fn token(source: &mut Source<'_>) -> Result<String, Fault> {
    let mut bytes = [0u8; TOKEN_BYTES];
    fill(source, &mut bytes)?;
    Ok(encoding::to_base64url(&bytes))
}

/// `random.code(length)`: `length` characters of Crockford's Base32, each
/// from five bits of its own byte, so each of the 32 is as likely.
pub fn code(length: i64, source: &mut Source<'_>) -> Result<String, Fault> {
    if !(CODE_LENGTHS.0..=CODE_LENGTHS.1).contains(&length) {
        return Err(Fault::Shape);
    }
    let mut bytes = vec![0u8; usize::try_from(length).map_err(|_| Fault::Shape)?];
    fill(source, &mut bytes)?;
    bytes
        .iter()
        .map(|byte| {
            CODE_ALPHABET
                .get(usize::from(byte & 0x1f))
                .map(|letter| char::from(*letter))
                .ok_or(Fault::Shape)
        })
        .collect()
}

/// `random.integer(min, max)`: a whole number from `min` to `max`, both
/// included, each as likely. A `min` above `max` is a defect.
pub fn integer(min: i64, max: i64, source: &mut Source<'_>) -> Result<i64, Fault> {
    if min > max {
        return Err(Fault::EmptyRange);
    }
    // How many whole numbers there are, less one, fits in 64 bits
    // unsigned; every one of the 2^64 is a draw itself.
    let span = max.abs_diff(min);
    let offset = match span.checked_add(1) {
        Some(count) => below(count, source)?,
        None => draw(source)?,
    };
    Ok(min.wrapping_add_unsigned(offset))
}

/// `random.shuffle(items)`: the items in random order, by Fisher and
/// Yates's shuffle, every order as likely.
pub fn shuffle(items: &[Value], source: &mut Source<'_>) -> Result<Vec<Value>, Fault> {
    let mut items = items.to_vec();
    for last in (1..items.len()).rev() {
        let pick = below(last as u64 + 1, source)?;
        items.swap(last, usize::try_from(pick).map_err(|_| Fault::Shape)?);
    }
    Ok(items)
}

/// `random.sample(items, count)`: `count` items from different positions,
/// in random order, by the first `count` steps of Fisher and Yates's
/// shuffle; all of them, shuffled, when `count` is larger. A negative
/// `count` is a defect the checker rules out.
pub fn sample(items: &[Value], count: i64, source: &mut Source<'_>) -> Result<Vec<Value>, Fault> {
    let count = usize::try_from(count).map_err(|_| Fault::Shape)?;
    let mut items = items.to_vec();
    let taken = count.min(items.len());
    for next in 0..taken {
        let left = (items.len() - next) as u64;
        let pick = next + usize::try_from(below(left, source)?).map_err(|_| Fault::Shape)?;
        items.swap(next, pick);
    }
    items.truncate(taken);
    Ok(items)
}

/// Runs one `random` operation with the values of its parameters in table
/// order and the invocation's source of random bytes. Any other operation
/// is [`Fault::Shape`]: [`crate::run`] runs those.
pub fn run(op: Op, args: &[Value], source: &mut Source<'_>) -> Result<Value, Fault> {
    match (op, args) {
        (Op::RandomToken, []) => token(source).map(|text| Value::text(&text)),
        (Op::RandomCode, [Value::Int(length)]) => {
            code(*length, source).map(|text| Value::text(&text))
        }
        (Op::RandomInteger, [Value::Int(min), Value::Int(max)]) => {
            integer(*min, *max, source).map(Value::Int)
        }
        (Op::RandomShuffle, [Value::List(items)]) => shuffle(items, source).map(Value::list),
        (Op::RandomSample, [Value::List(items), Value::Int(count)]) => {
            sample(items, *count, source).map(Value::list)
        }
        _ => Err(Fault::Shape),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole numbers a list holds, for comparing a shuffle with its
    /// list.
    fn ints(values: &[Value]) -> Vec<i64> {
        values
            .iter()
            .map(|value| match value {
                Value::Int(n) => *n,
                _ => panic!("not an Int"),
            })
            .collect()
    }

    /// A source that gives these bytes in turn, then none.
    fn tape(bytes: &[u8]) -> impl FnMut(&mut [u8]) -> bool + '_ {
        let mut at = 0;
        move |out: &mut [u8]| {
            let Some(next) = bytes.get(at..at + out.len()) else {
                return false;
            };
            out.copy_from_slice(next);
            at += out.len();
            true
        }
    }

    /// A deterministic source for the statistical checks: SplitMix64.
    fn counter(seed: u64) -> impl FnMut(&mut [u8]) -> bool {
        let mut state = seed;
        move |out: &mut [u8]| {
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
            true
        }
    }

    #[test]
    fn a_token_is_32_bytes_as_unpadded_base64url() {
        let bytes: Vec<u8> = (0..32).collect();
        let token = token(&mut tape(&bytes)).expect("a token");
        assert_eq!(token.len(), 43);
        assert_eq!(token, "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8");
        assert_eq!(encoding::from_base64url(&token), Some(bytes));
    }

    #[test]
    fn a_code_is_crockford_base32_from_five_bits_a_byte() {
        // 0, 31, 32 (0 again), 255 (31), 17 (`H`), 18 (`J`: no `I`).
        let made = code(6, &mut tape(&[0, 31, 32, 255, 17, 18])).expect("a code");
        assert_eq!(made, "0Z0ZHJ");
        for letter in ["I", "L", "O", "U"] {
            assert!(
                !std::str::from_utf8(CODE_ALPHABET)
                    .unwrap_or("")
                    .contains(letter)
            );
        }
        assert_eq!(code(3, &mut counter(1)), Err(Fault::Shape));
        assert_eq!(code(33, &mut counter(1)), Err(Fault::Shape));
        assert_eq!(code(32, &mut counter(1)).map(|code| code.len()), Ok(32));
    }

    #[test]
    fn a_whole_number_is_within_its_bounds_and_each_is_as_likely() {
        let mut source = counter(7);
        let mut seen = [0u32; 6];
        for _ in 0..60_000 {
            let n = integer(1, 6, &mut source).expect("a number");
            assert!((1..=6).contains(&n));
            seen[(n - 1) as usize] += 1;
        }
        // Each face within 5% of its 10,000.
        assert!(
            seen.iter().all(|count| (9_500..=10_500).contains(count)),
            "{seen:?}"
        );
        assert_eq!(integer(4, 4, &mut source), Ok(4));
        assert_eq!(integer(5, 4, &mut source), Err(Fault::EmptyRange));
        // 3 is past the one draw below 3 that a remainder would favour.
        assert_eq!(integer(-3, -1, &mut tape(&3u64.to_be_bytes())), Ok(-3));
    }

    #[test]
    fn the_whole_range_of_int_is_drawn_without_overflow() {
        assert_eq!(
            integer(i64::MIN, i64::MAX, &mut tape(&[0; 8])),
            Ok(i64::MIN)
        );
        assert_eq!(
            integer(i64::MIN, i64::MAX, &mut tape(&[0xff; 8])),
            Ok(i64::MAX)
        );
        assert_eq!(
            integer(i64::MAX - 1, i64::MAX, &mut tape(&[0, 0, 0, 0, 0, 0, 0, 1])),
            Ok(i64::MAX)
        );
    }

    #[test]
    fn a_draw_a_remainder_would_favour_is_drawn_again() {
        // Below 3, a draw under 2^64 mod 3 (1) is rejected: 0 is, and the
        // next draw, 5, gives 2.
        let mut bytes = vec![0u8; 8];
        bytes.extend_from_slice(&5u64.to_be_bytes());
        assert_eq!(below(3, &mut tape(&bytes)), Ok(2));
    }

    #[test]
    fn a_shuffle_keeps_every_item_and_every_order_comes_up() {
        let items: Vec<Value> = (1..=4).map(Value::Int).collect();
        let mut source = counter(3);
        let mut orders = std::collections::BTreeSet::new();
        for _ in 0..2_000 {
            let shuffled = ints(&shuffle(&items, &mut source).expect("shuffled"));
            let mut sorted = shuffled.clone();
            sorted.sort_unstable();
            assert_eq!(sorted, [1, 2, 3, 4]);
            orders.insert(shuffled);
        }
        assert_eq!(orders.len(), 24, "all 4! orders");
        assert_eq!(shuffle(&[], &mut source), Ok(Vec::new()));
    }

    #[test]
    fn a_sample_takes_different_positions_and_at_most_every_item() {
        let items: Vec<Value> = (1..=10).map(Value::Int).collect();
        let mut source = counter(5);
        let sample_of = |count, source: &mut dyn FnMut(&mut [u8]) -> bool| {
            ints(&sample(&items, count, source).expect("sampled"))
        };
        let three = sample_of(3, &mut source);
        assert_eq!(three.len(), 3);
        let distinct: std::collections::BTreeSet<_> = three.iter().collect();
        assert_eq!(distinct.len(), 3);
        let all = sample_of(25, &mut source);
        let mut sorted = all.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (1..=10).collect::<Vec<_>>());
        assert!(sample_of(0, &mut source).is_empty());
    }

    #[test]
    fn no_bytes_is_a_fault_never_a_weaker_value() {
        let mut empty = |_: &mut [u8]| false;
        assert_eq!(token(&mut empty), Err(Fault::NoRandomness));
        assert_eq!(integer(1, 6, &mut empty), Err(Fault::NoRandomness));
        assert_eq!(
            run(
                Op::RandomShuffle,
                &[Value::list(vec![Value::Int(1), Value::Int(2)])],
                &mut empty
            ),
            Err(Fault::NoRandomness)
        );
        // A one-item shuffle draws nothing.
        assert_eq!(
            run(
                Op::RandomShuffle,
                &[Value::list(vec![Value::Int(1)])],
                &mut empty
            ),
            Ok(Value::list(vec![Value::Int(1)]))
        );
    }

    #[test]
    fn the_same_bytes_give_the_same_values() {
        let items = Value::list((1..=8).map(Value::Int).collect());
        let mut once = counter(11);
        let mut again = counter(11);
        for op in [
            Op::RandomToken,
            Op::RandomCode,
            Op::RandomInteger,
            Op::RandomShuffle,
            Op::RandomSample,
        ] {
            let args: Vec<Value> = match op {
                Op::RandomCode => vec![Value::Int(8)],
                Op::RandomInteger => vec![Value::Int(1), Value::Int(100)],
                Op::RandomShuffle => vec![items.clone()],
                Op::RandomSample => vec![items.clone(), Value::Int(3)],
                _ => Vec::new(),
            };
            assert_eq!(
                run(op, &args, &mut once),
                run(op, &args, &mut again),
                "{op:?}"
            );
        }
    }
}
