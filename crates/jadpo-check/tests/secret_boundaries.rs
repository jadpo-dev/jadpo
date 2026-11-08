//! Area 19, "Where a secret can go": a `Password` doesn't go out on a
//! provider's operation unless a person signs it off, and no secret leaves
//! through an endpoint (decided 2026-11-08). A route takes one only in a
//! request's body, a token also from a header; nothing it answers, sends
//! back to a webhook or streams holds one.

use crate::support;

use jadpo_db::Layout;

/// The seven secret values (area 19), each of which every rule here is
/// about unless it says otherwise.
const KINDS: [&str; 7] = [
    "Password",
    "ApiKey",
    "BearerToken",
    "TotpSecret",
    "SecretText",
    "Jwt",
    "OneTimeCode",
];

const NONE: &[&str] = &[];

// --- a password, sent to a provider ------------------------------------------

const HOME: &str = "shared/providers/acme_directory/v1";

const APP: &str = "application {\n    connections: {\n        directory: {\n            package: AcmeDirectory.v1\n            domains: [accounts]\n            credential: config.directory_key\n            environment: config.directory_environment\n        }\n    }\n}\n";

const CONFIG: &str = "config {\n    directory_key: Text {\n        secret: true\n    }\n    directory_environment: ProviderEnvironment\n}\n";

/// A provider package whose `binds.create` takes `BindRequest`, holding
/// `fields`, by `method` to `path`; with `extra` files of its own.
fn package(
    fields: &str,
    method: &str,
    path: &str,
    extra: &[(&str, &str)],
) -> Vec<(String, String)> {
    let at = |file: &str, text: String| (format!("{HOME}/{file}"), text);
    let mut out = vec![
        at(
            "package.jadpo",
            "/// Acme's directory.\npackage AcmeDirectory.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n".to_string(),
        ),
        at(
            "provider.jadpo",
            "/// How Acme's directory is reached.\nprovider {\n    protocol: Protocol.json_http\n    hosts: {\n        live: \"https://directory.acme.example\"\n        sandbox: \"https://sandbox.acme.example\"\n    }\n    credential: CredentialScheme.bearer\n    webhooks: {\n        check: SignatureScheme.twilio\n        event_id: \"CallSid\"\n        event_type: \"CallStatus\"\n    }\n}\n".to_string(),
        ),
        at(
            "types/bind_request.jadpo",
            format!("/// A sign-in to check against the directory.\npublic type BindRequest = Object {{\n{fields}}}\n"),
        ),
        at(
            "types/bind_result.jadpo",
            "/// What the directory says of it.\npublic type BindResult = Object {\n    bound: Bool\n}\n".to_string(),
        ),
        at(
            "resources/binds.jadpo",
            format!("public resource binds {{\n    create: {{\n        method: HttpMethod.{method}\n        path: \"{path}\"\n        input: BindRequest\n        output: BindResult\n        changes: false\n        source: PackageSource.written\n    }}\n}}\n"),
        ),
    ];
    out.extend(extra.iter().map(|(path, text)| at(path, text.to_string())));
    out
}

/// The app's own action, which calls `binds.create` as `body` says.
fn app(body: &str, above: &str) -> Vec<(String, String)> {
    vec![
        ("app.jadpo".to_string(), APP.to_string()),
        ("config.jadpo".to_string(), CONFIG.to_string()),
        (
            "domains/accounts/actions/bind.jadpo".to_string(),
            format!(
                "use app {{ directory }}\nuse shared/providers/acme_directory\n\n/// Checks a sign-in against the directory.\n{above}action bind(request: AcmeDirectory.BindRequest) -> AcmeDirectory.BindResult {{\n{body}\n}}\n"
            ),
        ),
    ]
}

const CALLS: &str = "    return directory.binds.create(request)";

fn found(files: Vec<(String, String)>) -> Vec<String> {
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    support::problems(Layout::Project, &files)
}

fn sending(fields: &str) -> Vec<String> {
    let mut files = package(fields, "post", "/v1/binds", &[]);
    files.extend(app(CALLS, ""));
    found(files)
}

const REFUSED: &str = "domains/accounts/actions/bind.jadpo type.password_sent_to_provider: `directory.binds.create` sends a `Password` to a provider";

