//! Where personal data goes (area 15, "Marking personal fields" and "The
//! personal-data report"; area 11, "What a release shows"; chunk V5): a
//! field marked `personal:` on an entity, an event or a package's type,
//! and a standard type that is personal data whatever holds it, followed
//! by the walk area 21's check uses to every connection it leaves
//! through. Anything the walk can't tell about counts as reaching: a
//! report that says less than happens would be worse than none.

use jadpo_check::personal_data;
use jadpo_db::{Database, Layout};

use crate::support;

const APP: &str = "application {\n    connections: {\n        main: {\n            driver: Driver.postgres\n        }\n        pay: {\n            package: AcmePay.v1\n            domains: [billing]\n            credential: config.pay_api_key\n            environment: config.pay_environment\n        }\n    }\n    events: {\n        store: main\n    }\n}\n";

const CONFIG: &str = "config {\n    pay_api_key: Text {\n        secret: true\n    }\n    pay_environment: ProviderEnvironment\n}\n";

const HOME: &str = "shared/providers/acme_pay/v1";

/// The package's types, with `request` as `CustomerRequest`'s fields.
fn package(request: &str) -> Vec<(String, String)> {
    let at = |path: &str| format!("{HOME}/{path}");
    vec![
        (
            at("package.jadpo"),
            "/// Acme Pay's payments API.\npackage AcmePay.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n".to_string(),
        ),
        (
            at("provider.jadpo"),
            "/// Acme Pay's payments API.\nprovider {\n    protocol: Protocol.json_http\n    hosts: {\n        live: \"https://api.acme-pay.example\"\n        sandbox: \"https://sandbox.acme-pay.example\"\n    }\n    credential: CredentialScheme.bearer\n    idempotency: {\n        header: \"Idempotency-Key\"\n        window: 24h\n    }\n    error_code: \"error.code\"\n}\n".to_string(),
        ),
        (
            at("types/words.jadpo"),
            "/// Words Acme Pay keeps.\npublic type Words = Text {}\n".to_string(),
        ),
        (
            at("types/customer_request.jadpo"),
            format!("/// A customer to create.\npublic type CustomerRequest = Object {{\n{request}}}\n"),
        ),
        (
            at("types/customer_reply.jadpo"),
            "/// The customer Acme Pay made.\npublic type CustomerReply = Object {\n    id: Words\n    contact: Words {\n        personal: PersonalData.contact\n    }\n}\n".to_string(),
        ),
        (
            at("resources/customers.jadpo"),
            "public resource customers {\n    create: {\n        method: HttpMethod.post\n        path: \"/v1/customers\"\n        input: CustomerRequest\n        output: CustomerReply\n        changes: true\n        idempotent: true\n        source: PackageSource.written\n    }\n    look_up: {\n        method: HttpMethod.post\n        path: \"/v1/customers/search\"\n        input: CustomerRequest\n        output: CustomerReply\n        changes: false\n        source: PackageSource.written\n    }\n}\n".to_string(),
        ),
    ]
}

const PLAIN_REQUEST: &str = "    name: Words\n";

const WORDING: (&str, &str) = (
    "domains/billing/libraries/wording.jadpo",
    "use billing/entities { User }\nuse shared/providers/acme_pay\n\n/// Words of users'.\nlibrary wording {\n    /// The words as they are, through a call.\n    function shout(words: AcmePay.Words) -> AcmePay.Words {\n        return words\n    }\n\n    /// A user's name.\n    function name_of(user: User) -> AcmePay.Words {\n        return user.display_name\n    }\n}\n",
);

const USER: (&str, &str) = (
    "domains/billing/entities/user.jadpo",
    "use shared/providers/acme_pay\n\n/// Someone who pays.\nentity User {\n    id: Uuid\n    email: Email {\n        unique: true\n        personal: PersonalData.contact\n    }\n    display_name: AcmePay.Words {\n        personal: PersonalData.identifying\n    }\n    notes: AcmePay.Words {\n        personal: PersonalData.health\n    }\n    phone: PhoneNumber\n    plan: AcmePay.Words\n    person: true\n\n    policy: {\n        read: [Access.public]\n    }\n}\n",
);

/// `billing`'s action `send`, taking a user's id, whose body is `body`.
fn send(body: &str) -> String {
    format!(
        "use app {{ pay }}\nuse billing/entities {{ User, UserNotFound }}\nuse billing/libraries {{ wording }}\nuse shared/providers/acme_pay\n\n/// Sends a user to Acme Pay.\naction send(user_id: User.id) fails UserNotFound -> AcmePay.CustomerReply {{\n    var user: User = attempt User.get(user_id)\n{body}\n}}\n"
    )
}

/// A call of `customers.create` naming the customer `name`.
fn create(name: &str) -> String {
    format!(
        "    return pay.customers.create(AcmePay.CustomerRequest {{\n        name: {name}\n    }})"
    )
}

fn checked(request: &str, files: &[(&str, &str)]) -> (Database, jadpo_db::Project) {
    let package = package(request);
    let mut all: Vec<(&str, &str)> =
        vec![("app.jadpo", APP), ("config.jadpo", CONFIG), USER, WORDING];
    all.extend(
        package
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str())),
    );
    for (path, text) in files {
        match all.iter_mut().find(|(known, _)| known == path) {
            Some(file) => file.1 = text,
            None => all.push((path, text)),
        }
    }
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &all);
    (db, project)
}

/// Each problem but the unused, which these projects don't care about.
fn problems(db: &Database, project: jadpo_db::Project) -> Vec<String> {
    jadpo_check::check(db, project)
        .iter()
        .filter(|d| {
            !matches!(
                d.rule.id,
                "layout.unused_declaration"
                    | "import.unused"
                    | "semantic.unused_variable"
                    | "record.out_of_date"
            )
        })
        .map(|d| {
            let path = d
                .primary
                .as_ref()
                .map_or("", |label| label.span.path.as_str());
            format!("{path} {}: {}", d.rule.id, d.message)
        })
        .collect()
}

