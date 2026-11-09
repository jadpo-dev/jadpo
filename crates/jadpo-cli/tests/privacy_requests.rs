//! Data requests (area 15, "Data requests"): the routes `privacy:`
//! generates, driven through `jadpo test` and, in the compiled group, the
//! compiled corpus: a person asks by address and confirms, signed in, and
//! staff make a request for them; each is a record with its deadline and
//! an event; a cancel link cancels an erasure, once.

use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    run(&args, &Environment::empty().with_cwd(cwd))
}

/// A project that takes data requests by address, signed in and from
/// staff, noting each event it hears.
pub(crate) fn files() -> Vec<(String, String)> {
    let mut out: Vec<(&str, &str)> = vec![
        (
            "config.jadpo",
            r#"config {
    site: Url {
        default: "https://notes.example"
    }
    mail_sender: Email {
        default: "hello@notes.example"
    }
    smtp_host: Hostname {
        default: "localhost"
    }
}
"#,
        ),
        (
            "app.jadpo",
            r#"use notes/entities { Member }

application {
    display_name: "Notes"
    connections: {
        mail: {
            driver: Driver.smtp
            host: config.smtp_host
        }
    }
    email: {
        connection: mail
        from: config.mail_sender
    }
    privacy: {
        self_service: {}
    }
}

principal {
    user: {
        subject: Member.email
    }
}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
"#,
        ),
        (
            "authentication.jadpo",
            r#"authentication {
    browser_session: {
        cookie: "notes_session"
        origin: config.site
    }
    sign_in: {
        magic_link: {
            link: "/sign-in/confirm"
        }
    }
}
"#,
        ),
        (
            "policy.jadpo",
            r#"use notes
use notes/types { StaffRole }

policy {
    actions: {
        privacy.create_request: [StaffRole.support]
        notes.make_request: [StaffRole.support]
    }
}
"#,
        ),
        (
            "domains/notes/types/staff_role.jadpo",
            "type StaffRole = Enum {\n    support\n}\n",
        ),
        (
            "domains/notes/types/made_request.jadpo",
            "use notes/entities { Member }\n\ntype MadeRequest = Input {\n    member_id: Member.id\n    kind: PrivacyRequestKind\n    how_checked: Text {\n        max_length: 200\n    }\n}\n",
        ),
        (
            "domains/notes/types/request_view.jadpo",
            "type RequestView = Output {\n    request_id: Uuid\n    kind: PrivacyRequestKind\n    deadline: Instant\n}\n",
        ),
        (
            "domains/notes/entities/member.jadpo",
            r#"/// Someone with notes.
entity Member {
    id: Uuid
    email: Email {
        unique: true
        personal: PersonalData.contact
    }
    active: Bool

    person: true
    policy: {
        create: [Access.public]
        read: [id, Access.system]
        update: [id]
        delete: [id]
    }
}
"#,
        ),
        (
            "domains/notes/entities/staff_member.jadpo",
            r#"use notes/entities { Member }
use notes/types { StaffRole }

/// A member of staff.
entity StaffMember {
    id: Uuid
    member_id: Member.id {
        on_delete: OnDelete.cascade
    }
    role: StaffRole

    membership: {
        scope: MembershipScope.application
        member: member_id
        role: role
    }

    policy: {
        create: [Access.public]
        read: [member_id]
    }
}
"#,
        ),
        (
            "domains/notes/entities/heard.jadpo",
            r#"/// What the application heard of a data request.
entity Heard {
    id: Uuid
    what: Text {
        max_length: 200
    }
    kind: PrivacyRequestKind?
    source: PrivacyRequestSource?
    deadline: Instant?
    erases_at: Instant?
    cancel_link: Url?

    policy: {
        create: [Access.system]
        read: [Access.public]
    }
}
"#,
        ),
        (
            "domains/notes/failures/already_a_member.jadpo",
            "/// That address already has an account.\ntype AlreadyAMember = Failure {\n    kind: FailureKind.conflict\n}\n",
        ),
        (
            "domains/notes/queries/all_heard.jadpo",
            "use notes/entities { Heard }\n\nquery all_heard() -> List<Heard> {\n    from: Heard\n    order: [Heard.what asc]\n    limit: 20\n}\n",
        ),
        (
            "domains/notes/actions/note_received.jadpo",
            r#"use notes/entities { Heard }

/// Keeps a request received.
action note_received(event: PrivacyRequestReceived) -> Heard {
    return Heard.create {
        set: {
            what: "received"
            kind: event.kind
            source: event.source
            deadline: event.deadline
            erases_at: none
            cancel_link: none
        }
    }
}
"#,
        ),
        (
            "domains/notes/actions/note_scheduled.jadpo",
            r#"use notes/entities { Heard }

/// Keeps an erasure scheduled.
action note_scheduled(event: PrivacyErasureScheduled) -> Heard {
    return Heard.create {
        set: {
            what: "scheduled"
            kind: none
            source: none
            deadline: none
            erases_at: event.erases_at
            cancel_link: event.cancel_link
        }
    }
}
"#,
        ),
        (
            "domains/notes/subscribers/note_requests.jadpo",
            "use notes/actions { note_received }\n\n/// Notes each request.\nsubscriber note_requests {\n    on: PrivacyRequestReceived\n    run: note_received(event)\n}\n",
        ),
        (
            "domains/notes/subscribers/note_schedules.jadpo",
            "use notes/actions { note_scheduled }\n\n/// Notes each scheduled erasure.\nsubscriber note_schedules {\n    on: PrivacyErasureScheduled\n    run: note_scheduled(event)\n}\n",
        ),
        (
            "domains/notes/actions/make_request.jadpo",
            r#"use notes/types { MadeRequest, RequestView }

/// Staff make a request for someone they have identified.
action make_request(input: MadeRequest) -> RequestView {
    var made: PrivacyRequestAccepted = privacy.create_request(input.member_id, input.kind, input.how_checked)
    return RequestView {
        request_id: made.request_id
        kind: made.kind
        deadline: made.deadline
    }
}
"#,
        ),
        (
            "domains/notes/routes/staff_requests.jadpo",
            r#"use notes/actions { make_request }
use notes/types { MadeRequest, RequestView }

routes StaffRequests {
    prefix: /staff/requests

    /// Makes a request for someone staff have identified.
    POST / {
        input: MadeRequest
        output: RequestView
        status: SuccessStatus.accepted
        run: make_request(input)
    }
}
"#,
        ),
        (
            "domains/notes/routes/staff_requests.test.jadpo",
            r#"use notes/entities { Heard, Member, StaffMember }
use notes/failures { AlreadyAMember }
use notes/queries { all_heard }
use notes/types { MadeRequest, StaffRole }

fixture member_at(address: Member.email) -> Member {
    return attempt Member.create {
        set: {
            email: address
            active: true
        }
        conflict: {
            Member.email: AlreadyAMember
        }
    }
}

fixture staff_at(address: Member.email) -> Member {
    var member: Member = member_at(address)
    var staff: StaffMember = StaffMember.create {
        set: {
            member_id: member.id
            role: StaffRole.support
        }
    }
    assert staff.member_id == member.id
    return member
}

test "someone with an account asks by address and exports their data" {
    var ada: Member = member_at("ada@notes.example")
    var asked: Response = request {
        method: HttpMethod.post
        path: "/privacy/requests"
        input: PrivacyAsk {
            address: "ada@notes.example"
        }
    }
    assert asked.status == 202
    var sent: List<SentEmail> = email.sent
    assert list.count(sent) == 1
    match list.first(sent) {
        none -> {
            assert list.count(sent) == 1
        }
        some(message) -> {
            assert message.to == ada.email
            assert text.contains(message.subject, "Notes")
            var confirmed: Response = request {
                method: HttpMethod.post
                path: "/privacy/requests/confirm"
                input: PrivacyConfirmation {
                    token: message.privacy_token
                    kind: PrivacyRequestKind.export
                }
            }
            assert confirmed.status == 202
            match confirmed.output {
                none -> {
                    assert confirmed.output != none
                }
                some(output) -> {
                    assert output.kind == PrivacyRequestKind.export
                }
            }
            var again: Response = request {
                method: HttpMethod.post
                path: "/privacy/requests/confirm"
                input: PrivacyConfirmation {
                    token: message.privacy_token
                    kind: PrivacyRequestKind.export
                }
            }
            assert again.status == 401
            assert again.code == "privacy_link_expired"
        }
    }
    assert list.count(all_heard()) == 1
}

test "an address nobody has is told the same and sent nothing" {
    var asked: Response = request {
        method: HttpMethod.post
        path: "/privacy/requests"
        input: PrivacyAsk {
            address: "nobody@notes.example"
        }
    }
    assert asked.status == 202
    assert list.count(email.sent) == 0
}

test "an erasure by address is scheduled and its cancel link cancels it once" {
    var ada: Member = member_at("ada@notes.example")
    var asked: Response = request {
        method: HttpMethod.post
        path: "/privacy/requests"
        input: PrivacyAsk {
            address: ada.email
        }
    }
    assert asked.status == 202
    match list.first(email.sent) {
        none -> {
            assert list.count(email.sent) == 1
        }
        some(message) -> {
            var confirmed: Response = request {
                method: HttpMethod.post
                path: "/privacy/requests/confirm"
                input: PrivacyConfirmation {
                    token: message.privacy_token
                    kind: PrivacyRequestKind.erasure
                }
            }
            assert confirmed.status == 202
            var heard: List<Heard> = all_heard()
            assert list.count(heard) == 2
            match list.last(heard) {
                none -> {
                    assert list.count(heard) == 2
                }
                some(scheduled) -> {
                    assert scheduled.what == "scheduled"
                    match scheduled.cancel_link {
                        none -> {
                            assert scheduled.cancel_link != none
                        }
                        some(link) -> {
                            var token: SignInToken = SignInToken(text.replace(link, "https://notes.example/privacy/cancel?token=", ""))
                            var cancelled: Response = request {
                                method: HttpMethod.post
                                path: "/privacy/requests/cancel"
                                input: PrivacyCancellation {
                                    token: token
                                }
                            }
                            assert cancelled.status == 204
                            var again: Response = request {
                                method: HttpMethod.post
                                path: "/privacy/requests/cancel"
                                input: PrivacyCancellation {
                                    token: token
                                }
                            }
                            assert again.status == 401
                            assert again.code == "privacy_link_expired"
                        }
                    }
                }
            }
        }
    }
}

test "a signed-in person exports and erases their own data" {
    var ada: Member = member_at("ada@notes.example")
    var export: Response = request {
        method: HttpMethod.post
        path: "/privacy/export"
        as: Principal.user(ada)
    }
    assert export.status == 202
    var erasure: Response = request {
        method: HttpMethod.post
        path: "/privacy/erasure"
        as: Principal.user(ada)
    }
    assert erasure.status == 202
    match erasure.output {
        none -> {
            assert erasure.output != none
        }
        some(output) -> {
            assert output.kind == PrivacyRequestKind.erasure
        }
    }
    var heard: List<Heard> = all_heard()
    assert list.count(heard) == 3
}

test "asking again while one is open gives the same request" {
    var ada: Member = member_at("ada@notes.example")
    var first: Response = request {
        method: HttpMethod.post
        path: "/privacy/erasure"
        as: Principal.user(ada)
    }
    var second: Response = request {
        method: HttpMethod.post
        path: "/privacy/erasure"
        as: Principal.user(ada)
    }
    assert first.status == 202
    assert second.status == 202
    match first.output {
        none -> {
            assert first.output != none
        }
        some(made) -> {
            match second.output {
                none -> {
                    assert second.output != none
                }
                some(again) -> {
                    assert made.request_id == again.request_id
                }
            }
        }
    }
    assert list.count(all_heard()) == 2
}

test "only someone signed in calls the signed-in routes" {
    var refused: Response = request {
        method: HttpMethod.post
        path: "/privacy/export"
    }
    assert refused.status == 401
}

test "staff make a request for someone they have identified" {
    var ada: Member = member_at("ada@notes.example")
    var bo: Member = staff_at("bo@notes.example")
    var made: Response = request {
        method: HttpMethod.post
        path: "/staff/requests"
        input: MadeRequest {
            member_id: ada.id
            kind: PrivacyRequestKind.export
            how_checked: "Passport shown at the counter"
        }
        as: Principal.user(bo)
    }
    assert made.status == 202
    match made.output {
        none -> {
            assert made.output != none
        }
        some(output) -> {
            assert output.kind == PrivacyRequestKind.export
        }
    }
    var heard: List<Heard> = all_heard()
    assert list.count(heard) == 1
    match list.first(heard) {
        none -> {
            assert list.count(heard) == 1
        }
        some(received) -> {
            assert received.source == PrivacyRequestSource.assisted
        }
    }
}

test "someone who isn't staff can't make a request" {
    var ada: Member = member_at("ada@notes.example")
    var refused: Response = request {
        method: HttpMethod.post
        path: "/staff/requests"
        input: MadeRequest {
            member_id: ada.id
            kind: PrivacyRequestKind.export
            how_checked: "Asked"
        }
        as: Principal.user(ada)
    }
    assert refused.status == 403
}
"#,
        ),
    ];
    out.push(("translations/en_gb.jadpo", "translations {\n}\n"));
    out.into_iter()
        .map(|(path, text)| (path.to_string(), text.to_string()))
        .collect()
}

