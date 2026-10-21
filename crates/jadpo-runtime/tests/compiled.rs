//! What compiled code's sites call (roadmap chunk 8.1f;
//! docs/compiler-runtime.md §2.5, "Compiled code"): arithmetic checked,
//! faulting at its operator's site with the frames the interpreter gives;
//! a broken rule that leaves a callable a defect at its entry; and an
//! assertion's stop carrying the parts it recorded.

use jadpo_runtime::compiled::{self as rt, Stop};
use jadpo_runtime::operators::Operator;
use jadpo_runtime::{AssertionDetail, Ctx, Env, FaultKind, Frame, MemoryHost, Places};
use jadpo_value::{Rounding, Value};

fn places() -> Places {
    Places::new()
}

#[test]
fn arithmetic_faults_at_its_operators_site() {
    let (places, host) = (places(), MemoryHost::new());
    let mut cx = Ctx::new(&places, &host, Env::default());
    cx.enter(3, Frame::ENTRY).expect("a frame");
    let added = rt::binary(
        &mut cx,
        Operator::Add,
        &Value::Int(i64::MAX),
        &Value::Int(1),
        Rounding::HalfEven,
        7,
    );
    let Err(Stop::Fault(fault)) = added else {
        panic!("{added:?}");
    };
    assert_eq!(fault.kind, FaultKind::IntOverflow);
    assert_eq!(
        fault.frames,
        [Frame {
            callable: 3,
            site: 7
        }]
    );
    assert!(matches!(
        rt::binary(
            &mut cx,
            Operator::Less,
            &Value::Int(1),
            &Value::Int(2),
            Rounding::HalfEven,
            8
        ),
        Ok(Value::Bool(true))
    ));
}

#[test]
fn a_broken_rule_leaving_a_callable_is_a_defect_at_its_entry() {
    let (places, host) = (places(), MemoryHost::new());
    let mut cx = Ctx::new(&places, &host, Env::default());
    cx.enter(2, Frame::ENTRY).expect("a frame");
    cx.enter(5, 4).expect("a frame");
    let left = rt::left(
        &mut cx,
        Err(Stop::Invalid {
            rule: jadpo_std::RuleKey::Max,
            written: "max: 3".into(),
            problem: None,
            name: None,
        }),
    );
    let Err(Stop::Fault(fault)) = left else {
        panic!("{left:?}");
    };
    assert!(matches!(fault.kind, FaultKind::Defect(_)));
    assert_eq!(
        fault.frames,
        [
            Frame {
                callable: 5,
                site: Frame::ENTRY
            },
            Frame {
                callable: 2,
                site: 4
            }
        ]
    );
    // The callee's frame is left.
    assert_eq!(cx.frames().len(), 1);
}

#[test]
fn a_failed_comparison_is_an_assertion_with_both_sides() {
    let (places, host) = (places(), MemoryHost::new());
    let mut cx = Ctx::new(&places, &host, Env::default());
    cx.enter(1, Frame::ENTRY).expect("a frame");
    cx.assertion_begin();
    let held = rt::assert_compare(&mut cx, Operator::Equal, Value::Int(2), Value::Int(3), 5, 4);
    let ended = rt::assertion_end(&mut cx, held);
    let Err(Stop::Assertion(assertion)) = ended else {
        panic!("{ended:?}");
    };
    assert_eq!(
        assertion.detail,
        AssertionDetail::False {
            comparison: Some(("==", Value::Int(2), Value::Int(3)))
        }
    );
    assert_eq!(
        assertion.frames,
        [Frame {
            callable: 1,
            site: 4
        }]
    );
}