/// Each row of what leaves, as `connection holder through`, and what only
/// decided what was sent marked `decided`.
fn rows(db: &Database, project: jadpo_db::Project) -> Vec<String> {
    personal_data::sent(db, project)
        .iter()
        .map(|sent| {
            format!(
                "{} {} {}{}",
                sent.connection,
                sent.holder.key(),
                sent.through.named(),
                if sent.decided { " decided" } else { "" }
            )
        })
        .collect()
}

/// What leaves when `billing` holds `files`, which must check clean.
fn reach(request: &str, files: &[(&str, &str)]) -> Vec<String> {
    let (db, project) = checked(request, files);
    assert_eq!(problems(&db, project), Vec::<String>::new());
    rows(&db, project)
}

const SEND: &str = "domains/billing/actions/send.jadpo";

/// A marked field given to an operation reaches its connection, and a
/// field nobody marked doesn't.
#[test]
fn a_marked_field_sent_through_an_operation_reaches_its_connection() {
    let body = send(&create("user.display_name"));
    assert_eq!(
        reach(PLAIN_REQUEST, &[(SEND, &body)]),
        ["pay User.display_name customers.create"]
    );
    let body = send(&create("user.plan"));
    assert_eq!(reach(PLAIN_REQUEST, &[(SEND, &body)]), Vec::<String>::new());
}

/// A value keeps what it holds through variables, a function's call, text
/// built from it and a list.
#[test]
fn a_marked_field_is_followed_through_the_body() {
    for before in [
        "    var name: AcmePay.Words = user.display_name\n",
        "    var name: AcmePay.Words = wording.shout(user.display_name)\n",
        "    var name: AcmePay.Words = AcmePay.Words(text.trim(user.display_name))\n",
        "    var names: List<Text> = [text.trim(user.display_name)]\n    var name: AcmePay.Words = AcmePay.Words(text.join(names, \"\"))\n",
    ] {
        let body = send(&format!("{before}{}", create("name")));
        assert_eq!(
            reach(PLAIN_REQUEST, &[(SEND, &body)]),
            ["pay User.display_name customers.create"],
            "{before}"
        );
    }
}

/// A record passed whole to an action is followed to the field it reads;
/// passed to a library's function, nothing follows that code, so all of it
/// counts.
#[test]
fn a_record_passed_on_is_followed_as_far_as_can_be_told() {
    let named = (
        "domains/billing/actions/name_for.jadpo",
        "use billing/entities { User }\nuse shared/providers/acme_pay\n\n/// A user's name.\naction name_for(user: User) -> AcmePay.Words {\n    return user.display_name\n}\n",
    );
    let body = send(&create("name_for(user)")).replace(
        "use billing/entities",
        "use billing/actions { name_for }\nuse billing/entities",
    );
    assert_eq!(
        reach(PLAIN_REQUEST, &[named, (SEND, &body)]),
        ["pay User.display_name customers.create"]
    );
    let body = send(&create("wording.name_of(user)"));
    assert_eq!(
        reach(PLAIN_REQUEST, &[(SEND, &body)]),
        [
            "pay User.display_name customers.create",
            "pay User.email customers.create",
            "pay User.notes customers.create",
            "pay User.phone customers.create"
        ]
    );
}

const PASS_ON: &str = "domains/billing/actions/pass_on.jadpo";

/// An action sending what it is passed.
const PASSES_ON: &str = "use app { pay }\nuse shared/providers/acme_pay\n\n/// Sends what it is given.\naction pass_on(name: AcmePay.Words) -> AcmePay.CustomerReply {\n    return pay.customers.create(AcmePay.CustomerRequest {\n        name: name\n    })\n}\n";

fn calling_pass_on(name: &str) -> String {
    send(&format!("    return pass_on({name})")).replace(
        "use billing/entities",
        "use billing/actions { pass_on }\nuse billing/entities",
    )
}

/// A parameter stands for what every caller passes: one marked value is
/// followed, and an unmarked one leaves nothing.
#[test]
fn a_parameter_is_followed_to_what_every_caller_passes() {
    let body = calling_pass_on("user.notes");
    assert_eq!(
        reach(PLAIN_REQUEST, &[(PASS_ON, PASSES_ON), (SEND, &body)]),
        ["pay User.notes customers.create"]
    );
    let body = calling_pass_on("user.plan");
    assert_eq!(
        reach(PLAIN_REQUEST, &[(PASS_ON, PASSES_ON), (SEND, &body)]),
        Vec::<String>::new()
    );
}

/// What decides that a value is sent, or which one, is shown by sending
/// it, so it counts, marked as only having decided it.
#[test]
fn what_decided_a_value_leaves_with_it() {
    let under_if = send(&format!(
        "    if (user.notes == \"x\") {{\n{}\n    }}\n{}",
        create("user.plan"),
        create("user.plan"),
    ));
    assert_eq!(
        reach(PLAIN_REQUEST, &[(SEND, &under_if)]),
        ["pay User.notes customers.create decided"]
    );
    // A call made under the condition carries it too.
    let call = "    if (user.notes == \"x\") {\n        return pass_on(user.plan)\n    }\n    return pass_on(user.plan)";
    let body = send(call).replace(
        "use billing/entities",
        "use billing/actions { pass_on }\nuse billing/entities",
    );
    assert_eq!(
        reach(PLAIN_REQUEST, &[(PASS_ON, PASSES_ON), (SEND, &body)]),
        ["pay User.notes customers.create decided"]
    );
    // Sent as well as deciding, it is sent.
    let both = send(&format!(
        "    if (user.notes == \"x\") {{\n{}\n    }}\n{}",
        create("user.notes"),
        create("user.plan"),
    ));
    assert_eq!(
        reach(PLAIN_REQUEST, &[(SEND, &both)]),
        ["pay User.notes customers.create"]
    );
}

const MARKED_REQUEST: &str =
    "    name: Words {\n        personal: PersonalData.identifying\n    }\n";

/// A package's own type's marks count, whatever the app passes: the
/// package knows what its API does with the value.
#[test]
fn a_package_type_s_mark_counts_whatever_is_passed() {
    let body = send(&create("user.plan"));
    assert_eq!(
        reach(MARKED_REQUEST, &[(SEND, &body)]),
        ["pay AcmePay.CustomerRequest.name customers.create"]
    );
}

