//! Area 19, "Web and software" (chunk 5.5n), in the checker: `web`'s three
//! operations typed by their stubs, a `UrlPath` passed where a
//! `RelativeUrl` is taken and a `DomainName` where a `Hostname` is (never
//! the other way), the web types' literals checked while compiling by
//! their standards (RFC 3986, UTS 46, RFC 9110), `HttpStatus` and
//! `NetworkPort` as whole numbers with their ranges, and `ProblemDetails`'
//! reserved member written `type_`.

use crate::support;

use jadpo_db::Layout;

const NAMES: &str = "type ShopLink = Url {}\n\ntype CallerAddress = IpAddress {}\n\ntype Wire = Text {}\n\ntype Answer = Bool {}\n\ntype Unreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

fn rules(source: &str) -> Vec<String> {
    support::rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{NAMES}{source}"))],
    )
}

const NONE: &[&str] = &[];

#[test]
fn web_s_operations_take_what_their_stubs_say() {
    assert_eq!(
        rules(
            "function linked(page: ShopLink, link: RelativeUrl) -> ShopLink {\n    return ShopLink(web.resolve(page, link))\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function linked(page: ShopLink, path: UrlPath) -> ShopLink {\n    return ShopLink(web.resolve(page, path))\n}\n"
        ),
        NONE,
        "a path is a relative URL"
    );
    assert_eq!(
        rules(
            "function linked(page: ShopLink, raw: Wire) -> ShopLink {\n    return ShopLink(web.resolve(page, raw))\n}\n"
        ),
        ["type.mismatch"],
        "text is checked as a relative URL first"
    );
    assert_eq!(
        rules(
            "function inside(office: Cidr, caller: CallerAddress) -> Answer {\n    return Answer(web.cidr_contains(office, caller))\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function searched(page: ShopLink, term: Wire) -> ShopLink {\n    var none_yet: Map<Text, Text> = []\n    return ShopLink(web.with_query(page, map.with(none_yet, \"q\", term)))\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function searched(page: ShopLink) -> ShopLink {\n    return ShopLink(web.with_query(page, [\"q\"]))\n}\n"
        ),
        ["type.mismatch"],
        "a query is a map"
    );
}

#[test]
fn a_narrower_form_passes_for_the_wider_never_the_other_way() {
    assert_eq!(
        rules("function host(name: DomainName) -> Hostname {\n    return name\n}\n"),
        NONE,
        "every domain name is a host"
    );
    assert_eq!(
        rules("function name(host: Hostname) -> DomainName {\n    return host\n}\n"),
        ["type.mismatch"],
        "a host may be an address"
    );
    assert_eq!(
        rules("function path(link: RelativeUrl) -> UrlPath {\n    return link\n}\n"),
        ["type.mismatch"]
    );
}

#[test]
fn literals_are_checked_by_their_standards_while_compiling() {
    let literal = |ty: &str, text: &str| {
        rules(&format!(
            "function made() -> {ty} {{\n    return {ty}(\"{text}\")\n}}\n"
        ))
    };
    assert_eq!(literal("RelativeUrl", "../logo.png"), NONE);
    assert_eq!(
        literal("RelativeUrl", "https://example.com/"),
        ["type.invalid_literal"],
        "a scheme makes it a URL (RFC 3986 §4.2)"
    );
    assert_eq!(literal("UrlPath", "/products/42"), NONE);
    assert_eq!(literal("UrlPath", "/search?q=tea"), ["type.invalid_literal"]);
    assert_eq!(literal("DomainName", "xn--mnchen-3ya.de"), NONE);
    assert_eq!(
        literal("DomainName", "München.de"),
        ["type.invalid_literal"],
        "only UTS 46's A-labels in lower case are read"
    );
    assert_eq!(literal("HttpHeaderName", "content-type"), NONE);
    assert_eq!(literal("HttpHeaderName", "Content-Type"), ["type.invalid_literal"]);
    assert_eq!(literal("Cidr", "192.0.2.0/24"), NONE);
    assert_eq!(literal("Cidr", "192.0.2.1/24"), ["type.invalid_literal"]);
    assert_eq!(literal("WebOrigin", "https://example.com"), NONE);
    assert_eq!(literal("WebOrigin", "https://example.com:443"), ["type.invalid_literal"]);
    assert_eq!(literal("Emoji", "\u{2764}\u{FE0F}"), NONE);
    assert_eq!(literal("Emoji", "\u{2764}"), ["type.invalid_literal"]);
    assert_eq!(literal("MacAddress", "AC-DE-48-00-00-80"), NONE);
    assert_eq!(literal("Imei", "490154203237519"), ["type.invalid_literal"]);
}

