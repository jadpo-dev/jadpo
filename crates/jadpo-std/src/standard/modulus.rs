//! The UK modulus check across a sort code and an account number (area
//! 19, "Money and payments", chunk 5.5i4), by Vocalink's "Validating
//! account numbers: UK modulus checking", version 9.10 of 5 September 2026
//! (ref. MNIT15479): the standard modulus 10 and 11 checks and the double
//! alternate check (§1.3), each sort code's one or two checks from the
//! modulus weight table (§2.2.1), and exceptions 1 to 14 (§2.2.2).
//!
//! The weight table (`valacdos.txt`) and the sort code substitution table
//! (`scsubtab.txt`) are Vocalink's, published with the specification under
//! "© Vocalink Limited 2026. All rights reserved" and no licence to
//! redistribute them, so no release ships them (chosen in chunk 5.5i4):
//! [`check`] reads them as [`Tables`], in Vocalink's own text format
//! ([`Tables::read`]), and a sort code in no range of the weight table is
//! [`Outcome::Unchecked`], which §2.2.1 says to presume valid.

/// How one row of the weight table checks an account (§1.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    /// The standard check with a modulus of 10 (`MOD10`).
    Standard10,
    /// The standard check with a modulus of 11 (`MOD11`).
    Standard11,
    /// The double alternate check (`DBLAL`): each product's digits added.
    DoubleAlternate,
}

/// One row of the weight table: a range of sort codes, both ends included,
/// the check, a weight for each of the fourteen digits `u` to `h`, and the
/// exception, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub start: u32,
    pub end: u32,
    pub method: Method,
    pub weights: [i32; 14],
    pub exception: Option<u8>,
}

/// The weight table and the sort code substitution table (§2.2.2.5), in
/// the order Vocalink lists them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tables {
    pub rows: Vec<Row>,
    pub substitutions: Vec<(u32, u32)>,
}

/// What the check says of a sort code and an account number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The account number passes its sort code's checks: a possible
    /// account at that sort code, not necessarily one in use.
    Valid,
    /// The account number fails them.
    Invalid,
    /// No row of the weight table covers the sort code, so there is no
    /// check to make, and the account is presumed valid (§2.2.1, step 1).
    Unchecked,
}

impl Tables {
    /// The tables from the text of `valacdos.txt` and `scsubtab.txt`
    /// (§2.3.2 and §2.2.2.5): fields separated by spaces, a weight table
    /// row being its two sort codes, `MOD10`, `MOD11` or `DBLAL`, fourteen
    /// signed weights and an exception number where it has one, and a
    /// substitution row an original sort code and its substitute. Blank
    /// lines are skipped; the 1-based number of the first line of either
    /// file that is neither is the error, with which file it is in.
    pub fn read(weights: &str, substitutions: &str) -> Result<Tables, (&'static str, usize)> {
        let rows = lines(weights)
            .map(|(at, line)| row(line).ok_or(("valacdos.txt", at)))
            .collect::<Result<_, _>>()?;
        let substitutions = lines(substitutions)
            .map(|(at, line)| substitution(line).ok_or(("scsubtab.txt", at)))
            .collect::<Result<_, _>>()?;
        Ok(Tables {
            rows,
            substitutions,
        })
    }

    /// The rows for `sort_code`, in table order: none, one or two.
    fn rows_for(&self, sort_code: u32) -> Vec<&Row> {
        self.rows
            .iter()
            .filter(|row| (row.start..=row.end).contains(&sort_code))
            .collect()
    }
}

/// Each line that isn't blank, with its 1-based number.
fn lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines()
        .enumerate()
        .map(|(at, line)| (at + 1, line.trim()))
        .filter(|(_, line)| !line.is_empty())
}