/// A type taking its fields from another keeps their marks.
#[test]
fn a_mark_follows_a_field_into_a_type_that_takes_it() {
    let base = (
        "shared/providers/acme_pay/v1/types/contact_details.jadpo",
        "/// How to reach someone.\npublic type ContactDetails = Object {\n    name: Words {\n        personal: PersonalData.contact\n    }\n}\n",
    );
    let body = send(&create("\"x\""));
    let request = "    include: [ContactDetails]\n";
    assert_eq!(
        reach(request, &[base, (SEND, &body)]),
        ["pay AcmePay.ContactDetails.name customers.create"]
    );
    // A local holding that type, read for the field.
    let looked = send(
        "    var made: AcmePay.CustomerRequest = AcmePay.CustomerRequest {\n        name: \"x\"\n    }\n    return pay.customers.create(AcmePay.CustomerRequest {\n        name: made.name\n    })",
    );
    assert_eq!(
        reach(request, &[base, (SEND, &looked)]),
        ["pay AcmePay.ContactDetails.name customers.create"]
    );
}

/// A marked field of an operation's answer is personal data when it is
/// sent on, and the answer carries everything its call was given.
#[test]
fn a_provider_s_answer_keeps_what_it_holds() {
    let body = send(&format!(
        "    var made: AcmePay.CustomerReply = pay.customers.look_up(AcmePay.CustomerRequest {{\n        name: user.plan\n    }})\n{}",
        create("made.contact")
    ));
    assert_eq!(
        reach(PLAIN_REQUEST, &[(SEND, &body)]),
        ["pay AcmePay.CustomerReply.contact customers.create"]
    );
    let body = send(&format!(
        "    var made: AcmePay.CustomerReply = pay.customers.look_up(AcmePay.CustomerRequest {{\n        name: user.notes\n    }})\n{}",
        create("made.id")
    ));
    assert_eq!(
        reach(PLAIN_REQUEST, &[(SEND, &body)]),
        [
            "pay User.notes customers.create",
            "pay User.notes customers.look_up"
        ]
    );
}

const START: (&str, &str) = (
    "shared/providers/acme_pay/v1/actions/start.jadpo",
    "/// Creates a customer.\npublic action start(input: CustomerRequest) -> CustomerReply {\n    return customers.create(input)\n}\n",
);

/// Everything a package's action is passed leaves through the connection
/// it is called through, with its own operations named too.
#[test]
fn a_package_action_sends_what_it_is_passed() {
    let body =
        send("    return pay.start(AcmePay.CustomerRequest {\n        name: user.notes\n    })");
    assert_eq!(
        reach(PLAIN_REQUEST, &[START, (SEND, &body)]),
        ["pay User.notes customers.create", "pay User.notes start"]
    );
    // One that doesn't send what it is passed still counts: nothing says
    // it won't start to, and the report never says less than happens.
    let ignores = (
        "shared/providers/acme_pay/v1/actions/start.jadpo",
        "/// Creates a customer of no name.\npublic action start(input: CustomerRequest) -> CustomerReply {\n    return customers.create(CustomerRequest {\n        name: \"\"\n    })\n}\n",
    );
    assert_eq!(
        reach(PLAIN_REQUEST, &[ignores, (SEND, &body)]),
        ["pay User.notes start"]
    );
    // A package's marks come with it, its own operation's too.
    let plain =
        send("    return pay.start(AcmePay.CustomerRequest {\n        name: user.plan\n    })");
    assert_eq!(
        reach(MARKED_REQUEST, &[START, (SEND, &plain)]),
        [
            "pay AcmePay.CustomerRequest.name customers.create",
            "pay AcmePay.CustomerRequest.name start"
        ]
    );
}

const DUE: &str = "domains/billing/events/payment_due.jadpo";
const ANNOUNCE: &str = "domains/billing/actions/announce.jadpo";
const HEARD: (&str, &str) = (
    "domains/billing/subscribers/heard.jadpo",
    "use billing/actions { pass_on }\nuse billing/events { PaymentDue }\n\n/// Sends each payment due.\nsubscriber heard {\n    on: PaymentDue\n    run: pass_on(event.who)\n}\n",
);

fn due(marked: bool) -> String {
    format!(
        "use shared/providers/acme_pay\n\n/// A payment is due.\ntype PaymentDue = Event {{\n    who: AcmePay.Words{}\n}}\n",
        if marked {
            " {\n        personal: PersonalData.identifying\n    }"
        } else {
            ""
        }
    )
}

fn announce(who: &str) -> String {
    format!(
        "use billing/entities {{ User, UserNotFound }}\nuse billing/events {{ PaymentDue }}\nuse shared/providers/acme_pay\n\n/// Says a payment is due.\naction announce(user_id: User.id) fails UserNotFound -> Unit {{\n    var user: User = attempt User.get(user_id)\n    emit PaymentDue {{\n        who: {who}\n    }}\n}}\n"
    )
}

/// A subscriber's action sends what every `emit` of its event put there,
/// and the event's own marks.
#[test]
fn an_event_s_part_is_followed_through_its_subscribers() {
    let check = |marked: bool, who: &str| {
        let due = due(marked);
        let announce = announce(who);
        reach(
            PLAIN_REQUEST,
            &[
                (DUE, due.as_str()),
                (ANNOUNCE, announce.as_str()),
                HEARD,
                (PASS_ON, PASSES_ON),
            ],
        )
    };
    assert_eq!(check(false, "user.plan"), Vec::<String>::new());
    assert_eq!(
        check(false, "user.notes"),
        ["pay User.notes customers.create"]
    );
    assert_eq!(
        check(true, "user.plan"),
        ["pay PaymentDue.who customers.create"]
    );
}