#[test]
fn a_password_in_an_operations_input_is_refused_however_deep() {
    for (fields, why) in [
        ("    user: Text\n    password: Password\n", "a field"),
        ("    password: Password?\n", "may be none"),
        ("    passwords: List<Password>\n", "in a list"),
        ("    by_user: Map<Text, Password>\n", "a map's value"),
        ("    by_password: Map<Password, Text>\n", "a map's key"),
        ("    passwords: Set<Password>\n", "in a set"),
    ] {
        assert_eq!(sending(fields), [REFUSED], "{why}");
    }
    // A type built on one, and each container the input can reach it by.
    let built =
        "/// A staff password.\npublic type StaffPassword = Password {\n    min_length: 12\n}\n";
    let held = "/// What a bind holds.\npublic type Held = Object {\n    password: Password\n}\n";
    let fields = "/// A password or none.\npublic type Choice = Enum {\n    keyed: {\n        password: Password\n    }\n    open\n}\n";
    for (extra, field, why) in [
        (
            ("types/staff_password.jadpo", built),
            "    password: StaffPassword\n",
            "built on one",
        ),
        (
            ("types/held.jadpo", held),
            "    held: Held\n",
            "inside an object",
        ),
        (
            ("types/held.jadpo", held),
            "    held: List<Held>\n",
            "in a list of objects",
        ),
        (
            ("types/choice.jadpo", fields),
            "    choice: Choice\n",
            "in a variant's data",
        ),
    ] {
        let mut files = package(field, "post", "/v1/binds", &[extra]);
        files.extend(app(CALLS, ""));
        assert_eq!(found(files), [REFUSED], "{why}");
    }
}

#[test]
fn a_password_in_a_path_or_a_query_string_is_refused_too() {
    for (method, path) in [("post", "/v1/binds/{password}"), ("get", "/v1/binds")] {
        let mut files = package("    password: Password\n", method, path, &[]);
        files.extend(app(CALLS, ""));
        assert_eq!(found(files), [REFUSED], "{method} {path}");
    }
}

#[test]
fn only_a_password_is_refused_since_a_key_is_often_meant_for_the_provider() {
    for kind in KINDS.into_iter().filter(|kind| *kind != "Password") {
        assert_eq!(sending(&format!("    secret: {kind}\n")), NONE, "{kind}");
    }
}

#[test]
fn a_decline_lets_a_password_go_out() {
    let mut files = package("    password: Password\n", "post", "/v1/binds", &[]);
    files.extend(app(
        CALLS,
        "decline type.password_sent_to_provider: \"Checked against the old system during the migration\"\n",
    ));
    assert_eq!(found(files), NONE);
    // And a decline with nothing to answer is unused.
    let mut files = package("    secret: ApiKey\n", "post", "/v1/binds", &[]);
    files.extend(app(
        CALLS,
        "decline type.password_sent_to_provider: \"Not needed\"\n",
    ));
    let problems = found(files);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("decline.unused"), "{problems:?}");
}

/// A call nested in a value is still in its action: a `decline` line
/// can't stand inside a body, so the action's answers it.
#[test]
fn a_decline_above_the_action_answers_a_call_nested_in_a_value() {
    let mut files = package("    password: Password\n", "post", "/v1/binds", &[]);
    files.extend(vec![
        ("app.jadpo".to_string(), APP.to_string()),
        ("config.jadpo".to_string(), CONFIG.to_string()),
        (
            "domains/accounts/types/answer.jadpo".to_string(),
            "/// What the form is told.\ntype Answer = Object {\n    bound: Bool\n}\n".to_string(),
        ),
        (
            "domains/accounts/actions/answer_for.jadpo".to_string(),
            "use app { directory }\nuse accounts/types { Answer }\nuse shared/providers/acme_directory\n\n/// Answers from the directory.\ndecline type.password_sent_to_provider: \"Old system, during the migration off it\"\naction answer_for(request: AcmeDirectory.BindRequest) -> Answer {\n    return Answer {\n        bound: directory.binds.create(request).bound\n    }\n}\n".to_string(),
        ),
    ]);
    assert_eq!(found(files), NONE);
}

