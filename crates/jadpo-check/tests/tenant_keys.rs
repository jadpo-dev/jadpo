//! A unique rule on a tenant's records holds within its tenant (area 9,
//! "A unique rule holds within its tenant"; chunk 10.3a): each key's
//! columns end with the tenancy reference's, unless the rule holds it
//! already, while an entity every tenant shares and the tenant itself
//! keep one key. A key so widened finds one record only with the tenancy,
//! or a reference to another of the tenant's records, tested too.

use jadpo_db::{Database, Layout};

use crate::support;

const BASE: &str = r#"principal {
    user: {
        subject: Person.handle
    }
}

authentication {
    api_token: {}
}

policy {
    tenant: Company
}

type Label = Text {
    max_length: 80
}

type Role = Enum {
    owner
    viewer
}

entity Company {
    id: Uuid
    slug: Label {
        unique: true
    }

    policy: {
        read: [Role.owner, Role.viewer]
    }
}

entity Membership {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    person_id: Person.id {
        on_delete: OnDelete.cascade
    }
    role: Role

    membership: {
        scope: company_id
        member: person_id
        role: role
    }

    policy: {
        read: [Role.owner, person_id]
    }
}

entity Project {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    name: Label {
        unique: true
    }
    code: Label

    unique: {
        code_per_company: [company_id, code]
    }

    policy: {
        read: [Role.owner, Role.viewer]
    }
}

entity Step {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    project_id: Project.id {
        on_delete: OnDelete.cascade
    }
    title: Label

    unique: {
        title_per_project: [project_id, title]
    }

    policy: {
        read: [Role.owner, Role.viewer]
    }
}
"#;

const PERSON: &str = r#"
type Handle = Text {
    max_length: 40
}

entity Person {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        read: [id]
        tenant: Tenancy.shared
    }
}
"#;

fn source(rest: &str) -> String {
    format!("{BASE}{PERSON}{rest}")
}

#[test]
fn a_unique_rule_on_a_tenants_records_holds_within_the_tenant() {
    let db = Database::default();
    let text = source("");
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", text.as_str())]);
    let schema = jadpo_check::data::schema(&db, project);
    let tables = &schema.stores["main"].tables;
    let key = |table: &str, name: &str| tables[table].unique[name].columns.clone();
    // The tenancy column is added last, and a rule holding it already
    // keeps its fields.
    assert_eq!(key("project", "project_name_key"), ["name", "company_id"]);
    assert_eq!(
        key("project", "project_code_per_company_key"),
        ["company_id", "code"]
    );
    assert_eq!(
        key("step", "step_title_per_project_key"),
        ["project_id", "title", "company_id"]
    );
    // An entity every tenant shares, and the tenant, keep one key.
    assert_eq!(key("person", "person_handle_key"), ["handle"]);
    assert_eq!(key("company", "company_slug_key"), ["slug"]);
}

/// The problems `rest`, beside the base, gives.
fn problems(rest: &str) -> Vec<String> {
    let text = source(rest);
    support::problems(Layout::Loose, &[("main.jadpo", text.as_str())])
}

#[test]
fn a_query_for_one_record_tests_the_tenancy_beside_a_unique_field() {
    let alone = problems(
        r#"
query project_named(name: Label) -> Project? {
    from: Project
    where: Project.name == name
}
"#,
    );
    assert_eq!(
        alone,
        ["main.jadpo query.not_unique: A query returning `Project?` could match several records"]
    );
    let text = source(
        r#"
query project_named(name: Label) -> Project? {
    from: Project
    where: Project.name == name
}
"#,
    );
    let db = Database::default();
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", text.as_str())]);
    let reason = jadpo_check::check(&db, project)
        .iter()
        .find_map(|d| d.reason.clone())
        .unwrap_or_default();
    assert!(
        reason.contains("`Project.name` with `Project.company_id`")
            && reason.contains("and `code_per_company`'s fields."),
        "{reason}"
    );
    // With the tenancy, or through a reference to another of the tenant's
    // records, which names the tenant already, one record at most.
    assert_eq!(
        problems(
            r#"
query project_named(company_id: Company.id, name: Label) -> Project? {
    from: Project
    where: Project.company_id == company_id and Project.name == name
}

query step_titled(project_id: Project.id, title: Label) -> Step? {
    from: Step
    where: Step.project_id == project_id and Step.title == title
}

query company_at(slug: Label) -> Company? {
    from: Company
    where: Company.slug == slug
}
"#
        ),
        Vec::<String>::new()
    );
}

#[test]
fn a_unique_field_last_decides_no_tie_across_tenants() {
    let db = Database::default();
    let text = source(
        r#"
type ProjectCard = Output {
    pick: [Project.id, Project.name]
}

type PersonCard = Output {
    pick: [Person.id, Person.handle]
}

query projects() -> List<ProjectCard> {
    from: Project
    order: [Project.name asc]
    limit: 50
}

query people() -> List<PersonCard> {
    from: Person
    order: [Person.handle asc]
    limit: 50
}
"#,
    );
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", text.as_str())]);
    let queries = jadpo_check::data::queries::query_models(&db, project);
    let order = |name: &str| -> Vec<String> {
        queries
            .queries
            .iter()
            .find(|query| query.name == name)
            .expect("the query")
            .order
            .iter()
            .map(|field| field.field.clone())
            .collect()
    };
    assert_eq!(order("projects"), ["name", "id"]);
    assert_eq!(order("people"), ["handle"]);
}

#[test]
fn a_principal_whose_records_belong_to_the_tenant_is_found_by_no_subject() {
    let text = format!(
        "{BASE}{}",
        r#"
type Handle = Text {
    max_length: 40
}

entity Person {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    handle: Handle {
        unique: true
    }

    policy: {
        read: [id]
    }
}
"#
    );
    let found = support::problems(Layout::Loose, &[("main.jadpo", text.as_str())]);
    assert!(
        found.contains(
            &"main.jadpo type.principal_subject_not_unique: `Person.handle` doesn't identify one user"
                .to_string()
        ),
        "{found:?}"
    );
}