/// A query's `Output` is followed to the fields it was read from.
#[test]
fn a_query_s_output_is_followed_to_its_fields() {
    let card = (
        "domains/billing/types/user_card.jadpo",
        "use billing/entities { User }\n\n/// A user as a list shows them.\ntype UserCard = Output {\n    pick: [User.plan, User.notes]\n}\n",
    );
    let query = (
        "domains/billing/queries/cards.jadpo",
        "use billing/entities { User }\nuse billing/types { UserCard }\n\n/// Every user's card.\nquery cards() -> List<UserCard> {\n    from: User\n    order: [User.plan asc]\n    limit: 50\n}\n",
    );
    let check = |field: &str| {
        let body = send(&format!(
            "    var all: List<UserCard> = cards()\n    var lines: List<Text> = list.map(all, card -> text.trim(card.{field}))\n{}",
            create("AcmePay.Words(text.join(lines, \"\"))")
        ))
        .replace(
            "use billing/entities",
            "use billing/queries { cards }\nuse billing/types { UserCard }\nuse billing/entities",
        );
        reach(PLAIN_REQUEST, &[card, query, (SEND, &body)])
    };
    assert_eq!(check("plan"), Vec::<String>::new());
    assert_eq!(check("notes"), ["pay User.notes customers.create"]);
}

const PHONE_REQUEST: &str = "    name: Words\n    phone: PhoneNumber\n";

/// A standard type that is personal data keeps its category through a
/// local variable and text made from it.
#[test]
fn a_standard_type_is_followed_through_text_made_from_it() {
    let ring = "use app { pay }\nuse shared/providers/acme_pay\n\n/// Rings a number.\naction ring(number: PhoneNumber) -> AcmePay.CustomerReply {\n    var dialled: PhoneNumber = number\n    return pay.customers.create(AcmePay.CustomerRequest {\n        name: AcmePay.Words(json.write(dialled))\n    })\n}\n";
    assert_eq!(
        reach(
            PLAIN_REQUEST,
            &[("domains/billing/actions/ring.jadpo", ring)]
        ),
        ["pay PhoneNumber customers.create"]
    );
}

/// A value of a standard type that is personal data counts wherever it
/// comes from: a request's own, with nothing marked, or a field holding
/// one, which is named.
#[test]
fn a_standard_type_that_is_personal_data_counts() {
    let ring = "use app { pay }\nuse shared/providers/acme_pay\n\n/// Rings a number.\naction ring(number: PhoneNumber) -> AcmePay.CustomerReply {\n    return pay.customers.create(AcmePay.CustomerRequest {\n        name: \"x\"\n        phone: number\n    })\n}\n";
    assert_eq!(
        reach(
            PHONE_REQUEST,
            &[("domains/billing/actions/ring.jadpo", ring)]
        ),
        ["pay PhoneNumber customers.create"]
    );
    let body = send(
        "    return pay.customers.create(AcmePay.CustomerRequest {\n        name: user.plan\n        phone: user.phone\n    })",
    );
    assert_eq!(
        reach(PHONE_REQUEST, &[(SEND, &body)]),
        ["pay User.phone customers.create"]
    );
}

/// A value whose type is a marked field's counts as that field, though no
/// caller in the app passes it: a route's input, say.
#[test]
fn a_value_typed_as_a_marked_field_counts_as_it() {
    let rename = "use app { pay }\nuse billing/entities { User }\nuse shared/providers/acme_pay\n\n/// Renames a customer.\naction rename(name: User.display_name) -> AcmePay.CustomerReply {\n    return pay.customers.create(AcmePay.CustomerRequest {\n        name: name\n    })\n}\n";
    assert_eq!(
        reach(
            PLAIN_REQUEST,
            &[("domains/billing/actions/rename.jadpo", rename)]
        ),
        ["pay User.display_name customers.create"]
    );
    // And so does a field read off a parameter of a type with marks.
    let answered = "use app { pay }\nuse shared/providers/acme_pay\n\n/// Sends the contact of a customer.\naction answered(reply: AcmePay.CustomerReply) -> AcmePay.CustomerReply {\n    return pay.customers.create(AcmePay.CustomerRequest {\n        name: reply.contact\n    })\n}\n";
    assert_eq!(
        reach(
            PLAIN_REQUEST,
            &[("domains/billing/actions/answered.jadpo", answered)]
        ),
        ["pay AcmePay.CustomerReply.contact customers.create"]
    );
}

/// A request's whole input, with no caller in the app to follow it to,
/// counts for what its type holds, however it is passed on.
#[test]
fn a_parameter_with_no_caller_counts_for_what_its_type_holds() {
    let input = (
        "domains/billing/types/sign_up.jadpo",
        "use billing/entities { User }\n\n/// Who is signing up.\ntype SignUp = Input {\n    phone: PhoneNumber\n    pick: [User.display_name]\n}\n",
    );
    let join = (
        "domains/billing/actions/join.jadpo",
        "use app { pay }\nuse billing/types { SignUp }\nuse shared/providers/acme_pay\n\n/// Tells Acme Pay of someone signing up.\naction join(sign_up: SignUp) -> AcmePay.CustomerReply {\n    return pay.customers.create(AcmePay.CustomerRequest {\n        name: AcmePay.Words(json.write(sign_up))\n    })\n}\n",
    );
    assert_eq!(
        reach(PLAIN_REQUEST, &[input, join]),
        [
            "pay User.display_name customers.create",
            "pay PhoneNumber customers.create"
        ]
    );
    // One part of it counts for that part.
    let join = (
        "domains/billing/actions/join.jadpo",
        "use app { pay }\nuse billing/types { SignUp }\nuse shared/providers/acme_pay\n\n/// Tells Acme Pay of someone signing up.\naction join(sign_up: SignUp) -> AcmePay.CustomerReply {\n    return pay.customers.create(AcmePay.CustomerRequest {\n        name: sign_up.display_name\n    })\n}\n",
    );
    assert_eq!(
        reach(PLAIN_REQUEST, &[input, join]),
        ["pay User.display_name customers.create"]
    );
}

/// `caller.subject` is the field `principal { }` names.
#[test]
fn the_caller_s_subject_counts_as_its_field() {
    let principal = "use billing/entities { User }\n\napplication {\n    connections: {\n        main: {\n            driver: Driver.postgres\n        }\n        pay: {\n            package: AcmePay.v1\n            domains: [billing]\n            credential: config.pay_api_key\n            environment: config.pay_environment\n        }\n    }\n    events: {\n        store: main\n    }\n}\n\nprincipal {\n    user: {\n        subject: User.email\n    }\n}\n";
    let tell = "use app { pay }\nuse shared/providers/acme_pay\n\n/// Tells Acme Pay who is calling.\naction tell(caller: Principal.user) -> AcmePay.CustomerReply {\n    return pay.customers.create(AcmePay.CustomerRequest {\n        name: \"x\"\n        who: caller.subject\n    })\n}\n";
    let (db, project) = checked(
        "    name: Words\n    who: Email\n",
        &[
            ("app.jadpo", principal),
            ("domains/billing/actions/tell.jadpo", tell),
        ],
    );
    assert_eq!(
        (problems(&db, project), rows(&db, project)),
        (
            Vec::<String>::new(),
            vec!["pay User.email customers.create".to_string()]
        )
    );
}

