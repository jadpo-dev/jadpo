//! Code that depends on which kind of caller it is (area 8, "Who is
//! calling: `current_principal`"; decided 2026-11-22): a `Principal` is
//! matched by kind, `user(member)` binding that kind's caller or `service`
//! without one, with an arm for every kind `principal { }` declares
//! and no `_`; and a `Principal.user` or `Principal.service` widens to a
//! `Principal`.

use jadpo_db::Layout;

use crate::support;

/// A project whose principal declares `kinds`, with `code` after.
fn with(kinds: &str, code: &str) -> Vec<String> {
    let text = format!(
        "principal {{\n{kinds}}}\n\nentity Member {{\n    id: Uuid\n    email: Email {{\n        unique: true\n    }}\n\n    policy: {{\n        create: [Access.system]\n    }}\n}}\n\nentity Partner {{\n    id: Uuid\n    email: Email {{\n        unique: true\n    }}\n\n    policy: {{\n        create: [Access.system]\n    }}\n}}\n\n{code}"
    );
    support::rules(Layout::Loose, &[("app.jadpo", text.as_str())])
}

const BOTH: &str = "    user: {\n        subject: Member.email\n    }\n    service: {\n        subject: Partner.email\n    }\n";
const USERS: &str = "    user: {\n        subject: Member.email\n    }\n";

const NONE: &[&str] = &[];

#[test]
fn a_caller_is_matched_by_kind_and_each_arm_reads_its_own() {
    let code = "function member_of(caller: Principal) -> Member.id? {\n    match caller {\n        user(member) -> {\n            return member.id\n        }\n        service -> {\n            return none\n        }\n    }\n}\n\nfunction any(caller: Principal.user) -> Member.id? {\n    return member_of(caller)\n}\n";
    assert_eq!(with(BOTH, code), NONE);
    // As a value, too.
    let value = "type Kind = Enum {\n    person\n    system\n}\n\nfunction kind_of(caller: Principal) -> Kind {\n    return match caller {\n        user -> Kind.person\n        service -> Kind.system\n    }\n}\n";
    assert_eq!(with(BOTH, value), NONE);
}

#[test]
fn every_declared_kind_has_an_arm_and_only_those() {
    let missing = "function member_of(caller: Principal) -> Member.id? {\n    match caller {\n        user(member) -> {\n            return member.id\n        }\n    }\n}\n";
    assert_eq!(with(BOTH, missing), ["type.match_non_exhaustive"]);
    // With users alone, one arm covers every caller.
    assert_eq!(with(USERS, missing), NONE);
    let undeclared = "function member_of(caller: Principal) -> Member.id? {\n    match caller {\n        user(member) -> {\n            return member.id\n        }\n        service(partner) -> {\n            return none\n        }\n    }\n}\n";
    assert_eq!(with(USERS, undeclared), ["type.match_pattern_type"]);
    let wildcard = "function member_of(caller: Principal) -> Member.id? {\n    match caller {\n        user(member) -> {\n            return member.id\n        }\n        _ -> {\n            return none\n        }\n    }\n}\n";
    assert_eq!(with(BOTH, wildcard), ["type.match_wildcard_closed"]);
    // A selected kind isn't matched again, and a whole caller's fields wait
    // for its kind.
    let selected = "function member_of(caller: Principal.user) -> Member.id {\n    match caller {\n        user(member) -> {\n            return member.id\n        }\n    }\n}\n";
    assert_eq!(with(BOTH, selected), ["type.match_pattern_type"]);
}