#[test]
fn a_password_sent_from_a_message_built_at_the_call_is_refused() {
    let mut files = package("    password: Password\n", "post", "/v1/binds", &[]);
    files.extend(vec![
        ("app.jadpo".to_string(), APP.to_string()),
        ("config.jadpo".to_string(), CONFIG.to_string()),
        (
            "domains/accounts/actions/bind.jadpo".to_string(),
            "use app { directory }\nuse shared/providers/acme_directory\n\n/// Checks a password against the directory.\naction bind(given: Password) -> AcmeDirectory.BindResult {\n    return directory.binds.create(AcmeDirectory.BindRequest {\n        password: given\n    })\n}\n".to_string(),
        ),
    ]);
    assert_eq!(found(files), [REFUSED]);
}

/// An app connected to the package, whose domain does nothing with it.
fn app_doing_nothing() -> Vec<(String, String)> {
    vec![
        ("app.jadpo".to_string(), APP.to_string()),
        ("config.jadpo".to_string(), CONFIG.to_string()),
        (
            "domains/accounts/actions/nothing.jadpo".to_string(),
            "/// Nothing.\naction nothing() {\n}\n".to_string(),
        ),
    ]
}

/// A package whose public action `bind` takes `request` and does `does`
/// with it, through the app's `directory`.
fn through_an_action(action: &str, call: &str) -> Vec<String> {
    let mut files = package(
        "    password: Password\n",
        "post",
        "/v1/binds",
        &[("actions/bind.jadpo", action)],
    );
    files.extend(vec![
        ("app.jadpo".to_string(), APP.to_string()),
        ("config.jadpo".to_string(), CONFIG.to_string()),
        (
            "domains/accounts/actions/bind.jadpo".to_string(),
            call.to_string(),
        ),
    ]);
    found(files)
}

const PACKAGE_ACTION: &str = "/// Binds, through the package's own operation.\npublic action bind(request: BindRequest) -> BindResult {\n    return binds.create(request)\n}\n";

const CALLS_ACTION: &str = "use app { directory }\nuse shared/providers/acme_directory\n\n/// Checks a sign-in through the package's own action.\n{above}action bind(request: AcmeDirectory.BindRequest) -> AcmeDirectory.BindResult {\n    return directory.bind(request)\n}\n";

#[test]
fn a_packages_action_given_a_password_is_answered_by_the_app_that_gave_it() {
    let refused = through_an_action(PACKAGE_ACTION, &CALLS_ACTION.replace("{above}", ""));
    assert_eq!(
        refused,
        [
            "domains/accounts/actions/bind.jadpo type.password_sent_to_provider: `directory.bind` sends a `Password` to a provider"
        ]
    );
    // The app signs it off, not the package.
    let declined = through_an_action(
        PACKAGE_ACTION,
        &CALLS_ACTION.replace(
            "{above}",
            "decline type.password_sent_to_provider: \"Binds against the directory we are moving off\"\n",
        ),
    );
    assert_eq!(declined, NONE);
}

#[test]
fn a_packages_action_that_keeps_a_password_to_itself_needs_no_signature() {
    // It checks the password here, against what the provider holds.
    let keeps = "/// Whether a password is a user's, by the hash its provider keeps.\npublic action checks(given: Password, kept: PasswordHash) -> PasswordCheck {\n    return crypto.verify_password(given, kept)\n}\n";
    let mut files = package(
        "    user: Text\n",
        "post",
        "/v1/binds",
        &[("actions/checks.jadpo", keeps)],
    );
    files.extend(vec![
        ("app.jadpo".to_string(), APP.to_string()),
        ("config.jadpo".to_string(), CONFIG.to_string()),
        (
            "domains/accounts/actions/checks.jadpo".to_string(),
            "use app { directory }\n\n/// Checks a password here.\naction checks(given: Password, kept: PasswordHash) -> PasswordCheck {\n    return directory.checks(given, kept)\n}\n".to_string(),
        ),
    ]);
    assert_eq!(found(files), NONE);
}

