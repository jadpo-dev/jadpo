//! Area 19, "Web and software" (chunk 5.5n), run: the page's example
//! (`project=web-and-network`), a shop's links resolved by RFC 3986 and
//! given a query, a caller's address checked against a range, a domain
//! name read by UTS 46's rules, a status's class, and a problem shown to
//! people and sent as RFC 9457 sends one.

use crate::support;

use jadpo_interp::Value;
use support::{failure, int, main, text, value};

const SOURCE: &str = "type ShopLink = Url {}

type SearchTerm = Text {
    min_length: 1
    max_length: 100
}

type CallerAddress = IpAddress {}

type FormText = Text {}

type YesNo = Bool {}

type NotADomain = Failure {
    kind: FailureKind.invalid_value
}

/// A search page's link for a term, on its first page.
function search_link(catalogue_page: ShopLink, tea_term: SearchTerm) -> ShopLink {
    var none_yet: Map<Text, Text> = []
    var tea_query: Map<Text, Text> = map.with(map.with(none_yet, \"q\", tea_term), \"page\", \"1\")
    return ShopLink(web.with_query(catalogue_page, tea_query))
}

/// Where a link on `page` leads.
function link_target(page: ShopLink, page_link: RelativeUrl) -> ShopLink {
    return ShopLink(web.resolve(page, page_link))
}

/// The basket's address from any page: a path is a relative URL too.
function basket_from(page: ShopLink, basket: UrlPath) -> ShopLink {
    return ShopLink(web.resolve(page, basket))
}

/// Whether a caller is on the office's network.
function from_office(office: Cidr, caller: CallerAddress) -> YesNo {
    return YesNo(web.cidr_contains(office, caller))
}

/// The domain a seller typed for their shop.
function shop_domain(raw: FormText) fails NotADomain -> DomainName {
    var typed_domain: DomainName = attempt text.parse(raw, as: DomainName) {
        malformed: NotADomain
    }
    return typed_domain
}

/// Whether a failed call is worth trying again: the server's fault, not
/// the request's.
function worth_retrying(status: HttpStatus) -> YesNo {
    return YesNo(status >= 500)
}

/// The problem a sold-out item is reported with.
function sold_out(item_page: ShopLink) -> ProblemDetails {
    return ProblemDetails {
        type_: \"https://tea.example/problems/sold-out\"
        status: 409
        title: \"Sold out\"
        detail: \"Nothing is left of this item.\"
        instance: item_page
    }
}

/// A problem as a person reads it.
function problem_shown(problem: ProblemDetails, customer_locale: Locale) -> PresentationText {
    return text.format(problem, locale: customer_locale)
}

/// A problem as it is sent.
function problem_sent(problem: ProblemDetails) -> FormText {
    return FormText(json.write(problem))
}
";

fn locale(tag: &str) -> Value {
    let ordinal = jadpo_std::variant_ordinal(jadpo_std::LOCALE, tag).unwrap();
    Value::variant(jadpo_std::LOCALE, ordinal, tag, Vec::new())
}

#[test]
fn links_follow_rfc_3986() {
    let program = main(SOURCE).with_proof_checks();
    assert_eq!(
        value(
            &program,
            "search_link",
            vec![text("https://tea.example/search"), text("tea & cake")]
        ),
        text("https://tea.example/search?page=1&q=tea%20%26%20cake"),
        "in key order, percent-encoded"
    );
    assert_eq!(
        value(
            &program,
            "link_target",
            vec![
                text("https://tea.example/teas/green/sencha"),
                text("../black/assam")
            ]
        ),
        text("https://tea.example/teas/black/assam")
    );
    assert_eq!(
        value(
            &program,
            "link_target",
            vec![text("https://tea.example/teas?page=2"), text("#reviews")]
        ),
        text("https://tea.example/teas?page=2#reviews")
    );
    assert_eq!(
        value(
            &program,
            "basket_from",
            vec![text("https://tea.example/teas/green"), text("/basket")]
        ),
        text("https://tea.example/basket")
    );
}

#[test]
fn an_address_is_inside_a_range_of_its_own_family() {
    let program = main(SOURCE).with_proof_checks();
    let inside = |office: &str, caller: &str| {
        value(&program, "from_office", vec![text(office), text(caller)]) == Value::Bool(true)
    };
    assert!(inside("192.0.2.0/24", "192.0.2.77"));
    assert!(!inside("192.0.2.0/24", "198.51.100.1"));
    assert!(inside("2001:db8::/32", "2001:db8:1::5"));
    assert!(!inside("::/0", "192.0.2.77"), "never across families");
}

#[test]
fn a_domain_name_is_read_in_its_ascii_form_only() {
    let program = main(SOURCE).with_proof_checks();
    assert_eq!(
        value(&program, "shop_domain", vec![text("xn--mnchen-3ya.de")]),
        text("xn--mnchen-3ya.de")
    );
    for typed in ["München.de", "Example.com", "example.com.", "exa_mple.com"] {
        assert_eq!(
            failure(&program, "shop_domain", vec![text(typed)]).name,
            "NotADomain",
            "{typed}"
        );
    }
}

#[test]
fn a_status_s_class_is_a_comparison() {
    let program = main(SOURCE).with_proof_checks();
    assert_eq!(
        value(&program, "worth_retrying", vec![int(503)]),
        Value::Bool(true)
    );
    assert_eq!(
        value(&program, "worth_retrying", vec![int(404)]),
        Value::Bool(false)
    );
}

#[test]
fn a_problem_is_shown_and_sent_as_rfc_9457_says() {
    let program = main(SOURCE).with_proof_checks();
    let problem = value(
        &program,
        "sold_out",
        vec![text("https://tea.example/items/assam")],
    );
    assert_eq!(
        value(
            &program,
            "problem_shown",
            vec![problem.clone(), locale("en_gb")]
        ),
        text("Sold out\nNothing is left of this item.")
    );
    assert_eq!(
        value(&program, "problem_sent", vec![problem]),
        text(
            "{\"type\":\"https://tea.example/problems/sold-out\",\"status\":409,\"title\":\"Sold out\",\"detail\":\"Nothing is left of this item.\",\"instance\":\"https://tea.example/items/assam\"}"
        )
    );
}
