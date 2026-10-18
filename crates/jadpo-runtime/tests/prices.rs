//! Area 19, the money group's records (chunk 5.5i3), in the runtime:
//! `money.apply_discount` takes either kind of `Discount` off a `Price` as
//! compiled code calls it (`jadpo_value::run`, which the interpreter calls
//! too), and `TaxRate`'s and the parts' bounds refuse what they don't
//! hold. The interpreter's half is the page's `project=prices` tests; the
//! text, JSON, storage and format of every type are the
//! `standard_types` harness's.

use jadpo_std::standard::prices::{DISCOUNT, PRICE};
use jadpo_std::{Builtin, Op, RuleKey};
use jadpo_value::{Decimal, Fault, Validator, Value, run};

fn pounds(minor: i64) -> Value {
    jadpo_value::money::value(jadpo_std::money::ordinal_of_code("GBP").unwrap(), minor)
}

fn euros(minor: i64) -> Value {
    jadpo_value::money::value(jadpo_std::money::ordinal_of_code("EUR").unwrap(), minor)
}

fn price(amount: Value, includes_tax: bool) -> Value {
    Value::record(
        PRICE,
        vec![
            ("amount".into(), amount),
            ("includes_tax".into(), Value::Bool(includes_tax)),
        ],
    )
}

fn amount_off(amount: Value) -> Value {
    Value::variant(DISCOUNT, 0, "amount_off", vec![("amount".into(), amount)])
}

fn percentage_off(fraction: &str) -> Value {
    Value::variant(
        DISCOUNT,
        1,
        "percentage_off",
        vec![(
            "percentage".into(),
            Value::Percentage(Decimal::parse(fraction).unwrap()),
        )],
    )
}

fn discounted(price: &Value, discount: &Value) -> Result<Value, Fault> {
    run(
        Op::MoneyApplyDiscount,
        &[price.clone(), discount.clone()],
        None,
        &mut |_| None,
    )
}

#[test]
fn a_discount_comes_off_a_price_either_way() {
    let cake = price(pounds(1250), true);
    assert_eq!(
        discounted(&cake, &amount_off(pounds(500))),
        Ok(price(pounds(750), true))
    );
    assert_eq!(
        discounted(&cake, &percentage_off("0.20")),
        Ok(price(pounds(1000), true))
    );
    // A net price stays net.
    let net = price(pounds(1000), false);
    assert_eq!(
        discounted(&net, &percentage_off("0.10")),
        Ok(price(pounds(900), false))
    );
    // The percentage's share is rounded half to even to a penny before it
    // comes off: 12.5% of £1.00 is 12.5p, so 12p off.
    assert_eq!(
        discounted(&price(pounds(100), true), &percentage_off("0.125")),
        Ok(price(pounds(88), true))
    );
    // More off than the price leaves nothing.
    assert_eq!(
        discounted(&price(pounds(300), true), &amount_off(pounds(500))),
        Ok(price(pounds(0), true))
    );
    // An amount off in another currency.
    assert_eq!(
        discounted(&cake, &amount_off(euros(500))),
        Err(Fault::Broke(RuleKey::CurrencyMismatch))
    );
}

#[test]
fn bounds_refuse_what_they_don_t_hold() {
    let (of, rules) = jadpo_std::standard::within_rules("TaxRate").expect("its bounds");
    assert_eq!(of, Builtin::Percentage);
    let tax_rate = Validator::new(Some(of), vec![rules]);
    assert!(
        tax_rate
            .check(&Value::Percentage(Decimal::parse("0.20").unwrap()))
            .is_ok()
    );
    assert!(
        tax_rate
            .check(&Value::Percentage(Decimal::parse("0").unwrap()))
            .is_ok()
    );
    assert!(
        tax_rate
            .check(&Value::Percentage(Decimal::parse("-0.05").unwrap()))
            .is_err(),
        "a tax rate below 0%"
    );
    // A basis point count has no bounds.
    let (of, rules) = jadpo_std::standard::within_rules("BasisPoints").expect("its bounds");
    assert_eq!(of, Builtin::Int);
    assert!(
        Validator::new(Some(of), vec![rules])
            .check(&Value::Int(-25))
            .is_ok()
    );
    // A percentage off from 0% to 100%, an amount off never below 0.
    let fields = jadpo_std::standard::choice_variant(DISCOUNT, "percentage_off").unwrap();
    let percentage = Validator::new(Some(Builtin::Percentage), vec![fields[0].1.rules()]);
    for (fraction, holds) in [
        ("0", true),
        ("1.00", true),
        ("1.20", false),
        ("-0.05", false),
    ] {
        assert_eq!(
            percentage
                .check(&Value::Percentage(Decimal::parse(fraction).unwrap()))
                .is_ok(),
            holds,
            "{fraction}"
        );
    }
    let fields = jadpo_std::standard::choice_variant(DISCOUNT, "amount_off").unwrap();
    let amount = Validator::new(Some(Builtin::Money), vec![fields[0].1.rules()]);
    assert!(amount.check(&pounds(0)).is_ok());
    assert!(amount.check(&pounds(-1)).is_err());
}
