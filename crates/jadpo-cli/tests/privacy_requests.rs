//! Data requests (area 15, "Data requests"): the routes `privacy:`
//! generates, driven through `jadpo test` and, in the compiled group, the
//! compiled corpus: a person asks by address and confirms, signed in, and
//! staff make a request for them; each is a record with its deadline and
//! an event; a cancel link cancels an erasure, once. An export ends at its
//! one download and an erasure at its completion email, whichever executor
//! runs it.

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
    fines_kept: Duration {
        default: 2192d
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
        vault: {
            driver: Driver.disk
            path: "/var/lib/notes/vault"
        }
    }
    email: {
        connection: mail
        from: config.mail_sender
    }
    privacy: {
        erasure_log: {
            store: vault
        }
        exports: {
            store: vault
        }
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
            "domains/notes/entities/fine.jadpo",
            r#"use notes/entities { Member }

/// A fine a member was charged, kept for the tax year.
entity Fine {
    id: Uuid
    member_id: Member.id? {
        on_delete: OnDelete.restricted
        erase: Erasure.retained
    }
    issued_at: Instant {
        generated: Generated.on_create
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
    }

    retention: {
        after: config.fines_kept
        since: issued_at
    }
}
"#,
        ),
        (
            "domains/notes/translations/en_gb.jadpo",
            "translations {\n    fine: \"A fine\"\n}\n",
        ),
        (
            "domains/notes/queries/all_members.jadpo",
            "use notes/entities { Member }\n\nquery all_members() -> List<Member> {\n    from: Member\n    order: [Member.email asc]\n    limit: 20\n}\n",
        ),
        (
            "domains/notes/queries/all_fines.jadpo",
            "use notes/entities { Fine }\n\nquery all_fines() -> List<Fine> {\n    from: Fine\n    order: [Fine.issued_at asc]\n    limit: 20\n}\n",
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
    out.push((
        "domains/notes/queries/all_members.test.jadpo",
        REQUEST_STEPS,
    ));
    out.push(("translations/en_gb.jadpo", "translations {\n}\n"));
    out.into_iter()
        .map(|(path, text)| (path.to_string(), text.to_string()))
        .collect()
}

/// What a request leads to, each from asking to its end.
const REQUEST_STEPS: &str = r#"use notes/entities { Fine, Member }
use notes/failures { AlreadyAMember }
use notes/queries { all_fines }