#[test]
fn a_package_declaring_or_using_such_an_operation_isnt_answered_in_the_package() {
    // Described in the package and called by nobody: no one's password
    // is going anywhere.
    let mut files = package("    password: Password\n", "post", "/v1/binds", &[]);
    files.extend(app_doing_nothing());
    assert_eq!(found(files), NONE);
    // Its own action, called by nobody here, is the same.
    assert_eq!(
        through_an_action(PACKAGE_ACTION, "/// Nothing.\naction bind() {\n}\n"),
        NONE
    );
    // And the package can't sign it off for the apps that install it: the
    // rule doesn't fire there, so a decline there has nothing to answer.
    let signed = PACKAGE_ACTION.replace(
        "public action",
        "decline type.password_sent_to_provider: \"Our bind is safe\"\npublic action",
    );
    let problems = through_an_action(&signed, &CALLS_ACTION.replace("{above}", ""));
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("decline.unused")),
        "{problems:?}"
    );
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("type.password_sent_to_provider")),
        "{problems:?}"
    );
}

// --- webhooks, queues and models --------------------------------------------

#[test]
fn no_secret_goes_back_in_a_webhooks_reply() {
    for kind in KINDS {
        let mut files = package("    user: Text\n", "post", "/v1/binds", &[]);
        files.push((
            format!("{HOME}/types/ring_reply.jadpo"),
            format!("/// What the directory is told.\npublic type RingReply = Object {{\n    secret: {kind}\n}}\n"),
        ));
        files.push((
            format!("{HOME}/events/rung.jadpo"),
            "/// The directory rang.\npublic type Rung = Event {\n    wire_name: \"rung\"\n    reply: RingReply\n    occurred_at: Instant\n}\n".to_string(),
        ));
        files.extend(app_doing_nothing());
        let problems = found(files);
        assert!(
            problems.iter().any(|problem| {
                problem.contains("type.secret_exposed") && problem.contains("`reply:`")
            }),
            "{kind}: {problems:?}"
        );
    }
}

// --- a route ------------------------------------------------------------------

const ROUTE: &str =
    "type Same = Bool {}\n\ntype Named = {kind} {}\n\ntype View = Output {\n    ok: Same\n}\n\n";

fn route(kind: &str, rest: &str) -> Vec<String> {
    let source = format!("{}{rest}", ROUTE.replace("{kind}", kind));
    support::problems(Layout::Loose, &[("main.jadpo", source.as_str())])
}

fn by_path(placeholder: &str, kind: &str) -> Vec<String> {
    route(
        kind,
        &format!(
            "action by(x: {placeholder}) -> View {{\n    return View {{\n        ok: Same(x == x)\n    }}\n}}\n\npolicy {{\n    actions: {{\n        by: [Access.public]\n    }}\n}}\n\nroutes T {{\n    prefix: /t\n\n    GET /{{x: {placeholder}}} {{\n        authentication: RouteAuthentication.none\n        output: View\n        run: by(x)\n    }}\n}}\n"
        ),
    )
}

#[test]
fn no_secret_comes_in_a_path() {
    for kind in KINDS {
        for placeholder in [kind, "Named"] {
            let problems = by_path(placeholder, kind);
            assert_eq!(problems.len(), 1, "{kind} {placeholder}: {problems:?}");
            assert!(
                problems[0].contains("route.secret_outside_body")
                    && problems[0].contains("`x`")
                    && problems[0].contains(kind),
                "{problems:?}"
            );
        }
    }
}

fn by_query(kind: &str, method: &str, field: &str) -> Vec<String> {
    route(
        kind,
        &format!(
            "type Q = Input {{\n    {field}\n}}\n\naction by(q: Q) -> View {{\n    return View {{\n        ok: Same(true)\n    }}\n}}\n\npolicy {{\n    actions: {{\n        by: [Access.public]\n    }}\n}}\n\nroutes T {{\n    prefix: /t\n\n    {method} / {{\n        authentication: RouteAuthentication.none\n        input: Q\n        output: View\n        run: by(input)\n    }}\n}}\n"
        ),
    )
}

#[test]
fn no_secret_comes_in_a_query_string() {
    for kind in KINDS {
        for field in [
            format!("x: {kind}"),
            "x: Named".to_string(),
            "x: List<Named>".to_string(),
            format!("x: {kind}?"),
        ] {
            for method in ["GET", "DELETE"] {
                let problems = by_query(kind, method, &field);
                assert_eq!(problems.len(), 1, "{kind} {method} {field}: {problems:?}");
                assert!(
                    problems[0].contains("route.secret_outside_body")
                        && problems[0].contains("`Q.x`")
                        && problems[0].contains(kind),
                    "{problems:?}"
                );
            }
        }
    }
}