/// A sort code of six digits, as a number.
fn sort_code(text: &str) -> Option<u32> {
    (text.len() == 6 && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

/// One row of `scsubtab.txt`: a sort code and its substitute.
fn substitution(line: &str) -> Option<(u32, u32)> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    match fields.as_slice() {
        [from, to] => Some((sort_code(from)?, sort_code(to)?)),
        _ => None,
    }
}

/// One row of `valacdos.txt`.
fn row(line: &str) -> Option<Row> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    let (start, end, method, rest) = match fields.as_slice() {
        [start, end, method, rest @ ..] => (start, end, method, rest),
        _ => return None,
    };
    let method = match *method {
        "MOD10" => Method::Standard10,
        "MOD11" => Method::Standard11,
        "DBLAL" => Method::DoubleAlternate,
        _ => return None,
    };
    let (weights, exception) = match rest.len() {
        14 => (rest, None),
        15 => (rest.get(..14)?, Some(rest.get(14)?.parse::<u8>().ok()?)),
        _ => return None,
    };
    let mut parsed = [0; 14];
    for (slot, written) in parsed.iter_mut().zip(weights) {
        *slot = written.parse().ok()?;
    }
    Some(Row {
        start: sort_code(start)?,
        end: sort_code(end)?,
        method,
        weights: parsed,
        exception,
    })
}

// The digits' places, as the specification names them (§1.3.2): `u` to `z`
// the sort code's, `a` to `h` the account number's.
const A: usize = 6;
const B: usize = 7;
const C: usize = 8;
const G: usize = 12;
const H: usize = 13;

/// Exception 2's weights where `a` isn't 0 and `g` isn't 9, and where `g`
/// is 9 (§2.2.2.2).
const EXCEPTION_2: [i32; 14] = [0, 0, 1, 2, 5, 3, 6, 4, 8, 7, 10, 9, 3, 1];
const EXCEPTION_2_G9: [i32; 14] = [0, 0, 0, 0, 0, 0, 0, 0, 8, 7, 10, 9, 3, 1];

/// The sort code a Lloyds euro account is held at (§2.2.2.2), and the one
/// exception 8 checks with (§2.2.2.8).
const LLOYDS_EURO: [u8; 6] = [3, 0, 9, 6, 3, 4];
const EXCEPTION_8: [u8; 6] = [0, 9, 0, 1, 2, 6];

/// The UK modulus check of `account_number` at `sort_code`, both their
/// canonical digits (six and eight), by `tables`. Text that isn't six and
/// eight digits is [`Outcome::Invalid`].
pub fn check(sort_code: &str, account_number: &str, tables: &Tables) -> Outcome {
    let (Some(code), Some(account)) = (digits::<6>(sort_code), digits::<8>(account_number)) else {
        return Outcome::Invalid;
    };
    let number = code.iter().fold(0, |n, d| n * 10 + u32::from(*d));
    let rows = tables.rows_for(number);
    let (first, second) = match rows.as_slice() {
        [] => return Outcome::Unchecked,
        [first] => (*first, None),
        [first, second, ..] => (*first, Some(*second)),
    };
    let exception = |row: &Row, n: u8| row.exception == Some(n);
    let either = |n: u8| exception(first, n) || second.is_some_and(|row| exception(row, n));
    let mut all = [0u8; 14];
    all[..6].copy_from_slice(&code);
    all[6..].copy_from_slice(&account);

    // Exception 6: an account in a foreign currency, which can't be checked
    // (§2.2.2.6).
    if either(6) && (4..=8).contains(&all[A]) && all[G] == all[H] {
        return Outcome::Valid;
    }
    // Exception 5: the sort code substituted for both checks (§2.2.2.5).
    if exception(first, 5) {
        let substitute = tables
            .substitutions
            .iter()
            .find(|(from, _)| *from == number)
            .map_or(code, |(_, to)| to_digits(*to));
        all[..6].copy_from_slice(&substitute);
    }
    let first_passes = passes(first, &all);
    let Some(second) = second else {
        // Exception 14: a Coutts account whose last digit was its own
        // (§2.2.2.11).
        if !first_passes && exception(first, 14) {
            if !matches!(all[H], 0 | 1 | 9) {
                return outcome(false);
            }
            let mut shifted = all;
            shifted[A] = 0;
            shifted[B..=H].copy_from_slice(&all[A..H]);
            return outcome(passes(first, &shifted));
        }
        return outcome(first_passes);
    };
    // Exceptions 2 and 9: Lloyds, a sterling account passing the first
    // check, a euro account at 30-96-34 the second (§2.2.2.2).
    if exception(first, 2) && exception(second, 9) {
        if first_passes {
            return Outcome::Valid;
        }
        let mut euro = all;
        euro[..6].copy_from_slice(&LLOYDS_EURO);
        return outcome(passes(second, &euro));
    }
    // Exceptions 10 and 11, 12 and 13: either check passing is enough
    // (§2.2.2.9 and §2.2.2.10).
    if (exception(first, 10) && exception(second, 11))
        || (exception(first, 12) && exception(second, 13))
    {
        return outcome(first_passes || passes(second, &all));
    }
    if !first_passes {
        return Outcome::Invalid;
    }
    // Exception 3: no double alternate check where `c` is 6 or 9
    // (§2.2.2.3).
    if exception(second, 3) && matches!(all[C], 6 | 9) {
        return Outcome::Valid;
    }
    outcome(passes(second, &all))
}