fixture person_at(address: Member.email) -> Member {
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

fixture fine_for(member: Member) -> Fine {
    return Fine.create {
        set: {
            member_id: member.id
        }
    }
}

test "someone's export runs from asking to its one download" {
    var ada: Member = person_at("ada@notes.example")
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
        some(check) -> {
            var confirmed: Response = request {
                method: HttpMethod.post
                path: "/privacy/requests/confirm"
                input: PrivacyConfirmation {
                    token: check.privacy_token
                    kind: PrivacyRequestKind.export
                }
            }
            assert confirmed.status == 202
        }
    }
    assert list.count(email.sent) == 2
    match list.last(email.sent) {
        none -> {
            assert list.count(email.sent) == 2
        }
        some(ready) -> {
            assert ready.to == ada.email
            assert text.contains(ready.subject, "ready")
            match list.first(ready.links) {
                none -> {
                    assert list.count(ready.links) == 1
                }
                some(link) -> {
                    var token: SignInToken = SignInToken(text.replace(link, "https://notes.example/privacy/download?token=", ""))
                    var file: Response = request {
                        method: HttpMethod.post
                        path: "/privacy/exports/download"
                        input: PrivacyDownload {
                            token: token
                        }
                    }
                    assert file.status == 200
                    match file.output {
                        none -> {
                            assert file.output != none
                        }
                        some(output) -> {
                            assert text.contains(output.json, "ada@notes.example")
                            assert output.records == 1
                        }
                    }
                    var again: Response = request {
                        method: HttpMethod.post
                        path: "/privacy/exports/download"
                        input: PrivacyDownload {
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

test "an export's link stops working once it has ended" {
    var ada: Member = person_at("ada@notes.example")
    var export: Response = request {
        method: HttpMethod.post
        path: "/privacy/export"
        as: Principal.user(ada)
    }
    assert export.status == 202
    match list.last(email.sent) {
        none -> {
            assert list.count(email.sent) == 1
        }
        some(ready) -> {
            match list.first(ready.links) {
                none -> {
                    assert list.count(ready.links) == 1
                }
                some(link) -> {
                    clock.advance(7d)
                    var token: SignInToken = SignInToken(text.replace(link, "https://notes.example/privacy/download?token=", ""))
                    var file: Response = request {
                        method: HttpMethod.post
                        path: "/privacy/exports/download"
                        input: PrivacyDownload {
                            token: token
                        }
                    }
                    assert file.status == 401
                    assert file.code == "privacy_link_expired"
                }
            }
        }
    }
}

test "an erasure waits out its cooling-off and says what was kept" {
    var ada: Member = person_at("ada@notes.example")
    var fine: Fine = fine_for(ada)
    var erasure: Response = request {
        method: HttpMethod.post
        path: "/privacy/erasure"
        as: Principal.user(ada)
    }
    assert erasure.status == 202
    assert list.count(email.sent) == 1
    match list.last(email.sent) {
        none -> {
            assert list.count(email.sent) == 1
        }
        some(confirmed) -> {
            assert confirmed.to == ada.email
            assert text.contains(confirmed.subject, "erase")
            assert list.count(confirmed.links) == 1
        }
    }
    clock.advance(13d)
    assert list.count(all_members()) == 1
    clock.advance(1d)
    clock.advance(1h)
    assert list.count(all_members()) == 0
    assert list.count(all_fines()) == 1
    assert list.count(email.sent) == 2
    match list.last(email.sent) {
        none -> {
            assert list.count(email.sent) == 2
        }
        some(completed) -> {
            assert completed.to == ada.email
            assert text.contains(completed.subject, "erased")
            match completed.text {
                none -> {
                    assert completed.text != none
                }
                some(words) -> {
                    assert text.contains(words, "A fine, until")
                }
            }
        }
    }
    assert fine.member_id == ada.id
}

test "an erasure cancelled in its cooling-off never runs" {
    var ada: Member = person_at("ada@notes.example")
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
        some(check) -> {
            var confirmed: Response = request {
                method: HttpMethod.post
                path: "/privacy/requests/confirm"
                input: PrivacyConfirmation {
                    token: check.privacy_token
                    kind: PrivacyRequestKind.erasure
                }
            }
            assert confirmed.status == 202
        }
    }
    match list.last(email.sent) {
        none -> {
            assert list.count(email.sent) == 2
        }
        some(confirmed) -> {
            match list.first(confirmed.links) {
                none -> {
                    assert list.count(confirmed.links) == 1
                }
                some(link) -> {
                    var cancel: SignInToken = SignInToken(text.replace(link, "https://notes.example/privacy/cancel?token=", ""))
                    var cancelled: Response = request {
                        method: HttpMethod.post
                        path: "/privacy/requests/cancel"
                        input: PrivacyCancellation {
                            token: cancel
                        }
                    }
                    assert cancelled.status == 204
                }
            }
        }
    }
    clock.advance(15d)
    assert list.count(all_members()) == 1
    assert list.count(email.sent) == 2
}
"#;

/// The application's own routes (the file's name is about what it holds:
/// the operations, not the generated routes).
const OWN_DOWNLOAD: &str = r#"use notes/entities { Member }
use notes/failures { AlreadyAMember }
use notes/types { OwnAsk, OwnConfirm, OwnDownload }

fixture downloading_person(address: Member.email) -> Member {
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

test "the application's own route gives the export once" {
    var ada: Member = downloading_person("ada@notes.example")
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
        some(check) -> {
            match check.privacy_token {
                none -> {
                    assert check.privacy_token != none
                }
                some(asking) -> {
                    var confirmed: Response = request {
                        method: HttpMethod.post
                        path: "/own/confirm"
                        input: OwnConfirm {
                            token: asking
                            kind: PrivacyRequestKind.export
                        }
                    }
                    assert confirmed.status == 202
                }
            }
        }
    }
    match list.last(email.sent) {
        none -> {
            assert list.count(email.sent) == 2
        }
        some(ready) -> {
            match list.first(ready.links) {
                none -> {
                    assert list.count(ready.links) == 1
                }
                some(link) -> {
                    var token: SignInToken = SignInToken(text.replace(link, "https://notes.example/privacy/download?token=", ""))
                    var first: Response = request {
                        method: HttpMethod.post
                        path: "/own/download"
                        input: OwnDownload {
                            token: token
                        }
                    }
                    assert first.status == 200
                    var second: Response = request {
                        method: HttpMethod.post
                        path: "/own/download"
                        input: OwnDownload {
                            token: token
                        }
                    }
                    assert second.status == 401
                }
            }
        }
    }
}
"#;

/// The same project with `generate_routes: false` and the application's
/// own routes over `privacy.request`, `.confirm` and `.cancel`.
pub(crate) fn own_route_files() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = files()
        .into_iter()
        .filter(|(path, _)| {
            !path.ends_with("staff_requests.test.jadpo")
                && !path.ends_with("all_members.test.jadpo")
        })
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
        notes.download_own: [Access.public]
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
            "domains/notes/types/own_download.jadpo",
            "type OwnDownload = Input {\n    token: SignInToken\n}\n",
        ),
        (
            "domains/notes/types/own_export.jadpo",
            "type OwnExport = Output {\n    json: Text\n}\n",
        ),
        (
            "domains/notes/actions/download_own.jadpo",
            "use notes/types { OwnDownload, OwnExport }\n\n/// Gives an export, once.\naction download_own(input: OwnDownload) -> OwnExport {\n    var kept: PersonalDataExport = privacy.download(input.token)\n    return OwnExport {\n        json: kept.json\n    }\n}\n",
        ),
        (
            "domains/notes/actions/download_own.test.jadpo",
            OWN_DOWNLOAD,
        ),
        (
            "domains/notes/actions/cancel_own.jadpo",
            "use notes/types { OwnCancel }\n\n/// Cancels an erasure.\naction cancel_own(input: OwnCancel) -> Unit {\n    privacy.cancel(input.token)\n}\n",
        ),
        (
            "domains/notes/routes/own.jadpo",
            r#"use notes/actions { ask_own, cancel_own, confirm_own, download_own }
use notes/types { OwnAsk, OwnCancel, OwnConfirm, OwnDownload, OwnExport, RequestView }

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

    /// Gives an export, once.
    POST /download {
        authentication: RouteAuthentication.none
        input: OwnDownload
        output: OwnExport
        run: download_own(input)
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

/// Replaces or adds each of `changed` in `files`.
fn changing(mut files: Vec<(String, String)>, changed: &[(&str, &str)]) -> Vec<(String, String)> {
    for (path, text) in changed {
        files.retain(|(at, _)| at != path);
        files.push((path.to_string(), text.to_string()));
    }
    files
}

/// The base project with `changing` applied to `app.jadpo`.
fn with_app(
    files: Vec<(String, String)>,
    change: impl Fn(&str) -> String,
) -> Vec<(String, String)> {
    files
        .into_iter()
        .map(|(path, text)| {
            let text = if path == "app.jadpo" {
                change(&text)
            } else {
                text
            };
            (path, text)
        })
        .collect()
}

/// The project with the privacy emails sent as the application says: the
/// address check and the export in its own emails (level 4), the erasure's
/// two by its own sending (level 5).
pub(crate) fn own_mail_files() -> Vec<(String, String)> {
    let files: Vec<(String, String)> = files()
        .into_iter()
        .filter(|(path, _)| {
            !path.ends_with("staff_requests.test.jadpo")
                && !path.ends_with("all_members.test.jadpo")
        })
        .collect();
    let files = with_app(files, |app| {
        app.replace("use notes/entities { Member }\n", "use notes\nuse notes/entities { Member }\n").replace(
            "        self_service: {}\n",
            "        self_service: {}\n        emails: {\n            address_check: {\n                email: notes.CheckEmail\n            }\n            export_ready: {\n                email: notes.ExportEmail\n            }\n            erasure_confirmed: {\n                send: notes.send_confirmed\n            }\n            erasure_completed: {\n                send: notes.send_completed\n            }\n        }\n",
        )
    });
    changing(
        files,
        &[
            (
                "domains/notes/translations/en_gb.jadpo",
                "translations {\n    fine: \"A fine\"\n    check_email: \"Check it's you\"\n    export_email: \"Here is your data\"\n}\n",
            ),
            (
                "domains/notes/emails/check_email.jadpo",
                "/// Checks an address, in Notes' own words.\ntype CheckEmail = EmailMessage {\n    pick: [PrivacyAddressCheckEmail.confirm_link, PrivacyAddressCheckEmail.expires_in]\n}\n",
            ),
            (
                "domains/notes/emails/check_email.html",
                "<p><a href=\"{{ confirm_link }}\">Confirm</a> within {{ expires_in }}.</p>\n",
            ),
            (
                "domains/notes/emails/check_email.txt",
                "Confirm: {{ confirm_link }} within {{ expires_in }}.\n",
            ),
            (
                "domains/notes/emails/export_email.jadpo",
                "/// Delivers an export, in Notes' own words.\ntype ExportEmail = EmailMessage {\n    pick: [PrivacyExportEmail.download_link, PrivacyExportEmail.expires_in]\n}\n",
            ),
            (
                "domains/notes/emails/export_email.html",
                "<p><a href=\"{{ download_link }}\">Your data</a> for {{ expires_in }}.</p>\n",
            ),
            (
                "domains/notes/emails/export_email.txt",
                "Your data: {{ download_link }} for {{ expires_in }}.\n",
            ),
            (
                "domains/notes/actions/send_confirmed.jadpo",
                "use notes/entities { Heard }\n\n/// Notes how an erasure was confirmed.\naction send_confirmed(request: PrivacyErasureConfirmedRequest) -> Unit {\n    var noted: Heard = Heard.create {\n        set: {\n            what: \"confirmed\"\n            kind: none\n            source: none\n            deadline: none\n            erases_at: request.erases_at\n            cancel_link: request.cancel_link\n        }\n    }\n    assert noted.what == \"confirmed\"\n}\n",
            ),
            (
                "domains/notes/actions/send_completed.jadpo",
                "use notes/entities { Heard }\n\n/// Notes that an erasure finished, and what it kept.\naction send_completed(request: PrivacyErasureCompletedRequest) -> Unit {\n    var noted: Heard = Heard.create {\n        set: {\n            what: \"completed\"\n            kind: none\n            source: none\n            deadline: none\n            erases_at: none\n            cancel_link: none\n        }\n    }\n    assert list.count(request.kept) == 0\n    assert text.contains(request.address, \"@\")\n    assert noted.what == \"completed\"\n}\n",
            ),
            (
                "domains/notes/actions/send_completed.test.jadpo",
                OWN_MAIL_TESTS,
            ),
        ],
    )
}

const OWN_MAIL_TESTS: &str = r#"use notes/entities { Heard, Member }
use notes/failures { AlreadyAMember }
use notes/queries { all_heard }

fixture own_mail_person(address: Member.email) -> Member {
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

test "the application's own emails carry the links" {
    var ada: Member = own_mail_person("ada@notes.example")
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
        some(check) -> {
            assert check.subject == "Check it's you"
            var confirmed: Response = request {
                method: HttpMethod.post
                path: "/privacy/requests/confirm"
                input: PrivacyConfirmation {
                    token: check.privacy_token
                    kind: PrivacyRequestKind.export
                }
            }
            assert confirmed.status == 202
        }
    }
    match list.last(email.sent) {
        none -> {
            assert list.count(email.sent) == 2
        }
        some(ready) -> {
            assert ready.subject == "Here is your data"
            assert list.count(ready.links) == 1
        }
    }
}

test "the application's own sending is given the requests" {
    var ada: Member = own_mail_person("ada@notes.example")
    var erasure: Response = request {
        method: HttpMethod.post
        path: "/privacy/erasure"
        as: Principal.user(ada)
    }
    assert erasure.status == 202
    clock.advance(14d)
    clock.advance(1h)
    var heard: List<Heard> = all_heard()
    assert list.count(heard) == 4
    assert list.count(email.sent) == 0
}
"#;

/// The project with no mail connection: a subscriber hears the export
/// instead, and the only requests are made signed in.
pub(crate) fn no_mail_files() -> Vec<(String, String)> {
    let files: Vec<(String, String)> = files()
        .into_iter()
        .filter(|(path, _)| !path.ends_with(".test.jadpo"))
        .collect();
    let files = with_app(files, |app| {
        app.replace(
            "        mail: {\n            driver: Driver.smtp\n            host: config.smtp_host\n        }\n",
            "",
        )
        .replace(
            "    email: {\n        connection: mail\n        from: config.mail_sender\n    }\n",
            "",
        )
        .replace("        self_service: {}\n", "")
    });
    changing(
        files,
        &[
            (
                "authentication.jadpo",
                "use notes\n\nauthentication {\n    browser_session: {\n        cookie: \"notes_session\"\n        origin: config.site\n    }\n    sign_in: {\n        magic_link: {\n            link: \"/sign-in/confirm\"\n            send: notes.send_sign_in\n        }\n    }\n}\n",
            ),
            (
                "domains/notes/actions/send_sign_in.jadpo",
                "/// Hands a sign-in link to a person some other way.\naction send_sign_in(request: MagicLinkRequest) -> Unit {\n    assert text.contains(request.address, \"@\")\n}\n",
            ),
            (
                "domains/notes/actions/note_ready.jadpo",
                "use notes/entities { Heard }\n\n/// Keeps an export's link.\naction note_ready(event: PrivacyExportReady) -> Heard {\n    return Heard.create {\n        set: {\n            what: \"ready\"\n            kind: none\n            source: none\n            deadline: none\n            erases_at: none\n            cancel_link: event.download_link\n        }\n    }\n}\n",
            ),
            (
                "domains/notes/subscribers/note_exports.jadpo",
                "use notes/actions { note_ready }\n\n/// Keeps each export's link.\nsubscriber note_exports {\n    on: PrivacyExportReady\n    run: note_ready(event)\n}\n",
            ),
            (
                "domains/notes/actions/note_ready.test.jadpo",
                r#"use notes/entities { Heard, Member }
use notes/failures { AlreadyAMember }
use notes/queries { all_heard }

fixture quiet_person(address: Member.email) -> Member {
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

test "with no mail connection a subscriber hears the export" {
    var ada: Member = quiet_person("ada@notes.example")
    var made: Response = request {
        method: HttpMethod.post
        path: "/privacy/export"
        as: Principal.user(ada)
    }
    assert made.status == 202
    var heard: List<Heard> = all_heard()
    assert list.count(heard) == 2
    match list.first(heard) {
        none -> {
            assert list.count(heard) == 2
        }
        some(ready) -> {
            match ready.cancel_link {
                none -> {
                    assert ready.cancel_link != none
                }
                some(link) -> {
                    var token: SignInToken = SignInToken(text.replace(link, "https://notes.example/privacy/download?token=", ""))
                    var file: Response = request {
                        method: HttpMethod.post
                        path: "/privacy/exports/download"
                        input: PrivacyDownload {
                            token: token
                        }
                    }
                    assert file.status == 200
                }
            }
        }
    }
}
"#,
            ),
        ],
    )
}

/// The project with a provider that holds the person too, and that takes
/// its time to confirm: what the request says meanwhile, and that the
/// completion email waits for it.
pub(crate) fn provider_files() -> Vec<(String, String)> {
    let files: Vec<(String, String)> = files()
        .into_iter()
        .filter(|(path, _)| !path.ends_with(".test.jadpo"))
        .collect();
    let files = with_app(files, |app| {
        app.replace(
            "        vault: {\n",
            "        cards: {\n            package: AcmeCards.v1\n            domains: [notes]\n            credential: config.cards_api_key\n            environment: config.provider_environment\n            erasure: {\n                key: Member.cards_id\n            }\n        }\n        vault: {\n",
        )
    });
    let member = files
        .iter()
        .find(|(path, _)| path == "domains/notes/entities/member.jadpo")
        .map(|(_, text)| text.clone())
        .unwrap_or_default();
    let member = format!(
        "use shared/providers/acme_cards\n\n{}",
        member.replace(
            "    active: Bool\n",
            "    active: Bool\n    cards_id: AcmeCards.CustomerId?\n",
        )
    );
    let config = files
        .iter()
        .find(|(path, _)| path == "config.jadpo")
        .map(|(_, text)| text.clone())
        .unwrap_or_default()
        .replace(
            "config {\n",
            "config {\n    cards_api_key: Text {\n        secret: true\n    }\n    provider_environment: ProviderEnvironment {\n        default: ProviderEnvironment.sandbox\n    }\n",
        );
    changing(
        files,
        &[
            ("domains/notes/entities/member.jadpo", member.as_str()),
            ("config.jadpo", config.as_str()),
            (
                "shared/providers/acme_cards/v1/package.jadpo",
                "/// Acme Cards' API.\npackage AcmeCards.v1 {\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}\n",
            ),
            (
                "shared/providers/acme_cards/v1/provider.jadpo",
                "/// Acme Cards.\nprovider {\n    protocol: Protocol.json_http\n    hosts: {\n        live: \"https://api.acme.example\"\n        sandbox: \"https://sandbox.acme.example\"\n    }\n    credential: CredentialScheme.bearer\n    idempotency: {\n        header: \"Idempotency-Key\"\n        window: 24h\n    }\n    error_code: \"error.code\"\n    erasure: {\n        call: customers.delete\n        by: CustomerId\n    }\n}\n",
            ),
            (
                "shared/providers/acme_cards/v1/types/customer_id.jadpo",
                "/// Acme Cards' id for a customer.\npublic type CustomerId = Text {}\n",
            ),
            (
                "shared/providers/acme_cards/v1/types/customer_key.jadpo",
                "/// A customer to delete.\npublic type CustomerKey = Object {\n    /// Their id.\n    id: CustomerId\n}\n",
            ),
            (
                "shared/providers/acme_cards/v1/types/removed.jadpo",
                "/// A customer deleted.\npublic type Removed = Object {\n    deleted: Bool\n}\n",
            ),
            (
                "shared/providers/acme_cards/v1/failures/customer_gone.jadpo",
                "/// No customer has that id.\npublic type CustomerGone = Failure {\n    kind: FailureKind.not_found\n    message: \"No such customer.\"\n}\n",
            ),
            (
                "shared/providers/acme_cards/v1/resources/customers.jadpo",
                "/// Customers.\npublic resource customers {\n    /// Deletes a customer.\n    delete: {\n        method: HttpMethod.delete\n        path: \"/v1/customers/{id}\"\n        input: CustomerKey\n        output: Removed\n        changes: true\n        idempotent: true\n        failures: [CustomerGone]\n        source: PackageSource.written\n    }\n}\n",
            ),
            (
                "domains/notes/actions/note_overdue.jadpo",
                "use notes/entities { Heard }\n\n/// Keeps a request past its deadline.\naction note_overdue(event: PrivacyRequestOverdue) -> Heard {\n    return Heard.create {\n        set: {\n            what: \"overdue\"\n            kind: event.kind\n            source: none\n            deadline: event.deadline\n            erases_at: none\n            cancel_link: none\n        }\n    }\n}\n",
            ),
            (
                "domains/notes/subscribers/note_overdue_requests.jadpo",
                "use notes/actions { note_overdue }\n\n/// Keeps each overdue request.\nsubscriber note_overdue_requests {\n    on: PrivacyRequestOverdue\n    run: note_overdue(event)\n}\n",
            ),
            (
                "domains/notes/actions/note_overdue.test.jadpo",
                r#"use app { cards }
use notes/entities { Member }
use notes/failures { AlreadyAMember }
use notes/queries { all_heard, all_members }
use shared/providers/acme_cards

fixture carded_person(address: Member.email, cards_id: AcmeCards.CustomerId) -> Member {
    return attempt Member.create {
        set: {
            email: address
            active: true
            cards_id: cards_id
        }
        conflict: {
            Member.email: AlreadyAMember
        }
    }
}

test "a request a provider holds up goes overdue and the completion waits for it" {
    fakes: {
        cards.customers.delete: [unavailable, unavailable, unavailable, unavailable, unavailable, unavailable, AcmeCards.Removed {
            deleted: true
        }]
    }
    var ada: Member = carded_person("ada@notes.example", "cus_ada")
    var erasure: Response = request {
        method: HttpMethod.post
        path: "/privacy/erasure"
        as: Principal.user(ada)
    }
    assert erasure.status == 202
    clock.advance(14d)
    assert list.count(all_members()) == 0
    assert list.count(email.sent) == 1
    clock.advance(17d)
    var overdue: Bool = list.any(all_heard(), heard -> heard.what == "overdue")
    assert overdue
    assert list.count(email.sent) == 1
    clock.advance(1h)
    clock.advance(1h)
    assert list.count(email.sent) == 2
    assert list.count(cards.customers.delete.requests) == 7
}
"#,
            ),
        ],
    )
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

#[test]
fn the_applications_own_emails_and_sending_pass_their_tests() {
    if std::env::var("JADPO_SKIP_POSTGRES").is_ok() {
        return;
    }
    tests_pass(&project("own-mail", own_mail_files()));
}

#[test]
fn a_subscriber_delivers_the_export_with_no_mail_connection() {
    if std::env::var("JADPO_SKIP_POSTGRES").is_ok() {
        return;
    }
    tests_pass(&project("no-mail", no_mail_files()));
}

#[test]
fn a_provider_holding_a_request_up_is_overdue_and_held_for_it() {
    if std::env::var("JADPO_SKIP_POSTGRES").is_ok() {
        return;
    }
    tests_pass(&project("provider", provider_files()));
}

/// `jadpo <args>` at `T0` in plain text, as a script reads it.
fn listed(root: &Path, url: &str, args: &[&str]) -> Output {
    let mut args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    args.extend(["--url".to_string(), url.to_string()]);
    let env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    run(&args, &env.with_cwd(root))
}

const INSERT: &str = "INSERT INTO \"jadpo_privacy_requests\" (\"id\", \"kind\", \"source\", \"person\", \"state\", \"requested_at\", \"deadline\", \"locale\") VALUES ($1, $2, $3, $4, $5, $6, $7, 'en_gb')";

#[test]
fn an_operator_sees_each_request_and_how_close_its_deadline_is() {
    let root = project("operator", files());
    let mistake = jadpo(&root, &["privacy", "requests", "--state", "closed"]);
    assert_eq!(mistake.code, 2, "{}{}", mistake.stdout, mistake.stderr);
    assert!(
        mistake.stderr.contains("`closed` isn't a state"),
        "{}",
        mistake.stderr
    );
    let none = jadpo(&root, &["privacy", "frobnicate"]);
    assert_eq!(none.code, 2, "{}{}", none.stdout, none.stderr);
    assert!(
        none.stderr.contains("has no action `frobnicate`"),
        "{}",
        none.stderr
    );
    if std::env::var("JADPO_SKIP_POSTGRES").is_ok() || !jadpo_test::live() {
        return;
    }
    for args in [&["migrate", "new", "initial"][..], &["records", "update"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    }
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let host = jadpo_test::host::Harness::postgres(&url)
        .at(0)
        .random_from(0);
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    let database = jadpo_runtime::Host::database(&host, "main").expect("main");
    const DAY: i64 = 86_400_000;
    let now: i64 = 1_792_504_800_000;
    let row = |at: u8, kind: &str, source: &str, state: &str, deadline: i64| {
        vec![
            jadpo_runtime::SqlValue::Uuid([at; 16]),
            jadpo_runtime::SqlValue::Text(kind.to_string()),
            jadpo_runtime::SqlValue::Text(source.to_string()),
            jadpo_runtime::SqlValue::Uuid([9; 16]),
            jadpo_runtime::SqlValue::Text(state.to_string()),
            jadpo_runtime::SqlValue::Timestamp((now - 20 * DAY) * 1000),
            jadpo_runtime::SqlValue::Timestamp(deadline * 1000),
        ]
    };
    for params in [
        row(1, "erasure", "self_service", "stalled", now - 3 * DAY),
        row(2, "export", "signed_in", "received", now + 5 * DAY),
        row(3, "erasure", "assisted", "received", now + 20 * DAY),
        row(4, "export", "self_service", "downloaded", now + 9 * DAY),
    ] {
        runtime
            .block_on(database.query_once(INSERT, &params))
            .expect("kept");
    }
    let open = listed(&root, &url, &["privacy", "requests"]);
    assert_eq!(open.code, 0, "{}{}", open.stdout, open.stderr);
    let lines: Vec<&str> = open.stdout.lines().collect();
    // Nearest deadline first, each said in days, and nothing closed.
    assert!(
        lines.iter().any(|line| line.contains("erasure")
            && line.contains("stalled")
            && line.contains("3 days overdue")),
        "{}",
        open.stdout
    );
    assert!(
        lines.iter().any(|line| line.contains("export")
            && line.contains("signed in")
            && line.contains("in 5 days")),
        "{}",
        open.stdout
    );
    assert!(
        lines.iter().any(|line| line.contains("staff")
            && line.contains("cooling off")
            && line.contains("in 20 days")),
        "{}",
        open.stdout
    );
    assert!(!open.stdout.contains("downloaded"), "{}", open.stdout);
    assert!(
        open.stdout.contains("1 request is past its legal deadline"),
        "{}",
        open.stdout
    );
    assert!(
        open.stdout.contains("1 request is due within 7 days"),
        "{}",
        open.stdout
    );
    assert!(open.stdout.contains("jadpo deliveries"), "{}", open.stdout);
    let all = listed(&root, &url, &["privacy", "requests", "--all"]);
    assert!(all.stdout.contains("downloaded"), "{}", all.stdout);
    let closed = listed(
        &root,
        &url,
        &["privacy", "requests", "--state", "downloaded"],
    );
    assert!(closed.stdout.contains("1 downloaded"), "{}", closed.stdout);
    let json = listed(&root, &url, &["privacy", "requests", "--json"]);
    assert_eq!(json.code, 0, "{}{}", json.stdout, json.stderr);
    let document: serde_json::Value = serde_json::from_str(&json.stdout).expect("JSON");
    assert_eq!(document["data"]["overdue"], 1, "{document}");
    assert_eq!(
        document["data"]["requests"].as_array().map(Vec::len),
        Some(3),
        "{document}"
    );
    assert!(!json.stdout.contains('@'), "{}", json.stdout);
}