#[test]
fn a_secret_comes_in_a_body() {
    for kind in KINDS {
        for field in [
            format!("x: {kind}"),
            "x: Named".to_string(),
            "x: List<Named>".to_string(),
            format!("x: {kind}?"),
        ] {
            for method in ["POST", "PUT", "PATCH"] {
                assert_eq!(
                    by_query(kind, method, &field),
                    NONE,
                    "{kind} {method} {field}"
                );
            }
        }
    }
}

#[test]
fn a_header_carries_only_a_token_or_a_key() {
    let header = |ty: &str| format!("x: {ty} {{\n        header: \"X-Thing\"\n    }}");
    for kind in ["ApiKey", "BearerToken", "Jwt"] {
        for method in ["GET", "POST"] {
            assert_eq!(
                by_query(kind, method, &header(kind)),
                NONE,
                "{kind} {method}"
            );
            assert_eq!(
                by_query(kind, method, &header("Named")),
                NONE,
                "{kind} {method}"
            );
        }
    }
    for kind in ["Password", "TotpSecret", "SecretText", "OneTimeCode"] {
        for method in ["GET", "POST"] {
            for ty in [kind, "Named"] {
                let problems = by_query(kind, method, &header(ty));
                assert_eq!(problems.len(), 1, "{kind} {method} {ty}: {problems:?}");
                assert!(
                    problems[0].contains("route.secret_outside_body")
                        && problems[0].contains("`Q.x`")
                        && problems[0].contains(kind)
                        && problems[0].contains("header"),
                    "{problems:?}"
                );
            }
        }
    }
}

/// Answered with a value of `shape`, its action handing it straight back.
fn answering(kind: &str, shape: &str, name: &str) -> Vec<String> {
    route(
        kind,
        &format!(
            "type Held = Object {{\n    secret: {kind}\n}}\n\ntype Choice = Enum {{\n    keyed: {{\n        key: {kind}\n    }}\n    open\n}}\n\ntype Wrap = Input {{\n    value: {shape}\n}}\n\naction give_{name}(x: {shape}) -> {shape} {{\n    return x\n}}\n\npolicy {{\n    actions: {{\n        give_{name}: [Access.public]\n    }}\n}}\n\nroutes T {{\n    prefix: /t\n\n    POST / {{\n        authentication: RouteAuthentication.none\n        input: Wrap\n        output: {shape}\n        run: give_{name}(input.value)\n    }}\n}}\n"
        ),
    )
}

#[test]
fn no_route_answers_with_a_bare_secret_or_what_holds_one() {
    for kind in KINDS {
        for (shape, name) in [
            (kind, "bare"),
            ("Named", "named"),
            ("List<Named>", "list"),
            ("Held", "object"),
            ("Choice", "variant"),
        ] {
            let problems = answering(kind, shape, name);
            assert_eq!(problems.len(), 1, "{kind} {shape}: {problems:?}");
            assert!(
                problems[0].contains("route.output_not_output"),
                "{kind} {shape}: {problems:?}"
            );
        }
    }
}

#[test]
fn no_output_holds_a_secret_however_it_is_sent() {
    for kind in KINDS {
        // An `Output` named as a route's raw body, as JSON or in a stream
        // is the same `Output`, and it is refused where it is declared.
        let problems = route(
            kind,
            &format!(
                "type Raw = Output {{\n    secret: {kind}\n}}\n\naction give() -> Raw {{\n    return unreachable_raw()\n}}\n\npolicy {{\n    actions: {{\n        give: [Access.public]\n    }}\n}}\n\nroutes T {{\n    prefix: /t\n\n    POST / {{\n        authentication: RouteAuthentication.none\n        output: Raw\n        sends: \"text/plain\"\n        run: give()\n    }}\n}}\n"
            ),
        );
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("type.secret_exposed")
                    && problem.contains("Raw.secret")),
            "{kind}: {problems:?}"
        );
    }
}

