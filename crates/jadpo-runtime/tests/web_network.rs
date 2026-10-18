//! Area 19, "Web and software" (chunk 5.5n), in the runtime: the values the
//! page's test (`project=web-and-network`) and the interpreter's
//! `web_network` tests assert, given to `web`'s operations as compiled code
//! calls them (`jadpo_value::run`): RFC 3986's resolution with its own
//! examples (§5.4), a query added in key order, and a range holding only
//! addresses of its own family.

use std::collections::BTreeMap;
use std::rc::Rc;

use jadpo_std::Op;
use jadpo_value::{Value, run};

fn call(op: Op, args: &[Value]) -> Value {
    run(op, args, None, &mut |_| None).unwrap_or_else(|fault| panic!("{op:?}: {fault:?}"))
}

fn resolved(base: &str, relative: &str) -> Value {
    call(Op::WebResolve, &[Value::text(base), Value::text(relative)])
}

#[test]
fn references_resolve_by_rfc_3986() {
    // §5.4.1's normal examples and §5.4.2's abnormal ones that are
    // relative references.
    let base = "http://a/b/c/d;p?q";
    for (relative, expected) in [
        ("g", "http://a/b/c/g"),
        ("./g", "http://a/b/c/g"),
        ("g/", "http://a/b/c/g/"),
        ("/g", "http://a/g"),
        ("//g", "http://g"),
        ("?y", "http://a/b/c/d;p?y"),
        ("#s", "http://a/b/c/d;p?q#s"),
        ("g;x?y#s", "http://a/b/c/g;x?y#s"),
        ("..", "http://a/b/"),
        ("../../g", "http://a/g"),
        ("../../../g", "http://a/g"),
        ("/./g", "http://a/g"),
        ("g/../h", "http://a/b/c/h"),
        ("g?y/../x", "http://a/b/c/g?y/../x"),
    ] {
        assert_eq!(resolved(base, relative), Value::text(expected), "{relative}");
    }
    assert_eq!(
        resolved("https://shop.example.com/teas/green/sencha", "../black/assam"),
        Value::text("https://shop.example.com/teas/black/assam")
    );
    assert_eq!(
        resolved("https://shop.example.com/teas/green", "/basket"),
        Value::text("https://shop.example.com/basket")
    );
}

#[test]
fn a_query_is_added_in_key_order() {
    let entries: BTreeMap<Value, Value> = [
        (Value::text("q"), Value::text("tea & cake")),
        (Value::text("page"), Value::text("1")),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        call(
            Op::WebWithQuery,
            &[
                Value::text("https://shop.example.com/search#results"),
                Value::Map(Rc::new(entries)),
            ]
        ),
        Value::text("https://shop.example.com/search?page=1&q=tea%20%26%20cake#results")
    );
}

#[test]
fn a_range_holds_addresses_of_its_own_family() {
    let inside = |range: &str, address: &str| {
        call(
            Op::WebCidrContains,
            &[Value::text(range), Value::text(address)],
        ) == Value::Bool(true)
    };
    assert!(inside("192.0.2.0/24", "192.0.2.77"));
    assert!(!inside("192.0.2.0/24", "198.51.100.1"));
    assert!(inside("2001:db8::/32", "2001:db8:1::5"));
    assert!(!inside("::/0", "192.0.2.77"));
    assert!(inside("0.0.0.0/0", "203.0.113.9"));
}