/// An action run for each item of a batch sends from what the run read.
#[test]
fn a_batch_s_action_is_followed() {
    let page = (
        "domains/billing/types/user_page.jadpo",
        "/// A page of users.\ntype UserPage = Input {\n    page_size: PageSize {\n        default: 25\n    }\n    after: PageCursor {\n        optional: true\n    }\n}\n",
    );
    let users = (
        "domains/billing/queries/users.jadpo",
        "use billing/entities { User }\nuse billing/types { UserPage }\n\n/// A page of users.\nquery users(page: UserPage) -> Page<User> {\n    from: User\n    order: [User.id asc]\n    page: {\n        size: page.page_size\n        after: page.after\n    }\n}\n",
    );
    let sweep = (
        "domains/billing/actions/sweep.jadpo",
        "use billing/actions { pass_on }\nuse billing/queries { users }\nuse billing/types { UserPage }\n\n/// Sends every user.\naction sweep(page: UserPage) -> BatchRun {\n    return batches.each(\n        from: users(page),\n        each: user -> pass_on(user.notes)\n    )\n}\n",
    );
    let user = USER.1.replace(
        "read: [Access.public]",
        "read: [Access.public, Access.system]",
    );
    assert_eq!(
        reach(
            PLAIN_REQUEST,
            &[
                page,
                users,
                sweep,
                (PASS_ON, PASSES_ON),
                (USER.0, user.as_str())
            ]
        ),
        ["pay User.notes customers.create"]
    );
}

/// A file put in a store, and the rows a writer puts in a file.
#[test]
fn a_file_goes_to_its_store() {
    let app = APP.replace(
        "        pay: {",
        "        papers: {\n            driver: Driver.disk\n            path: \"/var/lib/papers\"\n        }\n        pay: {",
    );
    let keep = "use app { papers }\nuse billing/entities { User, UserNotFound }\n\n/// Keeps a user's notes as a file.\naction keep(user_id: User.id) fails UserNotFound -> File {\n    var user: User = attempt User.get(user_id)\n    return papers.put(encoding.to_utf8(user.notes), FileType.csv)\n}\n";
    let (db, project) = checked(
        PLAIN_REQUEST,
        &[
            ("app.jadpo", app.as_str()),
            ("domains/billing/actions/keep.jadpo", keep),
        ],
    );
    assert_eq!(rows(&db, project), ["papers User.notes a file"]);

    let page = (
        "domains/billing/types/user_page.jadpo",
        "/// A page of users.\ntype UserPage = Input {\n    page_size: PageSize {\n        default: 25\n    }\n    after: PageCursor {\n        optional: true\n    }\n}\n",
    );
    let users = (
        "domains/billing/queries/users.jadpo",
        "use billing/entities { User }\nuse billing/types { UserPage }\n\n/// A page of users.\nquery users(page: UserPage) -> Page<User> {\n    from: User\n    order: [User.id asc]\n    page: {\n        size: page.page_size\n        after: page.after\n    }\n}\n",
    );
    let line = (
        "domains/billing/types/user_line.jadpo",
        "use shared/providers/acme_pay\n\n/// One user in a file.\ntype UserLine = Output {\n    what: AcmePay.Words\n}\n",
    );
    let dump = (
        "domains/billing/actions/dump.jadpo",
        "use app { papers }\nuse billing/queries { users }\nuse billing/types { UserLine, UserPage }\n\n/// Writes every user to a file.\naction dump(page: UserPage) -> BatchRun {\n    return json_lines.write(\n        papers,\n        name: \"users.jsonl\",\n        from: users(page),\n        each: user -> UserLine {\n            what: user.notes\n        }\n    )\n}\n",
    );
    let user = USER.1.replace(
        "read: [Access.public]",
        "read: [Access.public, Access.system]",
    );
    let (db, project) = checked(
        PLAIN_REQUEST,
        &[
            ("app.jadpo", app.as_str()),
            (USER.0, user.as_str()),
            page,
            users,
            line,
            dump,
        ],
    );
    assert_eq!(rows(&db, project), ["papers User.notes a file"]);
}

const MAIL_APP: &str = "application {\n    display_name: \"Pay\"\n    connections: {\n        main: {\n            driver: Driver.postgres\n        }\n        mail: {\n            driver: Driver.smtp\n            host: \"smtp.pay.example\"\n        }\n        pay: {\n            package: AcmePay.v1\n            domains: [billing]\n            credential: config.pay_api_key\n            environment: config.pay_environment\n        }\n    }\n    events: {\n        store: main\n    }\n    email: {\n        connection: mail\n        from: \"hello@pay.example\"\n    }\n}\n";

/// An email goes through the connection `email:` names, with what its
/// fields were given and its own marks.
#[test]
fn an_email_goes_through_the_connection_email_names() {
    let welcome = (
        "domains/billing/emails/welcome_email.jadpo",
        "use shared/providers/acme_pay\n\n/// Welcomes someone.\ntype WelcomeEmail = EmailMessage {\n    name: AcmePay.Words\n}\n",
    );
    let html = (
        "domains/billing/emails/welcome_email.html",
        "<p>Hello {{ name }}</p>\n",
    );
    let txt = (
        "domains/billing/emails/welcome_email.txt",
        "Hello {{ name }}\n",
    );
    let welcomes = (
        "domains/billing/actions/welcomes.jadpo",
        "use billing/emails { WelcomeEmail }\nuse billing/entities { User, UserNotFound }\n\n/// Welcomes a user.\naction welcomes(user_id: User.id) fails UserNotFound -> Unit {\n    var user: User = attempt User.get(user_id)\n    emit WelcomeEmail {\n        to: user.email\n        locale: Locale(\"en\")\n        subject: \"Welcome\"\n        name: user.display_name\n    }\n}\n",
    );
    let (db, project) = checked(
        PLAIN_REQUEST,
        &[("app.jadpo", MAIL_APP), welcome, html, txt, welcomes],
    );
    assert_eq!(
        rows(&db, project),
        [
            "mail User.display_name email WelcomeEmail",
            "mail User.email email WelcomeEmail"
        ]
    );
}