fn outcome(valid: bool) -> Outcome {
    if valid {
        Outcome::Valid
    } else {
        Outcome::Invalid
    }
}

/// `text`'s `N` digits, or `None` if it isn't exactly that.
fn digits<const N: usize>(text: &str) -> Option<[u8; N]> {
    let bytes = text.as_bytes();
    if bytes.len() != N || !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let mut out = [0; N];
    for (slot, byte) in out.iter_mut().zip(bytes) {
        *slot = byte - b'0';
    }
    Some(out)
}

/// A sort code held as a number, as its six digits.
fn to_digits(code: u32) -> [u8; 6] {
    let mut out = [0; 6];
    let mut rest = code;
    for slot in out.iter_mut().rev() {
        *slot = u8::try_from(rest % 10).unwrap_or(0);
        rest /= 10;
    }
    out
}

/// Whether the fourteen digits `all` pass `row`'s check with its
/// exception (§1.3 and §2.2.2).
fn passes(row: &Row, all: &[u8; 14]) -> bool {
    let mut all = *all;
    let mut weights = row.weights;
    let zero_to_b = |weights: &mut [i32; 14]| weights[..=B].fill(0);
    match row.exception {
        // §2.2.2.2: other weights where `a` isn't 0.
        Some(2) if all[A] != 0 => {
            weights = if all[G] == 9 {
                EXCEPTION_2_G9
            } else {
                EXCEPTION_2
            };
        }
        // §2.2.2.7: `u` to `b` left out where `g` is 9.
        Some(7) if all[G] == 9 => zero_to_b(&mut weights),
        // §2.2.2.8: checked as if at 09-01-26.
        Some(8) => all[..6].copy_from_slice(&EXCEPTION_8),
        // §2.2.2.9: `u` to `b` left out where `ab` is 09 or 99 and `g` is 9.
        Some(10) if matches!((all[A], all[B]), (0 | 9, 9)) && all[G] == 9 => {
            zero_to_b(&mut weights);
        }
        _ => {}
    }
    let products = all
        .iter()
        .zip(weights)
        .map(|(digit, factor)| i64::from(*digit) * i64::from(factor));
    let total: i64 = match row.method {
        Method::DoubleAlternate => products.map(digit_sum).sum(),
        Method::Standard10 | Method::Standard11 => products.sum(),
    };
    match (row.method, row.exception) {
        // §2.2.2.1: 27 added, for the institution number before the sort
        // code.
        (Method::DoubleAlternate, Some(1)) => (total + 27) % 10 == 0,
        // §2.2.2.5: the check digit `h`, and `g` for the standard check.
        (Method::DoubleAlternate, Some(5)) => check_digit(total, 10, all[H]),
        (Method::Standard11, Some(5)) => {
            let remainder = total.rem_euclid(11);
            remainder != 1 && check_digit(total, 11, all[G])
        }
        // §2.2.2.4: the remainder is the two digits `gh`.
        (Method::Standard11, Some(4)) => {
            total.rem_euclid(11) == i64::from(all[G]) * 10 + i64::from(all[H])
        }
        (Method::Standard10 | Method::DoubleAlternate, _) => total.rem_euclid(10) == 0,
        (Method::Standard11, _) => total.rem_euclid(11) == 0,
    }
}

/// Whether `digit` is the check digit a total gives under `modulus` with
/// exception 5: 0 where the remainder is 0, else the remainder taken from
/// the modulus.
fn check_digit(total: i64, modulus: i64, digit: u8) -> bool {
    match total.rem_euclid(modulus) {
        0 => digit == 0,
        remainder => modulus - remainder == i64::from(digit),
    }
}

