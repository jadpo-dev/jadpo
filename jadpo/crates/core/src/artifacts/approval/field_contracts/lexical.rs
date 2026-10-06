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
        let Declaration::Callable(callable) = declaration else {
            return Self::default();
        };
        let parameters = callable
            .parameters
            .iter()
            .enumerate()
            .map(|(index, parameter)| {
                (
                    parameter.name.text.clone(),
                    json!({"kind":"parameter","name":parameter.name.text,
                "address":["parameter",index.to_string()],"owner":owner}),
                )
            })
            .collect();
        let mut scan = Scan {
            owner: owner.clone(),
            scopes: vec![parameters],
            links: Self::default(),
        };
        scan.block(&callable.body, &["body".into()]);
        scan.links
    }
    pub(super) fn expression(&self, name: &NameExpression) -> Value {
        self.uses.get(&(name.range.start, name.range.end)).cloned().unwrap_or_else(|| json!({
            "status":"unresolved","reason":"expression_scope_not_analyzed",
            "value_origin":"not_resolved","identity":"owner_and_build_local_structural_address_not_insertion_stable"}))
    }
}
impl Scan {
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
            Expression::Create(_)
            | Expression::Query(_)
            | Expression::Update(_)
            | Expression::Delete(_)
            | Expression::Literal(_)
            | Expression::Missing(_) => {}
        }
    }
}