#[test]
fn a_status_and_a_port_are_whole_numbers_in_their_ranges() {
    assert_eq!(
        rules("function failed(status: HttpStatus) -> Answer {\n    return Answer(status >= 500)\n}\n"),
        NONE,
        "a status is ordered, so a class is a comparison"
    );
    assert_eq!(
        rules("function teapot() -> HttpStatus {\n    return 418\n}\n"),
        NONE
    );
    assert_eq!(
        rules("function made() -> HttpStatus {\n    return HttpStatus(600)\n}\n"),
        ["type.invalid_literal"]
    );
    assert_eq!(
        rules("function made() -> NetworkPort {\n    return NetworkPort(0)\n}\n"),
        ["type.invalid_literal"],
        "port 0 is reserved"
    );
    // Read from text: digits within the range or `malformed`; a type with
    // a narrower range breaks it by name.
    assert_eq!(
        rules(
            "function read(raw: Wire) fails Unreadable -> HttpStatus {\n    var status: HttpStatus = attempt text.parse(raw, as: HttpStatus) {\n        malformed: Unreadable\n    }\n    return status\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "type HighPort = NetworkPort {\n    min: 1024\n}\n\nfunction read(raw: Wire) fails Unreadable -> HighPort {\n    var port: HighPort = attempt text.parse(raw, as: HighPort) {\n        malformed: Unreadable\n    }\n    return port\n}\n"
        ),
        ["failure.rule_unmapped"],
        "`min` isn't mapped"
    );
    assert_eq!(
        rules(
            "type HighPort = NetworkPort {\n    min: 1024\n}\n\nfunction read(raw: Wire) fails Unreadable -> HighPort {\n    var port: HighPort = attempt text.parse(raw, as: HighPort) {\n        malformed: Unreadable\n        min: Unreadable\n    }\n    return port\n}\n"
        ),
        NONE
    );
}

#[test]
fn a_problem_s_type_is_written_type_() {
    let problem = |members: &str| {
        rules(&format!(
            "function problem() -> ProblemDetails {{\n    return ProblemDetails {{\n{members}    }}\n}}\n"
        ))
    };
    let all = "        type_: \"https://example.com/probs/out-of-credit\"\n        status: 403\n        title: \"You do not have enough credit.\"\n        detail: \"Your balance is 30.\"\n        instance: none\n";
    assert_eq!(problem(all), NONE);
    assert_eq!(
        problem(&all.replace("403", "600")),
        ["type.invalid_literal"],
        "a status is from 100 to 599"
    );
    assert_eq!(
        problem(&all.replace("\"https://example.com/probs/out-of-credit\"", "\"/probs/out-of-credit\"")),
        ["type.invalid_literal"],
        "a problem's type is a URL"
    );
    assert_eq!(
        rules(
            "function status(problem: ProblemDetails) -> Answer {\n    return Answer(problem.status == 404 and problem.type_ == none)\n}\n"
        ),
        NONE
    );
    // `text.from` has no single text form to give.
    assert_eq!(
        rules(
            "function written(problem: ProblemDetails) -> Wire {\n    return Wire(text.from(problem))\n}\n"
        ),
        ["type.mismatch"]
    );
}
