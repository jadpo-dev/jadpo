//! Diagnostics that point at the cause (roadmap A1): the shapes an
//! outside agent's first app ran into, each one cause giving one
//! diagnostic, at the cause, with an edit where the fix is mechanical.

use crate::support::{self, problems};

use jadpo_db::{Database, Layout};

/// Every problem in the project, with the edits each offers.
fn found(files: &[(&str, &str)]) -> Vec<jadpo_diag::Diagnostic> {
    let db = Database::default();
    let project = support::project(&db, Layout::Project, files);
    jadpo_check::check(&db, project)
}

/// `path`'s text with `diagnostic`'s edits to it made.
fn edited(files: &[(&str, &str)], diagnostic: &jadpo_diag::Diagnostic, path: &str) -> String {
    let text = files
        .iter()
        .find(|(p, _)| *p == path)
        .map(|(_, text)| *text)
        .expect("the file");
    let edits = diagnostic
        .next
        .as_ref()
        .map(|next| {
            next.edits
                .iter()
                .filter(|edit| edit.span.path == path)
                .map(|edit| (edit.span.start, edit.span.end, edit.replacement.clone()))
                .collect()
        })
        .unwrap_or_default();
    jadpo_check::apply_edits(text, edits)
}

const AUTHENTICATION: (&str, &str) = (
    "authentication.jadpo",
    "authentication {\n    browser_session: {\n        cookie: \"s\"\n    }\n}\n",
);

/// A library app whose domain folder is `library`, a reserved word.
fn library_domain(policy: &'static str) -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "app.jadpo",
            "use library/entities { Member }\n\napplication {}\n\nprincipal {\n    user: {\n        subject: Member.handle\n    }\n}\n",
        ),
        AUTHENTICATION,
        (
            "domains/library/types/handle.jadpo",
            "/// A handle.\ntype Handle = Text {\n    max_length: 40\n}\n",
        ),
        (
            "domains/library/entities/member.jadpo",
            "use library/types { Handle }\n\n/// Someone.\nentity Member {\n    id: Uuid\n    handle: Handle {\n        unique: true\n    }\n\n    policy: {\n        create: [Access.public]\n        read: [id]\n    }\n}\n",
        ),
        (
            "domains/library/actions/borrow.jadpo",
            "/// Borrows.\npublic action borrow() -> Unit {}\n",
        ),
        (
            "domains/library/actions/renew.jadpo",
            "/// Renews.\naction renew() -> Unit {}\n",
        ),
        (
            "domains/desk/actions/lend.jadpo",
            "use library\n\n/// Lends through the library.\naction lend() -> Unit {\n    library.borrow()\n}\n",
        ),
        ("policy.jadpo", policy),
    ]
}

#[test]
fn a_domain_named_with_a_reserved_word_is_refused_at_its_folder() {
    let files = library_domain(
        "use library\n\npolicy {\n    actions: {\n        library.borrow: [Access.authenticated]\n        library.renew: [Access.authenticated]\n    }\n}\n",
    );
    let found = found(&files);
    let shown: Vec<String> = found
        .iter()
        .map(|d| {
            let path = d.primary.as_ref().map_or("", |l| l.span.path.as_str());
            format!("{path} {}: {}", d.rule.id, d.message)
        })
        .collect();
    assert_eq!(
        shown,
        [
            "domains/library/actions/borrow.jadpo syntax.expected_name: `library` is a reserved word and can't name a domain"
        ],
        "one diagnostic, at the folder, and nothing where the domain is used"
    );
    let next = found[0].next.as_ref().expect("a next step");
    assert!(
        next.text.contains("domains/library/"),
        "says which folder to rename: {}",
        next.text
    );
}

/// The `cascade-bare-policy` shape: actions listed in `policy.jadpo` by
/// their bare names, imported as a file in the domain would.
fn bare_policy(policy: &'static str) -> Vec<(&'static str, &'static str)> {
    vec![
        support::APP,
        (
            "domains/shop/actions/borrow.jadpo",
            "/// Borrows.\naction borrow() -> Unit {}\n",
        ),
        (
            "domains/shop/actions/lend.jadpo",
            "/// Lends.\naction lend() -> Unit {}\n",
        ),
        ("policy.jadpo", policy),
    ]
}

