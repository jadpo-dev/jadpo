//! Catalogue inventory is syntax-aware; references alone are not execution proof.
#[allow(dead_code)]
#[path = "../build.rs"]
mod discovery;

#[test]
fn discovery_ignores_comments_and_test_only_codes_but_includes_new_families() {
    let codes = discovery::emitted_codes(
        r###"
// CONFIG_COMMENT
/// CONFIG_DOC_COMMENT
fn emit() { call("CONFIG_VALUE_MISSING"); call(r#"POLICY_SCOPE_MISSING"#); call("TEST_FAILED"); }
#[test] fn test_only() { assert_eq!("TYPE_TEST_ONLY", "x"); }
#[cfg(test)] mod tests { fn helper() { call("SYN_HELPER_ONLY"); } }
"###,
    );
    assert_eq!(
        codes.into_iter().collect::<Vec<_>>(),
        [
            "CONFIG_VALUE_MISSING",
            "POLICY_SCOPE_MISSING",
            "TEST_FAILED"
        ]
    );
}

#[test]
fn references_are_bounded_to_real_test_bodies_and_string_literals() {
    let refs = discovery::test_references(
        r##"
// #[test] fn fake() { assert!("SYN_COMMENT"); }
#[test]
fn actual() {
    // assert!("CONFIG_COMMENT");
    fn unused_helper() { assert_eq!(call(), "TYPE_NESTED_HELPER"); }
    assert_eq!(call(), "CONFIG_VALUE_MISSING");
    let _ = vec!["POLICY_SCOPE_MISSING"];
}
fn helper_outside() { assert_eq!(call(), "TYPE_HELPER"); }
#[test] fn next() { assert!(true); }
#[test] #[ignore] fn ignored() { assert_eq!(call(), "TYPE_IGNORED"); }
"##,
    );
    assert_eq!(
        refs.into_iter().collect::<Vec<_>>(),
        vec![
            ("CONFIG_VALUE_MISSING".into(), "actual".into()),
            ("POLICY_SCOPE_MISSING".into(), "actual".into()),
        ]
    );
}

#[test]
fn source_target_module_is_not_a_cargo_build_directory() {
    let crate_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(!discovery::is_build_directory(
        &crate_root.join("../core/src/target")
    ));
    assert!(discovery::is_build_directory(
        &crate_root.join("../../target")
    ));
}

#[test]
fn fixture_references_come_only_from_diagnostic_codes_regardless_of_layout() {
    let codes = discovery::fixture_codes(
        r#"{"note":{"code":"TYPE_UNRELATED"},"diagnostics":[{"code":"SYN_EXPECTED_TOKEN"},{"code":"CONFIG_VALUE_MISSING"}]}"#,
    );
    assert_eq!(
        codes.into_iter().collect::<Vec<_>>(),
        ["CONFIG_VALUE_MISSING", "SYN_EXPECTED_TOKEN"]
    );
}

#[test]
fn compound_test_cfg_is_excluded_without_hiding_platform_emitters() {
    let codes = discovery::emitted_codes(
        r#"
#[cfg(all(test, unix))] mod helper { fn emit() { call("TYPE_TEST_ONLY"); } }
#[cfg(not(not(test)))] fn double_negative() { call("TYPE_ALSO_TEST_ONLY"); }
#[cfg(any(test, unix))] fn platform() { call("TYPE_REAL_PLATFORM"); }
#[cfg(not(test))] fn production() { call("TYPE_REAL_PRODUCTION"); }
"#,
    );
    assert_eq!(
        codes.into_iter().collect::<Vec<_>>(),
        ["TYPE_REAL_PLATFORM", "TYPE_REAL_PRODUCTION"]
    );
}

#[test]
fn native_environment_bindings_do_not_become_public_diagnostics() {
    let codes = discovery::emitted_codes(
        r#"
fn start() {
    child.env("JADPO_DEPLOYMENT_PORT", "8001");
    let _ = std::env::var("JADPO_ASCII");
    let _ = std::env::var("JADPO_DEBUG_TARGET_STACKS");
    fail("JADPO_TARGET_AUTH_NOT_IMPLEMENTED");
}
"#,
    );
    assert_eq!(
        codes.into_iter().collect::<Vec<_>>(),
        ["JADPO_TARGET_AUTH_NOT_IMPLEMENTED"]
    );
}