#[test]
fn no_header_cookie_or_redirect_is_built_from_a_route() {
    // A route says what it answers with and nothing else: no line of it
    // sets a response's headers, a cookie or a redirect.
    for member in ["headers", "cookies", "redirect", "location"] {
        let problems = route(
            "ApiKey",
            &format!(
                "action give() -> View {{\n    return View {{\n        ok: Same(true)\n    }}\n}}\n\npolicy {{\n    actions: {{\n        give: [Access.public]\n    }}\n}}\n\nroutes T {{\n    prefix: /t\n\n    GET / {{\n        authentication: RouteAuthentication.none\n        output: View\n        {member}: \"anything\"\n        run: give()\n    }}\n}}\n"
            ),
        );
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("route.item_unknown")),
            "{member}: {problems:?}"
        );
    }
    // An `Output`'s field with `header:` is one more way to name it.
    for kind in KINDS {
        let problems = route(
            kind,
            &format!(
                "type Headed = Output {{\n    secret: {kind} {{\n        header: \"X-Secret\"\n    }}\n}}\n"
            ),
        );
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("type.secret_exposed")),
            "{kind}: {problems:?}"
        );
    }
}

#[test]
fn a_secret_is_not_text_so_nothing_made_for_text_carries_one() {
    for kind in KINDS {
        for source in [
            format!(
                "function f(given: {kind}) -> Same {{\n    var written: Text = given\n    return Same(written == \"x\")\n}}\n"
            ),
            format!(
                "type Holder = Object {{\n    held: Text\n}}\n\nfunction f(given: {kind}) -> Holder {{\n    return Holder {{\n        held: given\n    }}\n}}\n"
            ),
        ] {
            let problems = route(kind, &source);
            assert_eq!(problems.len(), 1, "{kind}: {problems:?}");
            assert!(problems[0].contains("type.mismatch"), "{problems:?}");
        }
    }
}

#[test]
fn no_prompt_holds_a_secret() {
    for kind in KINDS {
        let problems = route(
            kind,
            &format!(
                "type Asked = Prompt {{\n    template: \"Is this right? {{secret}}\"\n    secret: {kind}\n}}\n"
            ),
        );
        assert!(
            problems.iter().any(|problem| problem.contains(kind)),
            "{kind}: {problems:?}"
        );
    }
}

// --- OpenAPI ------------------------------------------------------------------

/// A secret value is only ever sent to the server, and the document gives
/// no example of one: it gives no examples at all.
#[test]
fn the_openapi_document_gives_no_example_of_a_secret() {
    let fields: String = KINDS
        .iter()
        .map(|kind| format!("    {}: {kind}\n", kind.to_lowercase()))
        .collect();
    let input = format!("/// What the form sends.\ntype Form = Input {{\n{fields}}}\n");
    let files: Vec<(&str, &str)> = vec![
        (
            "app.jadpo",
            "application {\n    apis: {\n        main: {\n            prefix: /v1\n        }\n    }\n}\n",
        ),
        ("domains/forms/types/form.jadpo", &input),
        (
            "domains/forms/types/answer.jadpo",
            "/// What it is told.\ntype Answer = Output {\n    ok: Bool\n}\n",
        ),
        (
            "domains/forms/actions/take.jadpo",
            "use forms/types { Form, Answer }\n\n/// Takes a form.\naction take(form: Form) -> Answer {\n    return Answer {\n        ok: form.password == \"anything\"\n    }\n}\n",
        ),
        (
            "domains/forms/routes/v1_forms.jadpo",
            "use forms/actions { take }\nuse forms/types { Form, Answer }\n\nroutes V1Forms {\n    prefix: /v1/forms\n\n    POST / {\n        authentication: RouteAuthentication.none\n        input: Form\n        output: Answer\n        run: take(input)\n    }\n}\n",
        ),
        (
            "policy.jadpo",
            "use forms\n\npolicy {\n    actions: {\n        forms.take: [Access.public]\n    }\n}\n",
        ),
    ];
    let found: Vec<String> = support::problems(Layout::Project, &files)
        .into_iter()
        .filter(|problem| !problem.contains("record.out_of_date"))
        .collect();
    assert_eq!(found, NONE);
    let db = jadpo_db::Database::default();
    let project = support::project(&db, Layout::Project, &files);
    let document = jadpo_check::routes::openapi::document(&db, project, "main", "forms", "1")
        .expect("a document");
    let written = format!("{document:?}");
    assert!(!written.contains("example"), "{written}");
    assert!(written.contains("writeOnly"), "{written}");
}