#[test]
fn a_bare_action_in_policy_says_to_name_its_domain() {
    let policy = "use shop/actions { borrow, lend }\n\npolicy {\n    actions: {\n        borrow: [Access.public]\n        lend: [Access.public]\n    }\n}\n";
    let files = bare_policy(policy);
    let found = found(&files);
    let shown: Vec<String> = found
        .iter()
        .map(|d| format!("{}: {}", d.rule.id, d.message))
        .collect();
    assert_eq!(
        shown,
        [
            "policy.binding_invalid: `borrow` belongs to domain `shop`",
            "policy.binding_invalid: `lend` belongs to domain `shop`",
        ],
        "one per line, and the import that brought them in isn't also unused"
    );
    let fixed = edited(&files, &found[0], "policy.jadpo");
    assert!(
        fixed.contains("        shop.borrow: [Access.public]\n"),
        "the edit names the domain: {fixed}"
    );
    // With both edits made and the imports put right, the project checks.
    let both = edited(
        &[("policy.jadpo", fixed.as_str())],
        &found[1],
        "policy.jadpo",
    );
    let both = both.replace("use shop/actions { borrow, lend }", "use shop");
    let leaked: &'static str = Box::leak(both.into_boxed_str());
    assert_eq!(
        problems(Layout::Project, &bare_policy(leaked)),
        Vec::<String>::new()
    );
}

#[test]
fn an_unknown_bare_action_in_policy_is_still_no_action() {
    let found = found(&bare_policy(
        "policy {\n    actions: {\n        borow: [Access.public]\n    }\n}\n",
    ));
    let shown: Vec<String> = found
        .iter()
        .map(|d| format!("{}: {}", d.rule.id, d.message))
        .collect();
    assert_eq!(shown, ["policy.binding_invalid: `borow` isn't an action"]);
}

#[test]
fn some_called_like_a_function_says_an_optional_takes_the_value() {
    let files = [
        support::APP,
        (
            "domains/shop/types/title.jadpo",
            "/// A title.\ntype Title = Text {\n    max_length: 40\n}\n",
        ),
        (
            "domains/shop/actions/pick_title.jadpo",
            "use shop/types { Title }\n\n/// Picks a title.\naction pick_title(title: Title) -> Title? {\n    return some(title)\n}\n",
        ),
    ];
    let found = found(&files);
    let shown: Vec<String> = found
        .iter()
        .map(|d| format!("{}: {}", d.rule.id, d.message))
        .collect();
    assert_eq!(
        shown,
        ["semantic.not_callable: `some` is a match pattern and can't be called"]
    );
    let fixed = edited(&files, &found[0], "domains/shop/actions/pick_title.jadpo");
    assert!(
        fixed.contains("    return title\n"),
        "the edit passes the value itself: {fixed}"
    );
}

/// The `promotion-collision` shape: `ListBooks`, written beside query
/// `list_books`, is used by another query too, and its own file would be
/// `list_books.jadpo`, which `list_books` already folds to.
fn promotion() -> Vec<(&'static str, &'static str)> {
    vec![
        support::APP,
        (
            "domains/shop/entities/book.jadpo",
            "/// A book.\nentity Book {\n    id: Uuid\n    title: Text\n\n    policy: {\n        read: [Access.public]\n    }\n}\n",
        ),
        (
            "domains/shop/queries/list_books.jadpo",
            "use shop/entities { Book }\n\n/// A page request.\ntype ListBooks = Input {\n    page_size: PageSize {\n        default: 25\n    }\n}\n\n/// Books by title.\nquery list_books(filter: ListBooks) -> List<Book> {\n    from: Book\n    order: [Book.title asc]\n    limit: 25\n}\n",
        ),
        (
            "domains/shop/queries/catalogue.jadpo",
            "use shop/entities { Book }\nuse shop/queries { ListBooks }\n\n/// Everyone's view.\nquery catalogue(filter: ListBooks) -> List<Book> {\n    from: Book\n    order: [Book.title desc]\n    limit: 25\n}\n",
        ),
    ]
}

#[test]
fn a_promotion_onto_a_name_the_file_folds_to_says_which_to_rename_first() {
    let found = found(&promotion());
    let promotions: Vec<&jadpo_diag::Diagnostic> = found
        .iter()
        .filter(|d| d.rule.id == "layout.promotion_required")
        .collect();
    assert_eq!(promotions.len(), 1, "{found:#?}");
    let next = promotions[0].next.as_ref().expect("a next step");
    assert!(
        next.edits.is_empty(),
        "no move that `jadpo fmt --layout` would make: {:#?}",
        next.edits
    );
    assert!(
        next.text.contains("Rename `ListBooks`") && next.text.contains("`list_books`"),
        "says which to rename first: {}",
        next.text
    );
}