/// A magic link's email takes the address to the email connection, or to
/// wherever the application's own `send:` action sends it.
#[test]
fn a_magic_link_takes_its_address_to_the_mailer() {
    let config = "config {\n    browser_origin: Url\n    pay_api_key: Text {\n        secret: true\n    }\n    pay_environment: ProviderEnvironment\n}\n";
    let app = format!(
        "use billing/entities {{ User }}\n\n{MAIL_APP}\nprincipal {{\n    user: {{\n        subject: User.email\n    }}\n}}\n\nauthentication {{\n    browser_session: {{\n        cookie: \"session\"\n        origin: config.browser_origin\n    }}\n    sign_in: {{\n        magic_link: {{\n            link: \"/sign-in/confirm\"\n        }}\n    }}\n}}\n"
    );
    let (db, project) = checked(
        PLAIN_REQUEST,
        &[("app.jadpo", app.as_str()), ("config.jadpo", config)],
    );
    assert_eq!(rows(&db, project), ["mail User.email email MagicLinkEmail"]);
}

/// Marks: read on a package's own type, against the same categories, and
/// refused on an app's own type, which holds what a marked field gave it.
#[test]
fn a_package_type_s_mark_is_checked_where_it_is_written() {
    let (db, project) = checked(
        "    name: Words {\n        personal: PersonalData.nonsense\n    }\n",
        &[],
    );
    assert_eq!(
        problems(&db, project),
        [
            "shared/providers/acme_pay/v1/types/customer_request.jadpo type.setting_value: `personal` takes one of area 15's categories"
        ]
    );
    let card = (
        "domains/billing/types/card.jadpo",
        "use shared/providers/acme_pay\n\n/// A card.\ntype Card = Object {\n    name: AcmePay.Words {\n        personal: PersonalData.identifying\n    }\n}\n",
    );
    let (db, project) = checked(PLAIN_REQUEST, &[card]);
    assert_eq!(
        problems(&db, project),
        [
            "domains/billing/types/card.jadpo type.unknown_setting: `personal` isn't a setting of `Card`'s fields"
        ]
    );
    // `to_models:` stays an entity's and an event's.
    let (db, project) = checked("    name: Words {\n        to_models: false\n    }\n", &[]);
    assert_eq!(
        problems(&db, project),
        [
            "shared/providers/acme_pay/v1/types/customer_request.jadpo type.unknown_setting: `to_models` isn't a setting Jadpo knows"
        ]
    );
}

/// A package's type of the same name as an entity's marks only itself.
#[test]
fn a_package_type_s_mark_stays_on_that_type() {
    let reply = "shared/providers/acme_pay/v1/types/customer.jadpo";
    let customer = "/// A customer as Acme Pay holds them.\npublic type Customer = Object {\n    name: Words {\n        personal: PersonalData.identifying\n    }\n}\n";
    let entity = (
        "domains/billing/entities/customer.jadpo",
        "use shared/providers/acme_pay\n\n/// A customer's shop, which is no person.\nentity Customer {\n    id: Uuid\n    name: AcmePay.Words\n\n    policy: {\n        read: [Access.public]\n    }\n}\n",
    );
    let (db, project) = checked(PLAIN_REQUEST, &[(reply, customer), entity]);
    assert_eq!(problems(&db, project), Vec::<String>::new());
}

/// What a package sends, whoever calls it: each operation's input as its
/// type declares it, what its actions send, and the hosts.
#[test]
fn a_package_s_own_facts_come_from_its_types_and_actions() {
    let (db, project) = checked(MARKED_REQUEST, &[START]);
    assert_eq!(problems(&db, project), Vec::<String>::new());
    let (hosts, sends) = personal_data::package_sends(&db, project, HOME);
    assert_eq!(
        hosts,
        [
            "https://api.acme-pay.example",
            "https://sandbox.acme-pay.example"
        ]
    );
    let lines: Vec<String> = sends
        .iter()
        .map(|send| {
            format!(
                "{} {} {}",
                send.operation,
                send.holder.key(),
                send.categories.join(",")
            )
        })
        .collect();
    assert_eq!(
        lines,
        [
            "customers.create AcmePay.CustomerRequest.name identifying",
            "customers.look_up AcmePay.CustomerRequest.name identifying"
        ]
    );
    // An action building an input from what it read off another type of
    // the package's.
    let plain = (
        "shared/providers/acme_pay/v1/actions/start.jadpo",
        "/// Creates a customer from one Acme Pay holds.\npublic action start(reply: CustomerReply) -> CustomerReply {\n    return customers.create(CustomerRequest {\n        name: reply.contact\n    })\n}\n",
    );
    let (db, project) = checked(PLAIN_REQUEST, &[plain]);
    assert_eq!(problems(&db, project), Vec::<String>::new());
    let (_, sends) = personal_data::package_sends(&db, project, HOME);
    let lines: Vec<String> = sends
        .iter()
        .map(|send| format!("{} {}", send.operation, send.holder.key()))
        .collect();
    assert_eq!(lines, ["customers.create AcmePay.CustomerReply.contact"]);
}

/// A package with no marks and no personal type sends nothing to report.
#[test]
fn a_package_with_no_personal_data_has_no_facts() {
    let (db, project) = checked(PLAIN_REQUEST, &[]);
    let (_, sends) = personal_data::package_sends(&db, project, HOME);
    assert!(sends.is_empty());
}

