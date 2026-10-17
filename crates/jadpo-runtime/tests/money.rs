//! Area 19, "Money and payments" (chunk 5.5h), in the runtime: the money
//! operations give the page's values as compiled code calls them
//! (`jadpo_value::run`, which the interpreter calls too), and `split` and
//! `allocate` never lose a minor unit, over many totals, parts and ratios,
//! in currencies with 0, 2 and 3 places (`JPY`, `GBP`, `BHD`). `convert`
//! (chunk 5.5h2) applies an `ExchangeRate` read as a provider sends it,
//! only from its own currency. The interpreter's half is
//! crates/jadpo-interp/tests/money.rs and the page's `project=money` and
//! `project=money-rates` tests.

use jadpo_std::{Op, RuleKey};
use jadpo_value::{Fault, Money, Value, run};

fn call(op: Op, args: &[Value]) -> Result<Value, Fault> {
    run(op, args, None, &mut |_| None)
}

fn ok(op: Op, args: &[Value]) -> Value {
    call(op, args).unwrap_or_else(|fault| panic!("{op:?}: {fault:?}"))
}

fn ordinal(code: &str) -> u32 {
    jadpo_std::money::ordinal_of_code(code).unwrap_or_else(|| panic!("{code}"))
}

fn currency(code: &str) -> Value {
    jadpo_value::money::currency_value(ordinal(code)).unwrap()
}

fn amount(code: &str, minor: i64) -> Value {
    jadpo_value::money::value(ordinal(code), minor)
}

fn shares(value: Value) -> Vec<Money> {
    value
        .items()
        .unwrap_or_else(|| panic!("a list: {value:?}"))
        .iter()
        .map(|share| match share {
            Value::Money(money) => *money,
            other => panic!("an amount: {other:?}"),
        })
        .collect()
}

#[test]
fn the_money_operations_give_the_page_s_values() {
    let list_price = ok(Op::MoneyFromMinor, &[Value::Int(1050), currency("GBP")]);
    assert_eq!(list_price, amount("GBP", 1050));
    assert_eq!(
        ok(
            Op::MoneyOf,
            &[Value::decimal("10.50").unwrap(), currency("GBP")]
        ),
        list_price
    );
    assert_eq!(
        call(
            Op::MoneyOf,
            &[Value::decimal("10.505").unwrap(), currency("GBP")]
        ),
        Err(Fault::Broke(RuleKey::TooManyPlaces))
    );
    assert_eq!(
        ok(Op::MoneyOf, &[Value::Int(500), currency("JPY")]),
        amount("JPY", 500)
    );
    assert_eq!(
        ok(
            Op::MoneyOf,
            &[Value::decimal("1.250").unwrap(), currency("BHD")]
        ),
        amount("BHD", 1250)
    );
    assert_eq!(
        ok(Op::MoneyMinor, std::slice::from_ref(&list_price)),
        Value::Int(1050)
    );
    assert_eq!(
        ok(Op::MoneyCurrency, std::slice::from_ref(&list_price)),
        currency("GBP")
    );
    let lines = Value::list(vec![amount("GBP", 350), amount("GBP", 700)]);
    assert_eq!(
        ok(Op::MoneySum, &[lines, currency("GBP")]),
        amount("GBP", 1050)
    );
    assert_eq!(
        ok(Op::MoneySum, &[Value::list(Vec::new()), currency("EUR")]),
        amount("EUR", 0)
    );
    let mixed = Value::list(vec![amount("GBP", 350), amount("EUR", 700)]);
    assert_eq!(
        call(Op::MoneySum, &[mixed, currency("GBP")]),
        Err(Fault::Broke(RuleKey::CurrencyMismatch))
    );
    // £10.50 in three is three of £3.50; £10.00 hands its odd penny to the
    // first share.
    assert_eq!(
        shares(ok(Op::MoneySplit, &[list_price, Value::Int(3)]))
            .iter()
            .map(|share| share.minor)
            .collect::<Vec<_>>(),
        [350, 350, 350]
    );
    assert_eq!(
        shares(ok(Op::MoneySplit, &[amount("GBP", 1000), Value::Int(3)]))
            .iter()
            .map(|share| share.minor)
            .collect::<Vec<_>>(),
        [334, 333, 333]
    );
}

/// An exchange rate from its canonical text, as `text.parse` reads one.
fn rate(text: &str) -> Value {
    jadpo_value::standard::parse(text, "ExchangeRate")
        .unwrap_or_else(|fault| panic!("{text}: {fault:?}"))
}

#[test]
fn a_rate_converts_from_its_own_currency_only() {
    let cake = amount("GBP", 1050);
    let pounds_to_euros = rate("GBP/EUR 1.17 2026-10-18T09:00:00Z");
    // €12.285, half to even.
    assert_eq!(
        ok(Op::MoneyConvert, &[cake.clone(), pounds_to_euros.clone()]),
        amount("EUR", 1228)
    );
    assert_eq!(
        ok(
            Op::MoneyConvert,
            &[cake.clone(), rate("gbp/jpy 190.5 2026-10-18T09:00:00Z")]
        ),
        amount("JPY", 2000)
    );
    assert_eq!(
        call(
            Op::MoneyConvert,
            &[cake, rate("EUR/GBP 0.85 2026-10-18T09:00:00Z")]
        ),
        Err(Fault::Broke(RuleKey::CurrencyMismatch)),
        "a rate the wrong way round is never applied"
    );
    // A rate of 1 between currencies with the same places keeps every
    // minor unit, over many totals; and its JSON reads back.
    let even = rate("GBP/EUR 1 2026-10-18T09:00:00Z");
    let mut cases = Cases(0x5eed_2a7e);
    for _ in 0..500 {
        let total = cases.total();
        let Value::Money(converted) = ok(Op::MoneyConvert, &[amount("GBP", total), even.clone()])
        else {
            panic!("an amount");
        };
        assert_eq!((converted.code(), converted.minor), ("EUR", total));
    }
    assert_eq!(
        jadpo_value::standard::from(&pounds_to_euros),
        Ok(Value::text("GBP/EUR 1.17 2026-10-18T09:00:00Z"))
    );
}