/// The sum of a product's decimal digits, for the double alternate check.
fn digit_sum(product: i64) -> i64 {
    let mut rest = product.abs();
    let mut sum = 0;
    while rest > 0 {
        sum += rest % 10;
        rest /= 10;
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::{Outcome, Tables, check};

    /// A weight table made up for these tests, in `valacdos.txt`'s format:
    /// its sort code ranges and weights are invented, not Vocalink's, and
    /// each range exercises one method or exception (§1.3 and §2.2.2), its
    /// account numbers below found by the specification's rules, check by
    /// check.
    const WEIGHTS: &str = "
110000 110099 MOD10    0    0    0    0    0    0    3    1    3    1    3    1    3    1
120000 120099 MOD11    0    0    0    0    0    0    9    8    7    6    5    4    3    1
130000 130099 DBLAL    2    1    2    1    2    1    2    1    2    1    2    1    2    1
140000 140099 MOD11    0    0    0    0    0    0    9    8    7    6    5    4    3    1
140000 140099 DBLAL    2    1    2    1    2    1    2    1    2    1    2    1    2    1
150000 150099 DBLAL    0    0    0    2    1    2    1    2    1    2    1    2    1    2   1
160000 160099 MOD11    0    0    2    3    4    5    6    7    8    9    2    3    4    1   2
160000 160099 MOD11    0    0    4    7    3    6    2    9    5    8    3    4    6    1   9
170000 170099 MOD11    0    0    0    0    0    0    0    5    2    8    6    3    7    1
170000 170099 DBLAL    2    1    2    1    2    1    2    1    2    1    2    1    2    1   3
180000 180099 MOD11    0    0    0    3    8    2    7    5    9    4    6    7    0    0   4
190000 190099 MOD11    5    3    7    2    9    4    6    1    8    3    2    7    0    0   5
190000 190099 DBLAL    1    2    1    2    1    2    1    2    1    2    1    2    1    0   5
200000 200099 MOD11    0    0    0    0    0    0    4    9    2    6    8    5    3    1   6
200000 200099 DBLAL    2    1    2    1    2    1    2    1    2    1    2    1    2    1   6
210000 210099 MOD11    1    2    3    4    5    6    7    3    2    9    6    8    4    1   7
220000 220099 MOD10    4    0    2    9    6    1    3    8    5    2    7    4    9    1   8
230000 230099 MOD11    3    1    4    1    5    9    2    6    5    3    5    8    9    1  10
230000 230099 MOD11    0    0    2    7    1    8    2    8    1    8    2    8    4    1  11
240000 240099 MOD11    0    0    6    2    8    3    1    8    5    3    0    7    1   -1  12
240000 240099 MOD10    0    2    9    4    6    1    7    3    8    2    5    9    4   -1  13
250000 250099 MOD11    0    0    0    0    0    0    9    4    7    2    5    8    3    1  14
";

    /// A made-up substitution, in `scsubtab.txt`'s format, for exception 5.
    const SUBSTITUTIONS: &str = "190050 190060\n";

    /// Each case: sort code, account number, whether it is valid, and
    /// what it exercises.
    const CASES: &[(&str, &str, bool, &str)] = &[
        (
            "110010",
            "12633920",
            true,
            "passes its standard modulus 10 check",
        ),
        (
            "110010",
            "49081935",
            false,
            "fails its standard modulus 10 check",
        ),
        (
            "120010",
            "68106871",
            true,
            "passes its standard modulus 11 check",
        ),
        (
            "120010",
            "28816302",
            false,
            "fails its standard modulus 11 check",
        ),
        (
            "130010",
            "56126116",
            true,
            "passes its double alternate check",
        ),
        (
            "130010",
            "32301241",
            false,
            "fails its double alternate check",
        ),
        ("140010", "99304075", true, "passes both of its two checks"),
        (
            "140010",
            "69476293",
            false,
            "passes its first check and fails its second",
        ),
        (
            "140010",
            "37502921",
            false,
            "fails its first check and passes its second",
        ),
        (
            "150010",
            "34785794",
            true,
            "exception 1: passes with 27 added",
        ),
        (
            "150010",
            "64160468",
            false,
            "exception 1: passes only without 27 added",
        ),
        (
            "160010",
            "09814103",
            true,
            "exceptions 2 and 9: a is 0, so the table's weights pass",
        ),
        (
            "160010",
            "19190316",
            true,
            "exceptions 2 and 9: a isn't 0 and g isn't 9, exception 2's first weights pass",
        ),
        (
            "160010",
            "21849997",
            true,
            "exceptions 2 and 9: a isn't 0 and g is 9, exception 2's second weights pass",
        ),
        (
            "160010",
            "59117285",
            true,
            "exceptions 2 and 9: the first fails and the second passes at 30-96-34",
        ),
        (
            "160010",
            "83742074",
            false,
            "exceptions 2 and 9: both fail, though the second passes at the account's own sort code",
        ),
        (
            "170010",
            "83946251",
            true,
            "exception 3: c is 6 or 9, so the failing double alternate check isn't made",
        ),
        (
            "170010",
            "99440464",
            false,
            "exception 3: c isn't 6 or 9, so the failing double alternate check counts",
        ),
        (
            "180010",
            "12193908",
            true,
            "exception 4: the remainder is gh",
        ),
        (
            "180010",
            "53621481",
            false,
            "exception 4: no remainder, but gh isn't 00",
        ),
        (
            "190010",
            "70297512",
            true,
            "exception 5: both check digits right",
        ),
        (
            "190010",
            "09410200",
            true,
            "exception 5: both remainders 0, g and h 0",
        ),
        (
            "190010",
            "88489679",
            false,
            "exception 5: the first check digit right, the second wrong",
        ),
        (
            "190010",
            "66296682",
            false,
            "exception 5: the first check digit wrong, the second right",
        ),
        ("190010", "92903521", false, "exception 5: a remainder of 1"),
        (
            "190050",
            "17797951",
            true,
            "exception 5: right only at the substituted sort code",
        ),
        (
            "200010",
            "68282511",
            true,
            "exception 6: a foreign currency account, which can't be checked",
        ),
        (
            "200010",
            "37666555",
            false,
            "exception 6: a sterling account that fails",
        ),
        (
            "210010",
            "74021199",
            true,
            "exception 7: g is 9, so u to b are left out and it passes",
        ),
        (
            "210010",
            "05307809",
            false,
            "exception 7: g isn't 9, so u to b count and it fails",
        ),
        (
            "220010",
            "86638318",
            true,
            "exception 8: passes as if at 09-01-26",
        ),
        (
            "220010",
            "09442473",
            false,
            "exception 8: passes only at its own sort code",
        ),
        (
            "230010",
            "33159615",
            true,
            "exceptions 10 and 11: the first passes, the second fails",
        ),
        (
            "230010",
            "91612549",
            true,
            "exceptions 10 and 11: the first fails, the second passes",
        ),
        (
            "230010",
            "53198298",
            false,
            "exceptions 10 and 11: both fail",
        ),
        (
            "230010",
            "09899095",
            true,
            "exception 10: ab is 09 and g is 9, so u to b are left out",
        ),
        (
            "230010",
            "99581591",
            true,
            "exception 10: ab is 99 and g is 9, so u to b are left out",
        ),
        (
            "230010",
            "19752092",
            false,
            "exception 10: ab is 19, so u to b count",
        ),
        (
            "240010",
            "00385302",
            true,
            "exceptions 12 and 13: the modulus 11 check passes, the modulus 10 fails",
        ),
        (
            "240010",
            "04678076",
            true,
            "exceptions 12 and 13: the modulus 11 check fails, the modulus 10 passes",
        ),
        (
            "240010",
            "62283819",
            false,
            "exceptions 12 and 13: both fail",
        ),
        (
            "250010",
            "08435817",
            true,
            "exception 14: the first check passes",
        ),
        (
            "250010",
            "35553110",
            true,
            "exception 14: the first fails and the shifted number passes",
        ),
        (
            "250010",
            "08697858",
            false,
            "exception 14: h isn't 0, 1 or 9",
        ),
        (
            "250010",
            "53159561",
            false,
            "exception 14: the shifted number fails too",
        ),
        (
            "999999",
            "12345678",
            true,
            "unchecked: in no range, so presumed valid",
        ),
    ];

    fn tables() -> Tables {
        Tables::read(WEIGHTS, SUBSTITUTIONS).expect("the made-up tables read")
    }

    #[test]
    fn every_method_and_exception_gives_its_result() {
        let tables = tables();
        for (sort_code, account, valid, what) in CASES {
            let given = check(sort_code, account, &tables);
            assert_eq!(
                given != Outcome::Invalid,
                *valid,
                "{sort_code} {account}, {what}: {given:?}"
            );
        }
    }

    #[test]
    fn a_sort_code_in_no_range_is_unchecked() {
        assert_eq!(check("999999", "12345678", &tables()), Outcome::Unchecked);
        assert_eq!(
            check("110010", "12633920", &Tables::default()),
            Outcome::Unchecked
        );
    }

    #[test]
    fn text_that_isn_t_six_and_eight_digits_is_invalid() {
        let tables = tables();
        assert_eq!(check("11001", "12633920", &tables), Outcome::Invalid);
        assert_eq!(check("110010", "1263392A", &tables), Outcome::Invalid);
    }

    #[test]
    fn a_line_neither_table_reads_is_named() {
        assert_eq!(
            Tables::read("110000 110099 MOD12 0", ""),
            Err(("valacdos.txt", 1))
        );
        assert_eq!(
            Tables::read("", "\n190050 19006\n"),
            Err(("scsubtab.txt", 2))
        );
    }

    /// Chapter 3's test cases (§3.1 of Vocalink's specification 9.10), by
    /// number: sort code, account number and whether it is valid.
    const VOCALINK_CASES: [(u8, &str, &str, bool); 34] = [
        (1, "089999", "66374958", true),
        (2, "107999", "88837491", true),
        (3, "202959", "63748472", true),
        (4, "871427", "46238510", true),
        (5, "872427", "46238510", true),
        (6, "871427", "09123496", true),
        (7, "871427", "99123496", true),
        (8, "820000", "73688637", true),
        (9, "827999", "73988638", true),
        (10, "827101", "28748352", true),
        (11, "134020", "63849203", true),
        (12, "118765", "64371389", true),
        (13, "200915", "41011166", true),
        (14, "938611", "07806039", true),
        (15, "938600", "42368003", true),
        (16, "938063", "55065200", true),
        (17, "772798", "99345694", true),
        (18, "086090", "06774744", true),
        (19, "309070", "02355688", true),
        (20, "309070", "12345668", true),
        (21, "309070", "12345677", true),
        (22, "309070", "99345694", true),
        (23, "938063", "15764273", false),
        (24, "938063", "15764264", false),
        (25, "938063", "15763217", false),
        (26, "118765", "64371388", false),
        (27, "203099", "66831036", false),
        (28, "203099", "58716970", false),
        (29, "089999", "66374959", false),
        (30, "107999", "88837493", false),
        (31, "074456", "12345112", true),
        (32, "070116", "34012583", true),
        (33, "074456", "11104102", true),
        (34, "180002", "00000190", true),
    ];

    /// The 34 cases against Vocalink's own tables, which aren't in this
    /// repository: set `JADPO_VOCALINK_TABLES` to a folder holding the
    /// `valacdos.txt` and `scsubtab.txt` published at
    /// <https://www.vocalink.com/tools/modulus-checking/> to run them.
    #[test]
    fn every_vocalink_test_case_gives_its_expected_result() {
        let Some(folder) = std::env::var_os("JADPO_VOCALINK_TABLES") else {
            eprintln!(
                "skipped: set JADPO_VOCALINK_TABLES to a folder holding Vocalink's valacdos.txt and scsubtab.txt to run its 34 test cases"
            );
            return;
        };
        let folder = std::path::PathBuf::from(folder);
        let read = |name: &str| {
            std::fs::read_to_string(folder.join(name))
                .unwrap_or_else(|problem| panic!("{name} in JADPO_VOCALINK_TABLES: {problem}"))
        };
        let tables = Tables::read(&read("valacdos.txt"), &read("scsubtab.txt"))
            .unwrap_or_else(|(file, line)| panic!("{file} line {line} doesn't read"));
        for (number, sort_code, account, valid) in VOCALINK_CASES {
            let expected = if valid {
                Outcome::Valid
            } else {
                Outcome::Invalid
            };
            assert_eq!(
                check(sort_code, account, &tables),
                expected,
                "Vocalink test case {number}: {sort_code} {account}"
            );
        }
    }
}