/// The same project with `generate_routes: false` and the application's
/// own routes over `privacy.request`, `.confirm` and `.cancel`.
pub(crate) fn own_route_files() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = files()
        .into_iter()
        .filter(|(path, _)| !path.ends_with("staff_requests.test.jadpo"))
        .map(|(path, text)| {
            let text = if path == "app.jadpo" {
                text.replace("self_service: {}", "generate_routes: false")
            } else {
                text
            };
            (path, text)
        })
        .collect();
    let policy = "use notes
use notes/types { StaffRole }

policy {
    actions: {
        privacy.create_request: [StaffRole.support]
        notes.make_request: [StaffRole.support]
        notes.ask_own: [Access.public]
        notes.confirm_own: [Access.public]
        notes.cancel_own: [Access.public]
    }
}
";
    for (path, text) in [
        ("policy.jadpo", policy),
        (
            "domains/notes/types/own_ask.jadpo",
            "type OwnAsk = Input {\n    address: Email\n    locale: Locale\n}\n",
        ),
        (
            "domains/notes/types/own_confirm.jadpo",
            "type OwnConfirm = Input {\n    token: SignInToken\n    kind: PrivacyRequestKind\n}\n",
        ),
        (
            "domains/notes/types/own_cancel.jadpo",
            "type OwnCancel = Input {\n    token: SignInToken\n}\n",
        ),
        (
            "domains/notes/actions/ask_own.jadpo",
            "use notes/types { OwnAsk }\n\n/// Asks for an address check.\naction ask_own(input: OwnAsk) -> Unit {\n    privacy.request(input.address, input.locale)\n}\n",
        ),
        (
            "domains/notes/actions/confirm_own.jadpo",
            "use notes/types { OwnConfirm, RequestView }\n\n/// Confirms a request.\naction confirm_own(input: OwnConfirm) -> RequestView {\n    var made: PrivacyRequestAccepted = privacy.confirm(input.token, input.kind)\n    return RequestView {\n        request_id: made.request_id\n        kind: made.kind\n        deadline: made.deadline\n    }\n}\n",
        ),
        (
            "domains/notes/actions/cancel_own.jadpo",
            "use notes/types { OwnCancel }\n\n/// Cancels an erasure.\naction cancel_own(input: OwnCancel) -> Unit {\n    privacy.cancel(input.token)\n}\n",
        ),
        (
            "domains/notes/routes/own.jadpo",
            r#"use notes/actions { ask_own, cancel_own, confirm_own }
use notes/types { OwnAsk, OwnCancel, OwnConfirm, RequestView }

routes Own {
    prefix: /own

    /// Asks for an address check.
    POST /ask {
        authentication: RouteAuthentication.none
        input: OwnAsk
        status: SuccessStatus.no_content
        run: ask_own(input)
    }

    /// Confirms a request.
    POST /confirm {
        authentication: RouteAuthentication.none
        input: OwnConfirm
        output: RequestView
        status: SuccessStatus.accepted
        run: confirm_own(input)
    }

    /// Cancels an erasure.
    POST /cancel {
        authentication: RouteAuthentication.none
        input: OwnCancel
        status: SuccessStatus.no_content
        run: cancel_own(input)
    }
}
"#,
        ),
        (
            "domains/notes/routes/own.test.jadpo",
            r#"use notes/entities { Heard, Member }
use notes/failures { AlreadyAMember }
use notes/queries { all_heard }
use notes/types { OwnAsk, OwnCancel, OwnConfirm }

fixture member_at(address: Member.email) -> Member {
    return attempt Member.create {
        set: {
            email: address
            active: true
        }
        conflict: {
            Member.email: AlreadyAMember
        }
    }
}

test "the application's own routes make and cancel a request" {
    var ada: Member = member_at("ada@notes.example")
    var asked: Response = request {
        method: HttpMethod.post
        path: "/own/ask"
        input: OwnAsk {
            address: ada.email
            locale: Locale.en_gb
        }
    }
    assert asked.status == 204
    match list.first(email.sent) {
        none -> {
            assert list.count(email.sent) == 1
        }
        some(message) -> {
            match message.privacy_token {
                none -> {
                    assert message.privacy_token != none
                }
                some(token) -> {
                    var confirmed: Response = request {
                        method: HttpMethod.post
                        path: "/own/confirm"
                        input: OwnConfirm {
                            token: token
                            kind: PrivacyRequestKind.erasure
                        }
                    }
                    assert confirmed.status == 202
                    var heard: List<Heard> = all_heard()
                    assert list.count(heard) == 2
                    match list.last(heard) {
                        none -> {
                            assert list.count(heard) == 2
                        }
                        some(scheduled) -> {
                            match scheduled.cancel_link {
                                none -> {
                                    assert scheduled.cancel_link != none
                                }
                                some(link) -> {
                                    var cancel: SignInToken = SignInToken(text.replace(link, "https://notes.example/privacy/cancel?token=", ""))
                                    var cancelled: Response = request {
                                        method: HttpMethod.post
                                        path: "/own/cancel"
                                        input: OwnCancel {
                                            token: cancel
                                        }
                                    }
                                    assert cancelled.status == 204
                                    var again: Response = request {
                                        method: HttpMethod.post
                                        path: "/own/cancel"
                                        input: OwnCancel {
                                            token: cancel
                                        }
                                    }
                                    assert again.status == 401
                                    assert again.code == "privacy_link_expired"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
"#,
        ),
    ] {
        out.retain(|(at, _)| at != path);
        out.push((path.to_string(), text.to_string()));
    }
    out
}

fn project(name: &str, files: Vec<(String, String)>) -> PathBuf {
    let root = std::env::temp_dir().join(format!("jadpo-privacy-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    root
}

fn tests_pass(root: &Path) {
    for args in [&["migrate", "new", "initial"][..], &["records", "update"]] {
        let done = jadpo(root, args);
        assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    }
    let done = jadpo(root, &["test"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
}

#[test]
fn data_requests_pass_their_tests() {
    if std::env::var("JADPO_SKIP_POSTGRES").is_ok() {
        return;
    }
    tests_pass(&project("routes", files()));
}

#[test]
fn the_applications_own_routes_pass_their_tests() {
    if std::env::var("JADPO_SKIP_POSTGRES").is_ok() {
        return;
    }
    tests_pass(&project("own", own_route_files()));
}
