//! Declaration/use links only. No alias value expansion, origin or flow proof.
use super::{child, contract_id, Value};
use jadpo_syntax::*;
use serde_json::json;
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Links {
    uses: BTreeMap<(usize, usize), Value>,
}
struct Scan {
    owner: Value,
    scopes: Vec<BTreeMap<String, Value>>,
    links: Links,
}
impl Links {
    pub(super) fn derive(declaration: &Declaration, owner: &Value) -> Self {
        let mut scan = Scan {
            owner: owner.clone(),
            scopes: vec![BTreeMap::new()],
            links: Self::default(),
        };
        match declaration {
            Declaration::Callable(callable) => {
                for (index, parameter) in callable.parameters.iter().enumerate() {
                    scan.scopes[0].insert(
                        parameter.name.text.clone(),
                        json!({
                        "kind":"parameter","name":parameter.name.text,
                        "address":["parameter",index.to_string()],"owner":owner}),
                    );
                }
                scan.block(&callable.body, &["body".into()]);
            }
            Declaration::Route(route) => {
                // These are precisely the authored route wrappers inserted by
                // the checker, not runtime principal/clock/base-environment data.
                for name in ["input", "query"] {
                    let present = if name == "input" {
                        route.input.is_some()
                    } else {
                        route.query.is_some()
                    };
                    if present {
                        scan.route_transport(name, BTreeMap::new());
                    }
                }
                for (name, fields) in [
                    (
                        "path",
                        route
                            .path_fields
                            .iter()
                            .map(|field| field.name.text.as_str())
                            .collect::<Vec<_>>(),
                    ),
                    (
                        "headers",
                        route
                            .headers
                            .iter()
                            .map(|field| field.name.text.as_str())
                            .collect::<Vec<_>>(),
                    ),
                ] {
                    if !fields.is_empty() {
                        let members = fields
                            .into_iter()
                            .map(|field| {
                                (
                                    field.to_owned(),
                                    json!({
                            "kind":"authored_route_transport_field","name":field,"owner":owner,
                            "address":["route",name,"field",field]}),
                                )
                            })
                            .collect();
                        scan.route_transport(name, members);
                    }
                }
                if let Some(run) = &route.run {
                    scan.expr(&Expression::Invocation(run.clone()));
                }
                if let Some(action) = &route.inline_action {
                    scan.block(&action.body, &["body".into()]);
                }
            }
            _ => return Self::default(),
        }
        scan.links
    }
    pub(super) fn expression(&self, name: &NameExpression) -> Value {
        self.uses.get(&(name.range.start, name.range.end)).cloned().unwrap_or_else(|| json!({
            "status":"unresolved","reason":"expression_scope_not_analyzed",
            "value_origin":"not_resolved","identity":"owner_and_build_local_structural_address_not_insertion_stable"}))
    }
}
impl Scan {
    fn route_transport(&mut self, name: &str, members: BTreeMap<String, Value>) {
        self.scopes[0].insert(
            name.to_owned(),
            json!({
            "kind":"authored_route_transport","name":name,"owner":self.owner,
            "address":["route",name],"members":members}),
        );
    }
    fn block(&mut self, block: &Block, address: &[String]) {
        self.scopes.push(BTreeMap::new());
        for (index, statement) in block.statements.iter().enumerate() {
            match statement {
                Statement::Binding(binding) => {
                    // Initializer observes the outer declaration before shadowing.
                    self.expr(&binding.value);
                    let address = child(
                        address,
                        ["statement".into(), index.to_string(), "binding".into()],
                    );
                    let declaration = json!({"kind":if binding.mutable {"mutable_local"} else {"immutable_local"},
                        "name":binding.name.text,"owner":self.owner,"address":address,
                        "contract_id":contract_id("binding",&self.owner,&address)});
                    self.scopes
                        .last_mut()
                        .unwrap()
                        .insert(binding.name.text.clone(), declaration);
                }
                Statement::Assignment(assignment) => {
                    self.expr(&assignment.value);
                }
                Statement::Return(value) => self.expr(&value.value),
                Statement::Assert(value) => self.expr(&value.condition),
                Statement::AdvanceClock(value) => self.expr(&value.duration),
                Statement::Reject(value) => {
                    for field in &value.values {
                        self.expr(&field.value);
                    }
                }
                Statement::If(value) => {
                    self.expr(&value.condition);
                    let address = child(
                        address,
                        ["statement".into(), index.to_string(), "if".into()],
                    );
                    self.block(&value.then_block, &child(&address, ["then".into()]));
                    if let Some(block) = &value.else_block {
                        self.block(block, &child(&address, ["else".into()]));
                    }
                }
                // Pattern binders and persistence row scopes need separate
                // checked extraction. Never fall through to an outer namesake.
                Statement::Match(value) => self.expr(&value.subject),
                Statement::Unsupported(_) => {}
            }
        }
        self.scopes.pop();
    }
    fn expr(&mut self, expression: &Expression) {
        match expression {
            Expression::Name(name) => {
                let root = name
                    .path
                    .first()
                    .map(|name| name.text.as_str())
                    .unwrap_or_default();
                let declaration = self.scopes.iter().rev().find_map(|scope| scope.get(root));
                let link = match declaration {
                    Some(declaration) if declaration["kind"] == "authored_route_transport" => {
                        json!({
                        "status":"linked_route_transport_source_use",
                        "evidence":"checked_authored_route_declaration_not_runtime_value",
                        "declaration":declaration,"access_path":name.path.iter().skip(1).map(|name|name.text.as_str()).collect::<Vec<_>>(),
                        "member_declaration":name.path.get(1).and_then(|field|declaration["members"].get(&field.text)),
                        "value_origin":"not_resolved","alias_value_expansion":"not_performed",
                        "branch_execution":"not_proved","caller_identity":"not_established",
                        "identity":"owner_and_build_local_structural_address_not_insertion_stable"})
                    }
                    Some(declaration) if declaration["kind"] != "mutable_local" => json!({
                        "status":"linked_immutable_source_use","evidence":"checked_lexical_declaration_not_value_origin",
                        "declaration":declaration,"access_path":name.path.iter().skip(1).map(|name|name.text.as_str()).collect::<Vec<_>>(),
                        "value_origin":"not_resolved","alias_value_expansion":"not_performed",
                        "branch_execution":"not_proved","identity":"owner_and_build_local_structural_address_not_insertion_stable"}),
                    Some(declaration) => {
                        json!({"status":"unresolved","reason":"mutable_binding_value_not_linked",
                        "candidate_declaration":declaration,"value_origin":"not_resolved"})
                    }
                    None => {
                        json!({"status":"unresolved","reason":"name_not_in_supported_lexical_environment","value_origin":"not_resolved"})
                    }
                };
                self.links
                    .uses
                    .insert((name.range.start, name.range.end), link);
            }
            Expression::Invocation(value) => {
                for expression in &value.arguments {
                    self.expr(expression);
                }
                for field in &value.named_arguments {
                    self.expr(&field.value);
                }
            }
            Expression::TestCall(value) => {
                self.expr(&Expression::Invocation(value.invocation.clone()))
            }
            Expression::Construction(value) => {
                for field in &value.fields {
                    self.expr(&field.value);
                }
            }
            Expression::Object(value) => {
                for field in &value.fields {
                    self.expr(&field.value);
                }
            }
            Expression::Attempt(value) => self.expr(&value.value),
            Expression::Unary(value) => self.expr(&value.value),
            Expression::Binary(value) => {
                self.expr(&value.left);
                self.expr(&value.right);
            }
            Expression::Grouped(value) => self.expr(&value.value),
            Expression::OutcomeMatch(value) => self.expr(&value.subject),
            Expression::Create(value) => {
                // Explicit field values use the existing lexical environment.
                // Conflict/rejection scopes are deliberately not traversed.
                for field in &value.fields {
                    self.expr(&field.value);
                }
            }
            Expression::Query(value) => {
                // The predicate field is an entity field Name, not a lexical
                // expression. Ordinary point and many-query RHS values use the
                // outer environment; pagination remains checked literals only.
                let ordinary_point = matches!(
                    value.cardinality,
                    QueryCardinality::Optional | QueryCardinality::Required
                ) && value.pagination.is_none();
                if (ordinary_point || value.cardinality == QueryCardinality::Many)
                    && value.page.is_none()
                    && value.includes.is_empty()
                {
                    self.expr(&value.value);
                }
            }
            Expression::Update(_)
            | Expression::Delete(_)
            | Expression::Literal(_)
            | Expression::Missing(_) => {}
        }
    }
}
