//! A `crypto` call's key, decided once by the plan (area 19, "Hashing and
//! cryptography"; chunk 10.3d): `key: config.<name>` is the setting's key,
//! by its number, and a key read from a field is the field's, so neither
//! executor tells a key's source by the site's node, and no number stands
//! for a key.

use jadpo_db::{Database, File, Layout, Project};
use jadpo_graph::body::{Callee, Expr, Node};
use jadpo_plan::{Decoded, KeySource, Plan};

fn plan(source: &str) -> Plan {
    let db = Database::default();
    let file = File::new(&db, "main.jadpo".to_string(), source.to_string());
    let project = Project::new(
        &db,
        vec![file],
        Vec::new(),
        Layout::Loose,
        Default::default(),
        Vec::new(),
    );
    Plan::build(&db, project)
}

/// What the plan decided for the `key:` of the one `crypto` call in the
/// callable named `name`.
fn key_of(plan: &Plan, name: &str) -> Decoded {
    let callable = plan
        .callables()
        .iter()
        .find(|callable| callable.key.name() == name)
        .unwrap_or_else(|| panic!("no callable `{name}`"));
    let key = callable
        .body
        .sites
        .iter()
        .find_map(|site| match &site.node {
            Node::Expr(Expr::Call {
                callee: Callee::Std(op),
                args,
                ..
            }) if op.operation().namespace == "crypto" => op
                .operation()
                .params
                .iter()
                .zip(args)
                .find_map(|(param, arg)| {
                    matches!(param.kind, jadpo_std::Kind::Builtin(builtin) if builtin.is_key())
                        .then_some(*arg)?
                }),
            _ => None,
        })
        .expect("a crypto call with a key");
    callable.sites[key.0 as usize].decoded.clone()
}

#[test]
fn a_key_setting_is_its_setting_s_key() {
    let plan = plan(
        "type Sealed = Bytes {}\n\nconfig {\n    region: Text\n    notes_key: EncryptionKey\n}\n\naction seal(note: Text, record: Text) -> Sealed {\n    return Sealed(crypto.encrypt(note, key: config.notes_key, context: record))\n}\n",
    );
    assert_eq!(
        key_of(&plan, "seal"),
        Decoded::Key(Some(KeySource::Setting(1)))
    );
}

#[test]
fn a_key_read_from_a_field_is_the_field_s() {
    // No field holds a key until encrypted fields arrive (the roadmap's
    // "After the first build", item 3), so the checker refuses this; the
    // plan still says where the key would come from.
    let plan = plan(
        "type Signature = Bytes {}\n\ntype Device = Object {\n    signing_key: PrivateKey\n}\n\nfunction signed(device: Device, body: Text) -> Signature {\n    return Signature(crypto.sign(body, key: device.signing_key))\n}\n",
    );
    assert_eq!(
        key_of(&plan, "signed"),
        Decoded::Key(Some(KeySource::Field {
            name: "signing_key".into()
        }))
    );
}

#[test]
fn a_key_from_anywhere_else_has_no_source() {
    let plan = plan(
        "type Mac = Bytes {}\n\nfunction mac(body: Text, secret: Text) -> Mac {\n    return Mac(crypto.hmac(body, key: secret))\n}\n",
    );
    assert_eq!(key_of(&plan, "mac"), Decoded::Key(None));
}