/// A webhook's reply goes back to the connection it came from, with what
/// the reply holds and what decided whether the action failed.
#[test]
fn a_webhook_s_reply_goes_back_to_its_connection() {
    let app = "use support\n\napplication {\n    connections: {\n        main: {\n            driver: Driver.postgres\n        }\n        phone: {\n            package: AcmeVoice.v1\n            domains: [support]\n            credential: config.phone_key\n            host: config.phone_address\n            webhooks: {\n                secret: config.phone_auth_token\n                replies: {\n                    incoming_call: support.answer_call\n                }\n            }\n        }\n    }\n    events: {\n        store: main\n    }\n}\n";
    let config = "config {\n    phone_address: Url\n    phone_auth_token: Text {\n        secret: true\n    }\n    phone_key: Text {\n        secret: true\n    }\n}\n";
    let home = "shared/providers/acme_voice/v1";
    let package = "/// Acme Voice's calls API.\npackage AcmeVoice.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n";
    let provider = "/// Acme Voice's API, which signs its webhooks as Twilio does.\nprovider {\n    protocol: Protocol.json_http\n    credential: CredentialScheme.bearer\n    webhooks: {\n        check: SignatureScheme.twilio\n        event_id: \"CallSid\"\n        event_type: \"Direction\"\n    }\n}\n";
    let reply = "/// What a caller hears.\npublic type CallReply = Object {\n    spoken: Text\n}\n";
    let event = "/// A call is ringing.\npublic type IncomingCall = Event {\n    wire_name: \"ringing\"\n    reply: CallReply\n    occurred_at: Instant\n    call_sid: Text {\n        wire_name: \"CallSid\"\n    }\n}\n";
    let member = "/// Someone at the desk.\nentity Member {\n    id: Uuid\n    name: MemberName {\n        personal: PersonalData.identifying\n    }\n    person: true\n\n    policy: {\n        read: [Access.system]\n    }\n}\n\n/// A member's name.\ntype MemberName = Text {\n    max_length: 80\n}\n";
    let listed = "use support/entities { Member }\n\n/// Who is on the desk.\nquery on_desk() -> List<Member> {\n    from: Member\n    order: [Member.id asc]\n    limit: 1\n}\n";
    let answer = "use support/queries { on_desk }\nuse shared/providers/acme_voice\n\n/// Says who is on the desk.\naction answer_call(event: AcmeVoice.IncomingCall) -> AcmeVoice.CallReply {\n    var desk: List<Member> = on_desk()\n    return AcmeVoice.CallReply {\n        spoken: text.join(list.map(desk, one -> text.trim(one.name)), \"\")\n    }\n}\n";
    let answer = answer.replace(
        "use support/queries",
        "use support/entities { Member }\nuse support/queries",
    );
    let at = |path: &str| format!("{home}/{path}");
    let (p1, p2, p3, p4) = (
        at("package.jadpo"),
        at("provider.jadpo"),
        at("types/call_reply.jadpo"),
        at("events/incoming_call.jadpo"),
    );
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Project,
        &[
            ("app.jadpo", app),
            ("config.jadpo", config),
            (&p1, package),
            (&p2, provider),
            (&p3, reply),
            (&p4, event),
            ("domains/support/entities/member.jadpo", member),
            ("domains/support/queries/on_desk.jadpo", listed),
            ("domains/support/actions/answer_call.jadpo", answer.as_str()),
        ],
    );
    assert_eq!(rows(&db, project), ["phone Member.name reply answer_call"]);
}

/// `send:` keeps the magic link's address in its own action: followed to
/// wherever that action sends it, and no email of Jadpo's is sent.
#[test]
fn a_magic_link_s_own_sender_is_followed() {
    let config = "config {\n    browser_origin: Url\n    pay_api_key: Text {\n        secret: true\n    }\n    pay_environment: ProviderEnvironment\n}\n";
    let app = format!(
        "{MAIL_APP}\nprincipal {{\n    user: {{\n        subject: User.email\n    }}\n}}\n\nauthentication {{\n    browser_session: {{\n        cookie: \"session\"\n        origin: config.browser_origin\n    }}\n    sign_in: {{\n        magic_link: {{\n            link: \"/sign-in/confirm\"\n            send: billing.send_link\n        }}\n    }}\n}}\n"
    );
    let app = format!("use billing/entities {{ User }}\nuse billing\n\n{app}");
    let sender = "use app { pay }\nuse shared/providers/acme_pay\n\n/// Sends a sign-in link through Acme Pay.\naction send_link(request: MagicLinkRequest) -> Unit {\n    var made: AcmePay.CustomerReply = pay.customers.create(AcmePay.CustomerRequest {\n        name: \"x\"\n        who: request.address\n    })\n    assert made.id == made.id\n}\n";
    let (db, project) = checked(
        "    name: Words\n    who: Email\n",
        &[
            ("app.jadpo", app.as_str()),
            ("config.jadpo", config),
            ("domains/billing/actions/send_link.jadpo", sender),
        ],
    );
    assert_eq!(
        (problems(&db, project), rows(&db, project)),
        (
            Vec::<String>::new(),
            vec!["pay User.email customers.create".to_string()]
        )
    );
}

/// A package's own subscriber goes through each connection to the package,
/// with what its event's marks say.
#[test]
fn a_package_s_subscriber_goes_through_each_connection_to_it() {
    let event = (
        "shared/providers/acme_pay/v1/events/customer_seen.jadpo",
        "/// Acme Pay saw a customer.\npublic type CustomerSeen = Event {\n    wire_name: \"customer.seen\"\n    occurred_at: Instant\n    contact: Words {\n        personal: PersonalData.contact\n    }\n}\n",
    );
    let provider = (
        "shared/providers/acme_pay/v1/provider.jadpo",
        "/// Acme Pay's payments API.\nprovider {\n    protocol: Protocol.json_http\n    hosts: {\n        live: \"https://api.acme-pay.example\"\n        sandbox: \"https://sandbox.acme-pay.example\"\n    }\n    credential: CredentialScheme.bearer\n    idempotency: {\n        header: \"Idempotency-Key\"\n        window: 24h\n    }\n    error_code: \"error.code\"\n    webhooks: {\n        check: SignatureScheme.stripe\n        event_id: \"id\"\n        event_type: \"type\"\n    }\n}\n",
    );
    let looked = (
        "shared/providers/acme_pay/v1/actions/look_again.jadpo",
        "/// Looks a customer up again.\naction look_again(contact: Words) -> Unit {\n    var found: CustomerReply = customers.look_up(CustomerRequest {\n        name: contact\n    })\n    assert found.id == found.id\n}\n",
    );
    let heard = (
        "shared/providers/acme_pay/v1/subscribers/seen.jadpo",
        "/// Looks up each customer seen.\nsubscriber seen {\n    on: CustomerSeen\n    run: look_again(event.contact)\n}\n",
    );
    let app = APP.replace(
        "            environment: config.pay_environment\n",
        "            environment: config.pay_environment\n            webhooks: {\n                secret: config.pay_secret\n            }\n",
    );
    let config = CONFIG.replace(
        "    pay_environment: ProviderEnvironment\n",
        "    pay_environment: ProviderEnvironment\n    pay_secret: Text {\n        secret: true\n    }\n",
    );
    let (db, project) = checked(
        PLAIN_REQUEST,
        &[
            ("app.jadpo", app.as_str()),
            ("config.jadpo", config.as_str()),
            event,
            provider,
            looked,
            heard,
        ],
    );
    assert_eq!(
        rows(&db, project),
        ["pay AcmePay.CustomerSeen.contact customers.look_up"]
    );
    let (_, sends) = personal_data::package_sends(&db, project, HOME);
    let lines: Vec<String> = sends
        .iter()
        .map(|send| format!("{} {}", send.operation, send.holder.key()))
        .collect();
    assert_eq!(lines, ["customers.look_up AcmePay.CustomerSeen.contact"]);
}

