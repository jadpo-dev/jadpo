//! Area 1, "Files and names" (chunk 5.3d): two domains each with a private
//! type and helper of one name run their own, since each key carries its
//! domain (area 18, "Stable identity").

use jadpo_db::{Database, File, Layout, Project};
use jadpo_interp::Program;

use crate::support::{text, value};

const FILES: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "shared/types/label.jadpo",
        "type Label = Text {\n    max_length: 200\n}\n",
    ),
    (
        "domains/billing/actions/send_invoice.jadpo",
        "use shared/types { Label }\n\naction send_invoice() -> Label {\n    return format_address(Address {\n        street: \"High Street\"\n    })\n}\n\nfunction format_address(address: Address) -> Label {\n    return address.street\n}\n\ntype Address = Object {\n    street: Label\n}\n",
    ),
    (
        "domains/shipping/actions/ship_parcel.jadpo",
        "use shared/types { Label }\n\naction ship_parcel() -> Label {\n    return format_address(Address {\n        line_one: \"1 High Street\"\n        postcode: \"AB1 2CD\"\n    })\n}\n\nfunction format_address(address: Address) -> Label {\n    return address.postcode\n}\n\ntype Address = Object {\n    line_one: Label\n    postcode: Label\n}\n",
    ),
];

fn program() -> Program {
    let db = Database::default();
    let inputs: Vec<File> = FILES
        .iter()
        .map(|(path, text)| File::new(&db, path.to_string(), text.to_string()))
        .collect();
    let project = Project::new(
        &db,
        inputs,
        Vec::new(),
        Layout::Project,
        Default::default(),
        Vec::new(),
    );
    match Program::load(&db, project) {
        Ok(program) => program,
        Err(errors) => panic!(
            "the example doesn't check: {:#?}",
            errors
                .iter()
                .map(|d| format!("{}: {}", d.rule.id, d.message))
                .collect::<Vec<_>>()
        ),
    }
}

#[test]
fn each_domain_runs_its_own_private_helper() {
    let program = program();
    assert_eq!(
        value(&program, "send_invoice", Vec::new()),
        text("High Street")
    );
    assert_eq!(value(&program, "ship_parcel", Vec::new()), text("AB1 2CD"));
}
