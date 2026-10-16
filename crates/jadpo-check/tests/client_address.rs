//! Client addresses (area 10, chunk 5.1c): `application { client_address: }`
//! says where the address of the client is read and how many proxies of
//! the deployment's own to believe. The plan it becomes, and the errors
//! each wrong spelling gives.

use jadpo_db::{Database, Layout};
use jadpo_std::ClientAddressSource;
use jadpo_std::http::{ClientAddressPlan, Hops};

use crate::support;

const SETTINGS: &str = r#"type ProxyHops = Int {
    min: 1
    max: 8
}

type Spread = Int {
    min: 0
}

type Wide = ProxyHops {
    max: 4
}

config {
    proxy_hops: ProxyHops {
        default: 1
    }
    wide_hops: Wide {
        default: 2
    }
    plain_hops: Int {
        default: 1
    }
    spread: Spread {
        default: 1
    }
    label: Text {
        default: "one"
    }
}
"#;

fn source(application: &str) -> String {
    format!("{SETTINGS}\napplication {{\n{application}}}\n")
}

fn rules(application: &str) -> Vec<String> {
    support::rules(
        Layout::Loose,
        &[("main.jadpo", source(application).as_str())],
    )
}

fn plan(application: &str) -> Option<ClientAddressPlan> {
    let db = Database::default();
    let text = source(application);
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", text.as_str())]);
    jadpo_check::client_address::client_address(&db, project)
        .plan
        .clone()
}

fn declared(source: &str, hops: Option<&str>) -> String {
    let hops = hops.map_or_else(String::new, |hops| format!("        hops: {hops}\n"));
    format!("    client_address: {{\n        source: ClientAddressSource.{source}\n{hops}    }}\n")
}

const CLEAN: &[&str] = &[];

// --- what it reads ----------------------------------------------------------------

#[test]
fn the_page_example_declares_cleanly() {
    let application = declared("forwarded_for", Some("config.proxy_hops"));
    assert_eq!(rules(&application), CLEAN);
    assert_eq!(
        plan(&application),
        Some(ClientAddressPlan {
            source: ClientAddressSource::ForwardedFor,
            hops: Some(Hops::Setting("proxy_hops".into())),
        })
    );
}

#[test]
fn each_source_has_its_plan() {
    assert_eq!(
        plan(&declared("connection", None)),
        Some(ClientAddressPlan {
            source: ClientAddressSource::Connection,
            hops: None
        })
    );
    assert_eq!(
        plan(&declared("cloudflare", None)),
        Some(ClientAddressPlan {
            source: ClientAddressSource::Cloudflare,
            hops: None
        })
    );
    assert_eq!(
        plan(&declared("forwarded", Some("2"))),
        Some(ClientAddressPlan {
            source: ClientAddressSource::Forwarded,
            hops: Some(Hops::Count(2)),
        })
    );
    for application in [
        declared("connection", None),
        declared("cloudflare", None),
        declared("forwarded", Some("2")),
        declared("forwarded_for", Some("config.wide_hops")),
    ] {
        assert_eq!(rules(&application), CLEAN, "{application}");
    }
    assert_eq!(plan(""), None, "nothing declared takes the host's default");
}

// --- the source -------------------------------------------------------------------

#[test]
fn a_source_that_is_not_one_of_the_four_is_unknown_with_the_nearest() {
    let found = support::problems(
        Layout::Loose,
        &[(
            "main.jadpo",
            source(&declared("forward_for", Some("1"))).as_str(),
        )],
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains("configuration.client_address_source_unknown")
            && found[0].contains("forward_for"),
        "{found:?}"
    );
    assert_eq!(
        rules(&declared("xforwarded", Some("1"))),
        ["configuration.client_address_source_unknown"]
    );
}

#[test]
fn a_source_that_is_not_a_client_address_source_is_unknown() {
    for written in [
        "\"forwarded_for\"",
        "Driver.postgres",
        "forwarded_for",
        "ClientAddressSource",
        "42",
    ] {
        let application =
            format!("    client_address: {{\n        source: {written}\n        hops: 1\n    }}\n");
        assert_eq!(
            rules(&application),
            ["configuration.client_address_source_unknown"],
            "{written}"
        );
    }
}

#[test]
fn a_missing_source_is_the_wrong_shape() {
    assert_eq!(
        rules("    client_address: {\n        hops: 1\n    }\n"),
        ["configuration.client_address_invalid"]
    );
    assert_eq!(
        rules("    client_address: {\n    }\n"),
        ["configuration.client_address_invalid"]
    );
}

// --- the shape --------------------------------------------------------------------

#[test]
fn a_client_address_that_is_not_a_block_is_the_wrong_shape() {
    assert_eq!(
        rules("    client_address: ClientAddressSource.cloudflare\n"),
        ["configuration.client_address_invalid"]
    );
}

#[test]
fn an_option_it_does_not_take_is_the_wrong_shape_and_nothing_else() {
    assert_eq!(
        rules(
            "    client_address: {\n        source: ClientAddressSource.forwarded_for\n        hop: 1\n    }\n"
        ),
        ["configuration.client_address_invalid"],
        "the missing count is not reported on top"
    );
    assert_eq!(
        rules(
            "    client_address: {\n        source: ClientAddressSource.forwarded_for\n        hops: 1\n        trusted: true\n    }\n"
        ),
        ["configuration.client_address_invalid"]
    );
}

#[test]
fn a_second_client_address_is_reported_once() {
    let application = format!(
        "{}{}",
        declared("connection", None),
        declared("cloudflare", None)
    );
    assert_eq!(
        rules(&application),
        ["configuration.client_address_duplicate"]
    );
}

// --- hops -------------------------------------------------------------------------

#[test]
fn the_header_sources_need_a_count() {
    for name in ["forwarded_for", "forwarded"] {
        assert_eq!(
            rules(&declared(name, None)),
            ["configuration.client_address_hops_invalid"],
            "{name}"
        );
    }
}

#[test]
fn a_count_is_a_whole_number_from_one() {
    for hops in ["0", "-1", "1.5", "\"one\"", "true", "1d", "none"] {
        assert_eq!(
            rules(&declared("forwarded_for", Some(hops))),
            ["configuration.client_address_hops_invalid"],
            "{hops}"
        );
    }
    for hops in ["1", "2", "16"] {
        assert_eq!(
            rules(&declared("forwarded_for", Some(hops))),
            CLEAN,
            "{hops}"
        );
    }
}

#[test]
fn a_setting_for_the_count_is_an_int_that_cannot_be_below_one() {
    for setting in ["config.plain_hops", "config.spread", "config.label"] {
        assert_eq!(
            rules(&declared("forwarded", Some(setting))),
            ["configuration.client_address_hops_invalid"],
            "{setting}"
        );
    }
    assert_eq!(
        rules(&declared("forwarded", Some("config.wide_hops"))),
        CLEAN,
        "a type built on one with a minimum counts"
    );
}

#[test]
fn a_setting_that_does_not_exist_is_reported_where_it_is_read() {
    assert_eq!(
        rules(&declared("forwarded_for", Some("config.proxy_hop"))),
        ["configuration.field_unknown"]
    );
}

#[test]
fn a_count_means_nothing_to_a_source_that_counts_none() {
    for name in ["connection", "cloudflare"] {
        assert_eq!(
            rules(&declared(name, Some("1"))),
            ["configuration.client_address_hops_unused"],
            "{name}"
        );
        assert_eq!(
            rules(&declared(name, Some("config.proxy_hops"))),
            ["configuration.client_address_hops_unused"],
            "{name} with a setting"
        );
    }
}