/// Actions that call each other: everything that may flow round the cycle
/// counts, however many times round.
#[test]
fn a_cycle_of_actions_is_followed_until_nothing_more_is_found() {
    let ping = (
        "domains/billing/actions/ping.jadpo",
        "use app { pay }\nuse billing/actions { pong }\nuse shared/providers/acme_pay\n\n/// Passes a name on, or sends it.\naction ping(name: AcmePay.Words, again: Bool) -> AcmePay.CustomerReply {\n    if (again) {\n        return pong(name)\n    }\n    return pay.customers.create(AcmePay.CustomerRequest {\n        name: name\n    })\n}\n",
    );
    let pong = (
        "domains/billing/actions/pong.jadpo",
        "use billing/actions { ping }\nuse shared/providers/acme_pay\n\n/// Passes a name back.\naction pong(name: AcmePay.Words) -> AcmePay.CustomerReply {\n    return ping(name, false)\n}\n",
    );
    let body = send("    return pong(user.notes)").replace(
        "use billing/entities",
        "use billing/actions { pong }\nuse billing/entities",
    );
    let (db, project) = checked(PLAIN_REQUEST, &[ping, pong, (SEND, &body)]);
    assert!(
        rows(&db, project)
            .iter()
            .any(|row| row == "pay User.notes customers.create"),
        "{:?} {:?}",
        rows(&db, project),
        problems(&db, project)
    );
}

/// A package's own mark counts in the check of what reaches a model too,
/// and a provider's answer doesn't make its ids personal data there.
#[test]
fn a_package_type_s_mark_reaches_area_21_s_check() {
    let app = APP.replace(
        "    connections: {\n",
        "    connections: {\n        local_model: {\n            driver: Driver.openai_compatible\n            url: config.model_url\n        }\n",
    )
    .replace(
        "    events: {",
        "    ai: {\n        connection: local_model\n        models: {\n            default: config.drafting_model\n        }\n        limits: {\n            per_day: {\n                tokens: 2000000\n            }\n        }\n    }\n    events: {",
    );
    let config = CONFIG.replace(
        "config {\n",
        "config {\n    model_url: Url {\n        default: \"http://localhost:11434/v1\"\n    }\n    drafting_model: ModelId {\n        default: \"local/llama3.2\"\n    }\n",
    );
    let prompt = "use shared/providers/acme_pay\n\n/// Says hello.\ntype SayHello = Prompt {\n    name: AcmePay.Words\n}\n";
    let answer = "use shared/providers/acme_pay\n\n/// A hello.\ntype Hello = Output {\n    words: AcmePay.Words\n}\n";
    let failure = "/// The model can't answer now.\ntype HelloUnavailable = Failure {\n    kind: FailureKind.unavailable\n}\n";
    let body = |value: &str| {
        send(&format!(
            "    var made: AcmePay.CustomerReply = pay.customers.create(AcmePay.CustomerRequest {{\n        name: user.notes\n    }})\n    var said: Hello = attempt ai.generate(SayHello {{ name: {value} }}, as: Hello) {{\n        refused: HelloUnavailable\n        malformed: HelloUnavailable\n        timed_out: HelloUnavailable\n        over_budget: HelloUnavailable\n        too_long: HelloUnavailable\n    }}\n    return made"
        ))
        .replace(
            "use billing/entities",
            "use billing/failures { HelloUnavailable }\nuse billing/prompts { SayHello }\nuse billing/types { Hello }\nuse billing/entities",
        )
        .replace("fails UserNotFound", "fails UserNotFound, HelloUnavailable")
    };
    let check = |value: &str| {
        let body = body(value);
        let prompt_text = (
            "domains/billing/prompts/say_hello.prompt",
            "# Instructions\n\nSay hello.\n\n# Request\n\n{name}\n",
        );
        let package = package(PLAIN_REQUEST);
        let mut all: Vec<(&str, &str)> = vec![
            ("app.jadpo", app.as_str()),
            ("config.jadpo", config.as_str()),
            USER,
            WORDING,
            ("domains/billing/prompts/say_hello.jadpo", prompt),
            ("domains/billing/types/hello.jadpo", answer),
            ("domains/billing/failures/hello_unavailable.jadpo", failure),
            (SEND, body.as_str()),
        ];
        all.extend(
            package
                .iter()
                .map(|(path, text)| (path.as_str(), text.as_str())),
        );
        let db = Database::default();
        let project =
            support::project_with_migrated_records(&db, Layout::Project, &all, &[prompt_text]);
        problems(&db, project)
    };
    assert_eq!(check("made.id"), Vec::<String>::new());
    assert_eq!(
        check("made.contact"),
        [
            "domains/billing/actions/send.jadpo semantic.ai_personal_data_not_allowed: `CustomerReply.contact` reaches a model, and `ai:` doesn't allow `contact` data"
        ]
    );
}