/// A small, fixed generator, so every run tries the same cases.
struct Cases(u64);

impl Cases {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    /// A total: often small, sometimes zero or negative, sometimes large.
    fn total(&mut self) -> i64 {
        let size = match self.below(4) {
            0 => self.below(10),
            1 => self.below(10_000),
            2 => self.below(10_000_000_000),
            _ => self.below(jadpo_value::money::MOST_MINOR as u64 + 1),
        } as i64;
        if self.below(5) == 0 { -size } else { size }
    }
}

#[test]
fn split_never_loses_a_minor_unit() {
    let mut cases = Cases(0x5eed_5eed_5eed_5eed);
    for code in ["JPY", "GBP", "BHD"] {
        for _ in 0..3000 {
            let total = cases.total();
            let parts = 1 + cases.below(60) as i64;
            let given = amount(code, total);
            let split = shares(ok(Op::MoneySplit, &[given, Value::Int(parts)]));
            assert_eq!(split.len() as i64, parts, "{code} {total} in {parts}");
            assert!(split.iter().all(|share| share.currency == ordinal(code)));
            let sum: i128 = split.iter().map(|share| i128::from(share.minor)).sum();
            assert_eq!(sum, i128::from(total), "{code} {total} in {parts}");
            // Near-equal: no two shares differ by more than one minor unit,
            // and the larger ones come first.
            let sizes: Vec<i128> = split
                .iter()
                .map(|share| i128::from(share.minor).abs())
                .collect();
            let (low, high) = (sizes.iter().min().unwrap(), sizes.iter().max().unwrap());
            assert!(high - low <= 1, "{code} {total} in {parts}: {sizes:?}");
            assert!(sizes.windows(2).all(|pair| pair[0] >= pair[1]));
        }
    }
}

#[test]
fn allocate_never_loses_a_minor_unit() {
    let mut cases = Cases(0xa110_ca7e_a110_ca7e);
    for code in ["JPY", "GBP", "BHD"] {
        for _ in 0..3000 {
            let total = cases.total();
            let count = 1 + cases.below(8) as usize;
            let mut ratios: Vec<i64> = (0..count)
                .map(|_| match cases.below(4) {
                    0 => 0,
                    _ => cases.below(1000) as i64,
                })
                .collect();
            if ratios.iter().all(|ratio| *ratio == 0) {
                ratios[cases.below(count as u64) as usize] = 1 + cases.below(100) as i64;
            }
            let given = amount(code, total);
            let list = Value::list(ratios.iter().map(|ratio| Value::Int(*ratio)).collect());
            let allocated = shares(ok(Op::MoneyAllocate, &[given, list]));
            assert_eq!(allocated.len(), count, "{code} {total} by {ratios:?}");
            assert!(
                allocated
                    .iter()
                    .all(|share| share.currency == ordinal(code))
            );
            let sum: i128 = allocated.iter().map(|share| i128::from(share.minor)).sum();
            assert_eq!(sum, i128::from(total), "{code} {total} by {ratios:?}");
            // In proportion: each share is its exact part rounded down, or
            // one minor unit more, and a share of ratio 0 is nothing.
            let whole: i128 = ratios.iter().map(|ratio| i128::from(*ratio)).sum();
            for (share, ratio) in allocated.iter().zip(&ratios) {
                let exact = i128::from(total).abs() * i128::from(*ratio) / whole;
                let size = i128::from(share.minor).abs();
                assert!(
                    size == exact || size == exact + 1,
                    "{code} {total} by {ratios:?}: {size} for ratio {ratio}"
                );
                if *ratio == 0 {
                    assert_eq!(share.minor, 0);
                }
                if share.minor != 0 {
                    assert_eq!(share.minor < 0, total < 0, "a share has the total's sign");
                }
            }
        }
    }
}

#[test]
fn an_amount_past_two_to_the_53_is_overflow() {
    // The wire's amount is a JSON number, so it stays one a client reads
    // exactly (chunk 5.5h).
    let most = jadpo_value::money::MOST_MINOR;
    assert_eq!(
        call(Op::MoneyFromMinor, &[Value::Int(most), currency("GBP")]),
        Ok(amount("GBP", most))
    );
    assert_eq!(
        call(
            Op::MoneyFromMinor,
            &[Value::Int(-most - 1), currency("GBP")]
        ),
        Err(Fault::IntOverflow)
    );
    let both = Value::list(vec![amount("GBP", most), amount("GBP", 1)]);
    assert_eq!(
        call(Op::MoneySum, &[both, currency("GBP")]),
        Err(Fault::IntOverflow)
    );
}

#[test]
fn every_ratio_zero_is_a_defect() {
    let ratios = Value::list(vec![Value::Int(0), Value::Int(0)]);
    assert_eq!(
        call(Op::MoneyAllocate, &[amount("GBP", 100), ratios]),
        Err(Fault::DivisionByZero)
    );
}
