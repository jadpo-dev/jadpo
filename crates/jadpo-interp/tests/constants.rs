//! Area 3, "Constants" (chunk 5.3e): a constant's value, worked out and
//! checked while compiling, read by the bodies that use it: beside its
//! declaration, through a library, built from other constants, and as an
//! object or a list.

use crate::support::{decimal, int, list, main, value};

const SOURCE: &str = r#"type Pence = Int {
    min: 0
}

type VatRate = Decimal {
    scale: 4
    min: 0.0000
    max: 1.0000
}

type VatBand = Object {
    starts: Pence
    rate: VatRate
}

type VatBands = List<VatBand> {
    min_length: 1
}

constant free_delivery_from: Pence = 5000
constant small_order_fee: Pence = 250
constant double_fee: Pence = Pence(small_order_fee * 2)

library vat {
    constant standard: VatRate = 0.2000

    constant bands: VatBands = [
        VatBand {
            starts: 0
            rate: 0.1750
        },
        VatBand {
            starts: 100
            rate: standard
        }
    ]

    function latest() -> VatRate {
        var mut last: VatRate = standard
        for band in bands {
            last = band.rate
        }
        return last
    }
}

function delivery_fee(subtotal: Pence) -> Pence {
    if (subtotal >= free_delivery_from) {
        return 0
    }
    return small_order_fee
}

function doubled() -> Pence {
    return double_fee
}

function rate() -> VatRate {
    return vat.latest()
}

function fees() -> List<Pence> {
    return [small_order_fee, double_fee]
}
"#;

#[test]
fn a_constant_beside_its_use_is_read_as_its_value() {
    let program = main(SOURCE);
    assert_eq!(value(&program, "delivery_fee", vec![int(100)]), int(250));
    assert_eq!(value(&program, "delivery_fee", vec![int(6000)]), int(0));
}

#[test]
fn a_constant_built_from_another_is_worked_out() {
    let program = main(SOURCE);
    assert_eq!(value(&program, "doubled", Vec::new()), int(500));
    assert_eq!(
        value(&program, "fees", Vec::new()),
        list(vec![int(250), int(500)])
    );
}

#[test]
fn a_library_reads_its_own_constants_bare() {
    let program = main(SOURCE);
    assert_eq!(value(&program, "rate", Vec::new()), decimal("0.2000"));
}
